use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::package::Package;

/// A symbolic link to create inside the package.
///
/// Written in the config the way `ln -s` reads, target first:
///
/// ```toml
/// symlinks = [
///     ["opt/gvcp.AppImage", "usr/bin/gvcp"],
/// ]
/// ```
///
/// so `usr/bin/gvcp` points at `opt/gvcp.AppImage`.
#[derive(Debug, Clone, Serialize)]
pub struct SymlinkEntry {
    /// What the link points at, as written in the config.
    pub target: String,
    /// Where the link itself is created, relative to the package root.
    pub link: String,
}

impl<'de> Deserialize<'de> for SymlinkEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        let parts = Vec::<String>::deserialize(d)?;
        let [target, link] = parts.as_slice() else {
            return Err(D::Error::custom(format!(
                "symlink entry needs [target, link], got {} elements",
                parts.len()
            )));
        };

        if link.ends_with('/') {
            return Err(D::Error::custom(format!(
                "symlink `{link}` must name the link itself, not a directory"
            )));
        }

        Ok(Self { target: target.clone(), link: link.clone() })
    }
}

impl SymlinkEntry {
    /// Where the link is created, with `$name` placeholders expanded.
    pub fn resolve_link(&self, package: &Package) -> PathBuf {
        PathBuf::from(
            package
                .expand_dollar_properties(&self.link)
                .trim_start_matches('/')
                .to_string(),
        )
    }

    /// What the link points at.
    ///
    /// Policy 10.5 asks for relative links *within* a top-level directory and
    /// absolute ones *between* them, and lintian enforces both halves
    /// (`relative-symlink`, `absolute-symlink-in-top-level-folder`). So
    /// `usr/bin/x -> usr/lib/x/run` becomes `../lib/x/run`, while
    /// `usr/bin/x -> opt/x.AppImage` becomes `/opt/x.AppImage`.
    ///
    /// A target written absolute in the config is kept that way; the author
    /// asked for it.
    pub fn resolve_target(&self, package: &Package, link: &Path) -> PathBuf {
        let target = package.expand_dollar_properties(&self.target);

        if target.starts_with('/') {
            return PathBuf::from(target);
        }

        let target = Path::new(target.trim_start_matches("./"));

        if top_level(link) == top_level(target) {
            relative_target(link, target)
        } else {
            Path::new("/").join(target)
        }
    }
}

/// The first path component, which is what Policy 10.5 compares.
fn top_level(path: &Path) -> Option<std::path::Component<'_>> {
    path.components().next()
}

/// Expresses `target` relative to the directory containing `link`.
///
/// Both are package-root-relative, so the shared leading directories cancel
/// out and whatever remains of the link's path becomes `..` hops.
fn relative_target(link: &Path, target: &Path) -> PathBuf {
    let link_dir: Vec<_> = link.parent().unwrap_or(Path::new("")).components().collect();
    let target_parts: Vec<_> = target.components().collect();

    let shared = link_dir
        .iter()
        .zip(&target_parts)
        .take_while(|(a, b)| a == b)
        .count();

    let mut out = PathBuf::new();
    for _ in shared..link_dir.len() {
        out.push("..");
    }
    for part in &target_parts[shared..] {
        out.push(part);
    }

    // A link beside its target has nothing left to build a path from.
    if out.as_os_str().is_empty() {
        out.push(target);
    }

    out
}
