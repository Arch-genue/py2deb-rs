use std::path::{PathBuf};
use std::env;
use std::fs;

use clap::{Parser, Subcommand};
use anyhow::{Context, Result, bail};
use toml::{Table, Value};
use colored::Colorize;

use py2deb::package::Package;
use py2deb::deb::build::DebianBuild;
use py2deb::Verbosity;

#[derive(Parser)]
#[command(version, about)]
struct CliArgs {
    #[arg(long, value_name="PATH")]
    path: Option<String>,
    #[arg(short, long, global = true)]
    quiet: bool,
    #[arg(short, long, action = clap::ArgAction::Count, conflicts_with = "quiet")]
    verbose: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Init project in current directory
    Init,
    /// Build project in current directory
    Build,
    /// Show project info
    Show
}

fn main() -> Result<()> {
    let cli = CliArgs::parse();
    let verbosity = Verbosity::from_flags(cli.quiet, cli.verbose);

    let mut project_path = env::current_dir()?;
    if let Some(_proj_path) = cli.path.as_deref() {
        let p_path = shellexpand::tilde(_proj_path);
        project_path = PathBuf::from(p_path.as_ref());
    }
    let config_path = project_path.join("pyproject.toml");

    match cli.command {
        Some(Commands::Init) => {
            let filename = project_path.file_name().and_then(|f| f.to_str()).context("Parent path has no file name")?;
            let is_exists: bool = config_path.exists();
            if !is_exists {
                fs::File::create(&config_path)?;
            }

            let config_toml = fs::read_to_string(&config_path).with_context(|| format!("Failed to read {}", config_path.display()))?;
            let value: toml::Value = toml::from_str(&config_toml)?;
            if let Some(section) = value.get("tool").and_then(|t| t.get("py2deb")) {
                let package: Package = section.clone().try_into()?;
                eprintln!("Current project: \n");
                eprintln!("{}", package);
                bail!("Cannot init new project");
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
        Some(Commands::Build) => {
            if !config_path.exists() {
                bail!("pyproject.toml not found!");
            }
            let package = Package::from_config(&config_path)?;

            if verbosity.is_normal() {
                eprintln!("{:>12} {} {} ({})", "Packaging".green(), package.package, package.version, project_path.display());
            }
            let mut build = DebianBuild::new(package, project_path).with_verbosity(verbosity);
            let build_info = build.build()?;
            if verbosity.is_normal() {
                let file_size = fs::metadata(&build_info.deb_path)?.len();
                eprintln!("{:>12} target in {:.2?} ({} KiB)", "Finished".green(), build_info.time, file_size / 1024);
            }
            println!("{}", build_info.deb_path.display());
        },
        None | Some(Commands::Show) => {
            if !config_path.exists() {
                bail!("Cannot not find pyproject.toml in `{}`", project_path.display());
            }
            let package = Package::from_config(&config_path)?;
            println!("{}", package);
            eprintln!("Run `py2deb build` to build the .deb package.");
        }
    }

    Ok(())
}
