use std::path::{Path, PathBuf};
use std::fs;
use std::os::unix::fs::PermissionsExt;

use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};

use crate::package::Package;


#[derive(Debug, Clone, Serialize)]
pub struct IncludeEntry {
    pub src: String,
    pub dest: String,
    /// Explicit file mode from the config, if the entry gave a third element.
    /// `None` means "take whatever the file has on disk" — see
    /// [`IncludeEntry::resolve_mode`].
    pub mode: Option<u32>,
}

impl<'de> Deserialize<'de> for IncludeEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        let parts = Vec::<String>::deserialize(d)?;
        let (src, dest, mode) = match parts.as_slice() {
            [src, dest] => (src, dest, None),
            [src, dest, mode] => (src, dest, Some(mode.as_str())),
            other => {
                return Err(D::Error::custom(format!(
                    "include entry needs [source, dest] or [source, dest, mode], got {} elements",
                    other.len()
                )));
            }
        };

        // A mode is written the way chmod takes it: "0644", octal, not decimal.
        let mode = mode
            .map(|m| {
                u32::from_str_radix(m.trim_start_matches("0o"), 8)
                    .map_err(|_| D::Error::custom(format!("`{m}` is not an octal file mode")))
            })
            .transpose()?;

        Ok(Self { src: src.clone(), dest: dest.clone(), mode })
    }
}

impl IncludeEntry {
    /// The mode this entry should be archived with.
    ///
    /// An explicit mode from the config always wins. Otherwise the source
    /// file's own permissions are used verbatim, minus the file-type bits —
    /// the same rule `cargo-deb` applies, so a binary that is executable in
    /// the working tree stays executable in the package.
    pub fn resolve_mode(&self, source: &Path) -> Result<u32> {
        if let Some(mode) = self.mode {
            return Ok(mode);
        }

        let metadata = fs::metadata(source)
            .with_context(|| format!("cannot read permissions of {}", source.display()))?;
        Ok(metadata.permissions().mode() & 0o7777)
    }

    /// Where this entry lands inside the package.
    ///
    /// `$name` placeholders are expanded from `package` first, so a dest of
    /// `usr/share/$package/` follows the package name without repeating it.
    /// A dest ending in `/` then names a directory and keeps the source file
    /// name, exactly as `cp` would; anything else is the full target path and
    /// renames the file.
    pub fn resolve_dest(&self, source: &Path, package: &Package) -> Result<PathBuf> {
        let dest = package.expand_dollar_properties(&self.dest);

        if !dest.ends_with('/') {
            return Ok(PathBuf::from(dest));
        }

        let name = source
            .file_name()
            .with_context(|| format!("include source {} has no file name", source.display()))?;
        Ok(Path::new(&dest).join(name))
    }
}