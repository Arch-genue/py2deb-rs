use std::path::{Path, PathBuf};
use std::env;
use std::fs;

use clap::{Parser, Subcommand};
use anyhow::{Context, Result, bail};
use serde::{Serialize, Deserialize};
use toml::{Table, Value};
use colored::Colorize;

mod architecture;
use architecture::Architecture;

mod package;
use package::Package;

mod deb;
use deb::build::DebianBuild;

#[derive(Parser)]
#[command(version, about)]
struct CliArgs {
    #[arg(long, value_name="PATH")]
    path: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        name: Option<String>
    },
    Build {

    }
}

fn main() -> Result<()> {
    let cli = CliArgs::parse();

    let mut current_path = env::current_dir()?;
    if let Some(project_path) = cli.path.as_deref() {
        let p_path = shellexpand::tilde(project_path);
        current_path = PathBuf::from(p_path.as_ref());
    }
    let config_path = current_path.join("pyproject.toml");
    println!("{}", current_path.display());

    match cli.command {
        Some(Commands::Init {name: Some(name)}) => {
            println!("Project name {}", name);

            current_path.push(name);
            println!("Project directory is {}", current_path.display());
        },
        Some(Commands::Init {name : None}) => {
            let filename = current_path.file_name().and_then(|f| f.to_str()).context("Parent path has no file name")?;
            let is_exists: bool = config_path.exists();
            if !is_exists {
                fs::File::create(&config_path)?;
            }

            let config_toml = fs::read_to_string(&config_path).with_context(|| format!("Failed to read {}", config_path.display()))?;
            let value: toml::Value = toml::from_str(&config_toml)?;
            if let Some(section) = value.get("tool").and_then(|t| t.get("py2deb")) {
                let package: Package = section.clone().try_into()?;
                eprintln!("Cannot init new project. Current project: \n");
                println!("{}", package);
                return Ok(());
            }

            let mut table: Table = config_toml.parse()?;
            let mut tool = Table::new();

            let new_package = Package::new(
                filename,
                "0.1.0",
                "Description here"
            );
            let new_package_value = Value::try_from(new_package)?;
                tool.insert("py2deb".into(), new_package_value);
                table.insert(
                    "tool".into(),
                    Value::Table(tool),
            );

            fs::write(&config_path, toml::to_string_pretty(&table)?)?;
        },
        Some(Commands::Build {}) => {
            if !config_path.exists() {
                bail!("pyproject.toml not found!");
            }
            let config_toml = fs::read_to_string(&config_path).with_context(|| format!("Failed to read {}", config_path.display()))?;
            let value: toml::Value = toml::from_str(&config_toml)?;
            let section = value.get("tool").and_then(|t| t.get("py2deb")).with_context(|| format!("Cannot find [tool.py2deb] section. Init project first"))?;
            let package: Package = section.clone().try_into()?;
            println!("Building Debian package");
            println!("{}", package);
            let mut build = DebianBuild::new(package, current_path);
            build.build()?;
        },
        None => {}
    }

    Ok(())
}
