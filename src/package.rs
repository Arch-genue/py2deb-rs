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
    maintainer: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
}
fn default_arch() -> Architecture {
    Architecture::All
}

impl Package {
    pub fn new(package: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            package: package.into(),
            version: version.into(),
            arch: Architecture::All,
            maintainer: None,
            dependencies: Vec::new(),
        }
    }
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} {}", "Package:".bold(), self.package.bold().green())?;
        writeln!(f, "Version: {}", self.version.blue())?;
        writeln!(
            f,
            "Maintainer: {}",
            self.maintainer.as_deref().unwrap_or("Unknown")
        )?;
        writeln!(f, "Architecture: {}", self.arch)?;
        writeln!(f, "Dependencies: {}", self.dependencies.join(", "))?;
        Ok(())
    }
}
