use std::{
    path::Path,
    fmt,
    fs
};

use super::architecture::Architecture;
use super::include_entry::IncludeEntry;

use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    /// The name of the Debian package
    #[serde(deserialize_with = "validate_package_name")]
    pub package: String,
    /// Package version
    pub version: String,

    #[serde(deserialize_with = "validate_maintainer")]
    /// Maintainer
    pub maintainer: String,
    #[serde(default)]
    /// To whom and when the copyright of the software is granted. 
    /// If not present, the maintainer is used.
    pub copyright: String,
    #[serde(default)]
    /// Project URL, written to the copyright file's `Source:` field so the
    /// package points back at where the code came from.
    pub homepage: String,
    #[serde(default)]
    /// Short licence name for the copyright file, e.g. `GPL-3`, `MIT`,
    /// `Apache-2.0`. Debian's own spellings, which are the file names under
    /// `/usr/share/common-licenses`. Defaults to `GPL-3`.
    pub license: String,
    #[serde(default)]
    /// Location of the license file
    /// If not present, GPL-3.0 will be used.
    pub license_file: String,
    #[serde(default)]
    /// Amount of lines to skip at the top of license file. Default 0
    pub license_file_skip_lines: u32,

    #[serde(default = "default_arch")]
    /// Target architecture, default = all 
    arch: Architecture, // TODO!!! Check for binary files in project
    
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

    #[serde(default)]
    /// Package description.. or use can use description_file below.
    /// README.md is used if it is not provided
    description: String,
    #[serde(default)]
    /// Package description
    description_file: String,

    #[serde(default)]
    /// Dpkg section
    section: String,
    #[serde(default)]
    /// Dpkg priority (//TODO!! VALIDATION, required, optional)
    priority: String,
    #[serde(default)]
    /// Changelog file path (relative!)
    pub changelog: String,

    // Runtime fields here
    #[serde(default, skip_serializing)]
    /// Dpkg installed size, update automatically
    pub installed_size: u64,
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

impl Package {
    pub fn new(package: impl Into<String>, version: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            package: package.into(),
            version: version.into(),
            
            maintainer: "Name <user@example.com>".into(),
            copyright: "".into(),
            homepage: "".into(),
            license: "GPL-3".into(),
            license_file: "".into(),
            license_file_skip_lines: 0,

            arch: Architecture::All,
            depends: Vec::new(),
            conflicts: Vec::new(),
            src: "src".into(),
            dest: None,
            include: Vec::new(),
            exclude: Vec::new(),
            description: description.into(),
            description_file: "README.md".into(),
            section: "".into(),
            priority: "optional".into(),
            changelog: "".into(),
            installed_size: 0,
        }
    }
    pub fn from_config(path: &Path) -> Result<Self> {
        let config_toml = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let value: toml::Value = toml::from_str(&config_toml)
            .with_context(|| format!("{} is not valid TOML", path.display()))?;

        let section = value.get("tool").and_then(|t| t.get("py2deb"))
            .with_context(|| "Cannot find [tool.py2deb] section. Init project first".to_string())?;

        section.clone().try_into()
            .with_context(|| format!("Invalid [tool.py2deb] section in {}", path.display()))
    }
    pub fn get_package_file_name(&self) -> String {
        format!("{}_{}_{}.deb", self.package, self.version, self.arch)
    }
    /// The value of a field addressed by name, as `$name` in a template.
    ///
    /// Only fields worth interpolating are exposed; anything else is left
    /// alone by [`Package::expand_dollar_properties`] so an unrelated `$`
    /// in the text survives untouched.
    pub fn get_dollar_property(&self, name: &str) -> Option<String> {
        Some(match name {
            "package" => self.package.clone(),
            "version" => self.version.clone(),
            "arch" | "architecture" => self.arch.to_string(),
            "maintainer" => self.maintainer.clone(),
            "description" => self.description.clone(),
            "section" => self.section.clone(),
            "priority" => self.priority.clone(),
            "installed_size" => self.installed_size.to_string(),
            _ => return None,
        })
    }

    /// Replaces every `$name` in `template` with that field's value.
    ///
    /// A name runs until the first character that cannot be part of one, so
    /// `$package_1.0` reads as `$package` followed by `_1.0`. `$$` is a
    /// literal dollar sign, and an unknown `$name` is left as written rather
    /// than silently becoming empty.
    pub fn expand_dollar_properties(&self, template: &str) -> String {
        let mut out = String::with_capacity(template.len());
        let mut chars = template.chars().peekable();

        while let Some(c) = chars.next() {
            if c != '$' {
                out.push(c);
                continue;
            }

            if chars.peek() == Some(&'$') {
                chars.next();
                out.push('$');
                continue;
            }

            let mut name = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    name.push(next);
                    chars.next();
                } else {
                    break;
                }
            }

            match self.get_dollar_property(&name) {
                Some(value) => out.push_str(&value),
                None => {
                    out.push('$');
                    out.push_str(&name);
                }
            }
        }

        out
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

#[cfg(test)]
mod dollar_tests {
    use super::*;

    fn pkg() -> Package {
        Package::new("python3-libgkeyboard", "1.1.8", "GKeyboard library")
    }

    #[test]
    fn substitutes_known_fields() {
        let p = pkg();
        assert_eq!(p.expand_dollar_properties("$package v$version"), "python3-libgkeyboard v1.1.8");
    }

    #[test]
    fn leaves_unknown_names_alone() {
        let p = pkg();
        assert_eq!(p.expand_dollar_properties("cost is $nonexistent"), "cost is $nonexistent");
    }

    #[test]
    fn double_dollar_is_a_literal() {
        let p = pkg();
        assert_eq!(p.expand_dollar_properties("$$5 for $package"), "$5 for python3-libgkeyboard");
    }

    #[test]
    fn name_stops_at_punctuation() {
        let p = pkg();
        assert_eq!(p.expand_dollar_properties("$package-doc"), "python3-libgkeyboard-doc");
        assert_eq!(p.expand_dollar_properties("v$version, released"), "v1.1.8, released");
    }
}
