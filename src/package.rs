use std::fmt;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::architecture::Architecture;
use colored::Colorize;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct IncludeEntry {
    pub src: String,
    pub dest: String,
    /// Explicit file mode from the config, if the entry gave a third element.
    /// `None` means "take whatever the file has on disk" — see
    /// [`IncludeEntry::resolve_mode`].
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    /// Deb package name
    #[serde(deserialize_with = "validate_package_name")]
    pub package: String,
    /// Package version
    version: String,

    #[serde(default = "default_arch")]
    /// Target architecture, default = all 
    arch: Architecture,
    /// Maintainer
    #[serde(deserialize_with = "validate_maintainer")]
    maintainer: String,
    #[serde(default)]
    /// Dpkg depends list
    depends: Vec<String>,
    #[serde(default)]
    /// Dpkg conflicts list
    conflicts: Vec<String>,
    #[serde(default)]
    /// Project source path
    pub src: String,
    /// Debian destination path (usr/lib/python3/dist-packages/{dest}), default is project_name
    pub dest: Option<String>,
    #[serde(default)]
    /// Include other files
    pub include: Vec<IncludeEntry>,
    #[serde(default)]
    /// Exclude files or directories with pattern
    pub exclude: Vec<String>,
    /// Package description
    description: String,
    #[serde(default)]
    /// Dpkg section
    section: String,
    #[serde(default)]
    /// Dpkg priority (//TODO!!, optional)
    priority: String,
    #[serde(default)]
    /// Dpkg installed size, update automatically
    pub installed_size: u64
}

fn validate_package_name<'de, D>(d: D) -> Result<String, D::Error>
where D: serde::Deserializer<'de> {
    use serde::de::Error;
    let raw = String::deserialize(d)?;

    if raw.len() < 2 {
        return Err(D::Error::custom("package name must be at least 2 characters"));
    }

    if !raw.chars().next().unwrap().is_ascii_alphanumeric() {
        return Err(D::Error::custom(
            format!("package name must start with a letter or digit, got `{raw}`")
        ));
    }
    if !raw.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+-.".contains(c)) {
        return Err(D::Error::custom(
            format!("package name may only certain a-z, 0-9, +, -, . - got `{raw}`")
        ));
    }

    Ok(raw)
}

fn validate_maintainer<'de, D>(d: D) -> Result<String, D::Error>
where D: serde::Deserializer<'de> {
    use serde::de::Error;
    let raw = String::deserialize(d)?;
    if raw.is_empty() {
        return Err(D::Error::custom(
            "maintainer must not be empty; dpkg requires it as `Name <you@example.com>`",
        ));
    }

    if !raw.contains('<') {
        return Err(D::Error::custom(
            format!("maintainer must include an email in angle brackets: `Name <you@example.com>` got - `{raw}`")
        ));
    }

    Ok(raw)
}

fn default_arch() -> Architecture {
    Architecture::All
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
    /// A `dest` ending in `/` names a directory and keeps the source file
    /// name, exactly as `cp` would; anything else is the full target path and
    /// renames the file.
    pub fn resolve_dest(&self, source: &Path) -> Result<PathBuf> {
        if !self.dest.ends_with('/') {
            return Ok(PathBuf::from(&self.dest));
        }

        let name = source
            .file_name()
            .with_context(|| format!("include source {} has no file name", source.display()))?;
        Ok(Path::new(&self.dest).join(name))
    }
}

impl Package {
    pub fn new(package: impl Into<String>, version: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            package: package.into(),
            version: version.into(),
            arch: Architecture::All,
            maintainer: "Unknown".into(),
            depends: Vec::new(),
            conflicts: Vec::new(),
            src: "src".into(),
            dest: None,
            include: Vec::new(),
            exclude: Vec::new(),
            description: description.into(),
            section: "".into(),
            priority: "optional".into(),
            installed_size: 0
        }
    }
    pub fn get_package_file_name(&self) -> String {
        format!("{}_{}_{}.deb", self.package, self.version, self.arch)
    }
    pub fn generate_control(&mut self) -> String {
        let mut control_str = format!("
Package: {}
Version: {}
Architecture: {}
Maintainer: {}
Installed-Size: {}
Priority: {}
Description: {}\n",
            self.package,
            self.version,
            self.arch,
            self.maintainer,
            self.installed_size,
            self.priority,
            self.description,
        );
        if !self.section.is_empty() {
            control_str.push_str(format!("Section: {}\n", self.section).as_str());
        } else {
            control_str.push_str("Section: python\n");
        }

        if !self.depends.is_empty() {
            control_str.push_str(format!("Depends: python3:any, {}\n", self.depends.join(", ")).as_str());
        } else {
            control_str.push_str("Depends: python3:any\n");
        }
        if !self.conflicts.is_empty() {
            control_str.push_str(format!("Conflicts: {}\n", self.conflicts.join(", ")).as_str());
        }

        control_str.trim_start().to_string()
    }
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} {}", "Package:".bold(), self.package.bold().green())?;
        writeln!(f, "Version: {}", self.version.blue())?;
        writeln!(f, "Maintainer: {}", self.maintainer)?;
        writeln!(f, "Architecture: {}", self.arch)?;
        if !self.depends.is_empty() {
            writeln!(f, "Dependencies: {}", self.depends.join(", "))?;
        }
        if !self.conflicts.is_empty() {
            writeln!(f, "Conflicts: {}", self.conflicts.join(", "))?;
        }
        Ok(())
    }
}
