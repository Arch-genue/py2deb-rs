use std::fmt;

use super::architecture::Architecture;
use colored::Colorize;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    package: String,
    version: String,
    #[serde(default = "default_arch")]
    arch: Architecture,
    #[serde(default)]
    maintainer: String,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    pub src: String,
    pub dest: Option<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    description: String,
    #[serde(default)]
    section: String,
    #[serde(default)]
    priority: String,
    #[serde(default)]
    pub installed_size: u64
}
fn default_arch() -> Architecture {
    Architecture::All
}

impl Package {
    pub fn new(package: impl Into<String>, version: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            package: package.into(),
            version: version.into(),
            arch: Architecture::All,
            maintainer: "Unknown".into(),
            dependencies: Vec::new(),
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
    pub fn save_string(&mut self) -> String {
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

        if !self.dependencies.is_empty() {
            control_str.push_str(format!("Depends: python3:any, {}\n", self.dependencies.join(", ")).as_str());
        } else {
            control_str.push_str("Depends: python3:any\n");
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
        writeln!(f, "Dependencies: {}", self.dependencies.join(", "))?;
        Ok(())
    }
}
