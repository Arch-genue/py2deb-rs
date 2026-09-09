use crate::Verbosity;
use crate::include_entry::IncludeEntry;
use crate::info::BuildInfo;
use crate::package::Package;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::{Write, empty};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, Context, bail};
use ignore::{WalkBuilder, overrides::OverrideBuilder};

use flate2::write::GzEncoder;
use flate2::Compression;
use tar::{Header as TarHeader, Builder as TarBuilder};
use md5::{Md5, Digest};

use colored::Colorize;

struct EntryOption {
    path: PathBuf,
    rel_str: String,
    chmod: u32
}
impl Default for EntryOption {
    fn default() -> Self {
        Self {
            path: PathBuf::from("/"),
            rel_str: "".to_string(),
            chmod: 0o644
        }
    }
}

pub struct DebianBuild {
    /// Package struct
    package: Package,
    /// Project path
    current_path: PathBuf,
    /// Project source path
    source_path: PathBuf,
    /// Verbosity enum
    verbosity: Verbosity,

    tar_header: Option<TarHeader>,
    tar_root_path: PathBuf,

    /// MD5 digest of every regular file written to `data.tar.gz`, keyed by its
    /// archive path without the leading `./` — exactly the form `md5sums`
    /// needs in `control.tar.gz`. Filled in while the data archive is built,
    /// so the source tree is only walked once.
    md5sums: HashMap<String, String>,
    /// Build time for all files
    mtime: u64,
    /// HashSet for created dirs
    created_dirs: HashSet<String>,
    /// Summary files size in data.tar.gz
    data_bytes: u64
}

impl DebianBuild {
    pub fn new(package: Package, current_path: PathBuf) -> Self {
        Self { 
            package, 
            current_path: current_path.clone(),
            source_path: current_path,
            verbosity: Verbosity::Normal,
            tar_header: None,
            tar_root_path: PathBuf::from("usr/lib/python3/dist-packages"),
            md5sums: HashMap::new(),
            mtime: 0,
            created_dirs: HashSet::new(),
            data_bytes: 0
        }
    }
    pub fn with_verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = verbosity;
        self
    }

    pub fn check(&mut self) -> Result<()> {

        Ok(())
    }

    pub fn build(&mut self) -> Result<BuildInfo> {
        let source_path = self.current_path.join(&self.package.src);
        self.source_path = self.current_path.join(&self.package.src);
        if !source_path.exists() {
            bail!("Cannot find source path");
        }
        
        self.mtime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let started = Instant::now();
        self.build_data_archive().context("Unable to create data.tar.gz")?;
        self.build_control_archive().context("Unable to create control.tar.gz")?;
        let deb_path = self.build_ar_archive().context("Unable to create ar deb")?;
        
        let build_info = BuildInfo::new(
            deb_path,
            started.elapsed()
        );

        Ok(build_info)
    }

    fn build_data_archive(&mut self) -> Result<()> {
        if self.verbosity.is_normal() {
            eprintln!("{:>13} data.tar.gz archive", "Building".blue());
        }   

        let filename = self.current_path.file_name().and_then(|f| f.to_str()).context("Parent path has no file name")?;
        let dest_name = self.package.dest.clone().unwrap_or(filename.to_string());
        
        if dest_name.is_empty() {
            bail!("Destination name is empty!");
        }
        self.tar_root_path = self.tar_root_path.join(&dest_name);

        let mut overrides = OverrideBuilder::new(&self.source_path);
        for exclude_rule in &self.package.exclude {
            let exclude_item = &("!".to_owned()+exclude_rule);
            overrides.add(exclude_item)?;
        }

        let walker = WalkBuilder::new(&self.source_path)
            .hidden(false)
            .git_ignore(true)
            .git_global(false)
            .git_exclude(false)
            .overrides(overrides.build()?)
            .build();

        let target_deb_path = self.current_path.join("target").join("debian");
        if !target_deb_path.exists() {
            fs::create_dir_all(&target_deb_path).context("Cannot create target/debian directory")?;
        }
        
        // Tar pack
        let tar_file = fs::File::create(target_deb_path.join("data.tar.gz")).context("Cannot create data.tar.gz file")?;
        let encoder = GzEncoder::new(tar_file, Compression::default());
        let mut data_archive = TarBuilder::new(encoder);

        let mut header = TarHeader::new_gnu();
        header.set_mode(0o755);
        header.set_uid(0);
        header.set_gid(0);
        header.set_username("0")?;
        header.set_groupname("0")?;
        header.set_mtime(self.mtime);

        self.tar_header = Some(header);

        for entry in walker {
            let entry = entry.context("Failed to walk the source tree")?;
            let path = entry.path();

            let nd_path = path.strip_prefix(&self.source_path).with_context(|| format!("{} is not inside {}", path.display(), self.source_path.display()))?;
            
            let relative_path: PathBuf = self.tar_root_path.join(nd_path);
            if path.is_dir() {
                // The path may already end in a slash; tar wants exactly one.
                let dir = relative_path.display().to_string();
                self.write_dir_entry(&mut data_archive, format!("./{}/", dir.trim_end_matches('/')))?;
            } else {
                self.write_file_entry(&mut data_archive, EntryOption{path: path.to_path_buf(), rel_str: format!("./{}", relative_path.display()), ..Default::default()}, true)?;
            }
        }

        // Copyright
        let copyright_path = target_deb_path.join("copyright");
        let copyright_contents = self.generate_copyright()?;

        fs::write(&copyright_path, copyright_contents).context("Cannot create copyright file")?;
        let copyright_entry = IncludeEntry{
            src: "target/debian/copyright".into(),
            dest: "usr/share/doc/$package/copyright".into(),
            mode: Some(0o644)
        };
        self.package.include.push(copyright_entry);

        // Includes
        let includes = &self.package.include.clone();
        for entry in includes {
            let src_path = self.current_path.join(&entry.src);
            if !src_path.exists() {
                bail!("Include entry not exists {}", src_path.display());
            }
            let dest_path = entry.resolve_dest(&src_path, &self.package).context("Cannot resolve dest in include field")?;

            let chmode = entry.resolve_mode(&src_path).unwrap_or(0o644);
            self.warn_unexpected_mode(&dest_path, chmode);

            // An include may land anywhere, so its parents are not covered by
            // the walk above; dpkg needs every one of them present.
            if let Some(parent) = dest_path.parent() {
                if !parent.as_os_str().is_empty() {
                    self.write_dir_entry(&mut data_archive, format!("./{}/", parent.display()))?;
                }
            }

            self.write_file_entry(&mut data_archive, EntryOption{path: src_path, rel_str: format!("./{}", dest_path.display()), chmod: chmode}, true)?;
        }

        let count  = self.md5sums.len();
        if self.verbosity.is_normal() {
            eprintln!("{:>15} {} files, {} KiB", "Collected".blue(), count, self.data_bytes / 1024);
        }

        data_archive.finish()?;

        Ok(())
    }

    /// Builds the machine-readable copyright file.
    ///
    /// Follows what `cargo-deb` does: the ownership line falls back through
    /// the config's `copyright`, then the maintainer as a `Comment` — never
    /// as a `Copyright`, which would claim the packager owns the work — and
    /// is left out entirely for licences that grant rights without naming an
    /// owner. Anything else warns, because Debian requires the information.
    fn generate_copyright(&self) -> Result<String> {
        let package = &self.package;
        let license = self.resolve_license()?;

        let mut out = String::from(
            "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n",
        );
        out.push_str(&format!("Upstream-Name: {}\n", package.package));

        let homepage = package.expand_dollar_properties(&package.homepage);
        if !homepage.is_empty() {
            out.push_str(&format!("Source: {homepage}\n"));
        }

        out.push_str("\nFiles: *\n");

        let copyright = package.expand_dollar_properties(&package.copyright);
        if !copyright.is_empty() {
            out.push_str(&format!("Copyright: {copyright}\n"));
        } else if license_needs_no_author(&license.name) {
            // A licence like CC0 grants rights without naming an owner, so
            // there is nothing missing to report.
        } else if !package.maintainer.is_empty() {
            out.push_str(&format!(
                "Comment: Copyright information missing (maintainer: {})\n",
                package.maintainer
            ));
            eprintln!(
                "{:>15} no copyright set; add `copyright = \"2026 Your Name\"` to [tool.py2deb]",
                "Warning".yellow().bold()
            );
        } else {
            eprintln!(
                "{:>15} Debian requires copyright information, but none could be determined",
                "Warning".yellow().bold()
            );
        }

        out.push_str(&format!("License: {}\n", license.name));
        out.push_str(&license.body);

        Ok(out)
    }

    /// Resolves what goes into the copyright file's `License:` field.
    ///
    /// A `license-file` is read verbatim and indented into the field, which is
    /// what proprietary or uncommon licenses need. Otherwise the short name is
    /// used on its own, and for licenses Debian ships in
    /// `/usr/share/common-licenses` a pointer there replaces the full text —
    /// Policy asks packages not to duplicate those.
    fn resolve_license(&self) -> Result<ResolvedLicense> {
        let name = self.package.expand_dollar_properties(&self.package.license);
        let name = if name.is_empty() {
            if self.package.license_file.is_empty() {
                eprintln!(
                    "{:>15} no license set, assuming GPL-3",
                    "Warning".yellow().bold()
                );
            }
            "GPL-3".to_string()
        } else {
            name
        };

        if !self.package.license_file.is_empty() {
            let path = self.current_path.join(&self.package.license_file);
            let text = fs::read_to_string(&path)
                .with_context(|| format!("Cannot read license file {}", path.display()))?;

            return Ok(ResolvedLicense {
                body: indent_license_text(&text, self.package.license_file_skip_lines as usize),
                name,
            });
        }

        // Debian ships these, so the file only has to point at them.
        let common = Path::new("/usr/share/common-licenses").join(&name);
        let body = if common.exists() {
            format!(
                " On Debian systems, the complete text of the {} license can be\n found in `{}'.\n",
                name,
                common.display()
            )
        } else {
            eprintln!(
                "{:>15} {} is not in /usr/share/common-licenses; set `license-file` to ship its text",
                "Warning".yellow().bold(),
                name
            );
            String::new()
        };

        Ok(ResolvedLicense { name, body })
    }

    /// Warns when a file's mode contradicts where it is being installed.
    ///
    /// Neither case stops the build — the mode may well be deliberate — but
    /// both are almost always a mistake worth seeing: a program under `bin`
    /// that nobody can run, or a plain data file marked executable, which
    /// lintian reports too.
    fn warn_unexpected_mode(&self, dest: &Path, mode: u32) {
        let dest_str = dest.to_string_lossy();
        let in_bin_dir = ["bin/", "sbin/"]
            .iter()
            .any(|dir| dest_str.contains(dir));
        let executable = mode & 0o111 != 0;

        if in_bin_dir && !executable {
            eprintln!(
                "{:>15} {} is not executable ({:04o}) — nothing under bin/ can run it",
                "Warning".yellow().bold(),
                dest_str,
                mode
            );
        } else if !in_bin_dir && executable {
            eprintln!(
                "{:>15} {} is executable ({:04o}) but installs outside bin/",
                "Warning".yellow().bold(),
                dest_str,
                mode
            );
        }

        // Debian Policy asks for 0755/0644; anything group- or world-writable
        // is a packaging bug even when the source file has it.
        if mode & 0o022 != 0 {
            eprintln!(
                "{:>15} {} is writable by group or others ({:04o})",
                "Warning".yellow().bold(),
                dest_str,
                mode
            );
        }
    }

    fn build_control_archive(&mut self) -> Result<()> {
        if self.verbosity.is_normal() {
            eprintln!("{:>13} control.tar.gz archive", "Building".blue());
        }
        let target_deb_path = self.current_path.join("target").join("debian");
        if !target_deb_path.exists() {
            fs::create_dir(&target_deb_path).context("Cannot create target/debian directory")?;
        }

        let tar_file = fs::File::create(target_deb_path.join("control.tar.gz")).context("Cannot create control.tar.gz file")?;
        let encoder = GzEncoder::new(tar_file, Compression::default());
        let mut control_archive = TarBuilder::new(encoder);

        let mut header = TarHeader::new_gnu();
        header.set_mode(0o755);
        header.set_uid(0);
        header.set_gid(0);
        header.set_username("0")?;
        header.set_groupname("0")?;
        header.set_mtime(self.mtime);
        self.tar_header = Some(header);

        let control_path = target_deb_path.join("control");
        fs::write(&control_path, self.package.generate_control()).context("Cannot create control file")?;

        // Collect md5 sums
        let md5sums_path = target_deb_path.join("md5sums");
        let mut md5_strings = String::new();
        for (path, hash) in &self.md5sums {
            let stripped = path.strip_prefix("./").unwrap_or(path);
            md5_strings.push_str(format!("{}  {}\n", hash, stripped).as_str());
        }
        fs::write(&md5sums_path, md5_strings).context("Cannot create md5sums file")?;

        self.write_file_entry(&mut control_archive, EntryOption{path: control_path, rel_str: "./control".to_string(), ..EntryOption::default()}, false)?;
        self.write_file_entry(&mut control_archive, EntryOption{path: md5sums_path, rel_str: "./md5sums".to_string(), ..EntryOption::default()}, false)?;

        let scripts = self.create_control_scripts().context("Cannot create control scripts")?;
        for script_path in scripts {
            let filename = script_path.file_name().and_then(|f| f.to_str()).context("Cannot get script filename")?;
            self.write_file_entry(&mut control_archive, EntryOption{path: script_path.clone(), rel_str: format!("./{}", filename), chmod: 0o755}, false)?;
        }

        // Changelog
        let changelog = &self.package.changelog;
        if changelog.is_empty() {
            eprintln!(
                "{:>15} Changelog file not specified", "Warning".yellow().bold()
            );
        } else {
            let changelog_path = self.current_path.join(changelog);
            if changelog_path.exists() {
                //TODO!!! CHANGELOG BUILD
                // self.write_file_entry(&mut data_archive, EntryOption{path: control_path, rel_str: "./control".to_string(), ..EntryOption::default()}, false)?;
            } else {
                eprintln!("{:>15} changelog file not found {}", "Warning".yellow().bold(), changelog_path.display());
            }
        }

        control_archive.finish()?;

        Ok(())
    }

    fn create_control_scripts(&mut self) -> Result<Vec<PathBuf>> {
        let target_deb_path = self.current_path.join("target").join("debian");
        let mut scripts: Vec<PathBuf> = Vec::new();
        let postinst = format!("#!/bin/sh
set -e
case \"$1\" in
    configure)
        py3compile -p {}
    ;;
esac
", self.package.package);
        let prerm = format!("#!/bin/sh
set -e
case \"$1\" in
    remove|upgrade|deconfigure)
        py3clean -p {}
    ;;
esac
", self.package.package);
        let postinst_path = target_deb_path.join("postinst");
        let prerm_path = target_deb_path.join("prerm");
        
        fs::write(&postinst_path, postinst).context("Unable to create postinst script")?;
        fs::write(&prerm_path, prerm).context("Unable to create prerm script")?;
        scripts.push(postinst_path);
        scripts.push(prerm_path);

        Ok(scripts)
    }

    fn build_ar_archive(&mut self) -> Result<PathBuf> {
        let target_deb_path = self.current_path.join("target").join("debian");
        let package_name = self.package.get_package_file_name();
        let debian_package_path = target_deb_path.join(&package_name);
        if self.verbosity.is_normal() {
            eprintln!("{:>13} package {}", "Building".blue(), package_name);
        }

        let mut builder = ar::Builder::new(fs::File::create(&debian_package_path)?);

        // Members must come in this order: dpkg reads the archive as a stream
        // and wants the format version first, then the metadata it decides on.
        self.append_ar_member(&mut builder, "debian-binary", b"2.0\n")?;
        for member in ["control.tar.gz", "data.tar.gz"] {
            let bytes = fs::read(target_deb_path.join(member))
                .with_context(|| format!("Cannot read {member}"))?;
            self.append_ar_member(&mut builder, member, &bytes)?;
        }

        Ok(debian_package_path)
    }

    /// Appends one `ar` member with the ownership dpkg itself writes.
    ///
    /// `ar::Builder::append_path` would copy uid, gid and mtime from the build
    /// machine, which leaves the packager's own account stamped on the archive
    /// and makes builds unreproducible; every field is set explicitly instead.
    fn append_ar_member<W: Write>(&self, builder: &mut ar::Builder<W>, name: &str, bytes: &[u8]) -> Result<()> {
        let mut header = ar::Header::new(name.as_bytes().to_vec(), bytes.len() as u64);
        header.set_mode(0o100644); // dpkg writes the file-type bits too
        header.set_mtime(self.mtime);
        header.set_uid(0);
        header.set_gid(0);

        builder
            .append(&header, bytes)
            .with_context(|| format!("Cannot add {name} to the deb archive"))
    }

    fn write_dir_entry<W: Write>(&mut self, archive: &mut TarBuilder<W>, rel_str: String) -> Result<()> {
        if self.verbosity.is_verbose() {
            eprintln!("{:>15} {}", "Adding".blue(), rel_str.green());
        }
        let mut acc = String::from(".");
        for segment in rel_str.trim_start_matches("./").trim_end_matches("/").split("/") {
            if segment.is_empty() {
                continue;
            }
            acc.push('/');
            acc.push_str(segment);

            if !self.created_dirs.insert(acc.clone()) {
                continue;
            }

            self.append_dir(archive, format!("{acc}/"))?;
        }

        Ok(())
    }

    fn append_dir<W: Write>(&mut self, archive: &mut TarBuilder<W>, dir: String) -> Result<()> {
        let header = self.tar_header.as_mut().expect("TarHeader doesnt exists");

        let path_bytes = dir.as_bytes();
        let bytes_to_copy = &path_bytes[..std::cmp::min(path_bytes.len(), 100)];
        header.as_mut_bytes()[0..100].fill(0);
        header.as_mut_bytes()[0..bytes_to_copy.len()].copy_from_slice(bytes_to_copy);

        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();

        archive.append(header, empty())?;
        Ok(())
    }

    fn write_file_entry<W: Write>(&mut self, archive: &mut TarBuilder<W>, entry: EntryOption, data_tar: bool) -> Result<()> {
        if self.verbosity.is_verbose() {
            eprintln!("{:>15} {}", "Adding".blue(), entry.rel_str);
        }
        let path = &entry.path;
        let contents = fs::read(path)?;

        // md5sums and Installed-Size describe the payload only, so they are
        // collected for data.tar alone. Both touch `self`, so they run before
        // `tar_header` is borrowed — that borrow covers all of `self`.
        if data_tar {
            let hex: String = Md5::digest(&contents)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            self.md5sums.insert(entry.rel_str.clone(), hex);
            let size = contents.len();
            self.package.installed_size += size.div_ceil(1024) as u64;
            self.data_bytes += size as u64;
        }

        let header = self.tar_header.as_mut().expect("TarHeader doesnt exists");
        // header.set_path(entry.rel_str)?; // SHIT SHIT SHIT!!!!!!!!!!!

        let path_bytes = entry.rel_str.as_bytes();
        let bytes_to_copy = &path_bytes[..std::cmp::min(path_bytes.len(), 100)];
        header.as_mut_bytes()[0..100].fill(0);
        header.as_mut_bytes()[0..bytes_to_copy.len()].copy_from_slice(bytes_to_copy);

        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(contents.len() as u64);
        header.set_mode(entry.chmod);
        header.set_cksum();

        // `&[u8]` is a `Read`, so the file is not opened a second time.
        archive.append(header, contents.as_slice())?;
        Ok(())
    }
}

/// The `License:` field of a copyright file: its short name, plus the
/// indented block that follows it (empty when there is nothing to say).
struct ResolvedLicense {
    name: String,
    body: String,
}

/// Folds licence text into an RFC822 continuation block.
///
/// Every line gains a leading space, and blank lines become ` .` — a truly
/// empty line would end the paragraph and cut the field short.
fn indent_license_text(text: &str, skip_lines: usize) -> String {
    text.lines()
        .skip(skip_lines)
        .map(|line| {
            if line.trim().is_empty() {
                " .\n".to_string()
            } else {
                format!(" {line}\n")
            }
        })
        .collect()
}

/// Licences that grant their rights without naming a copyright owner, so a
/// missing `Copyright:` is not a packaging mistake. Mirrors `cargo-deb`.
fn license_needs_no_author(name: &str) -> bool {
    ["UNLICENSED", "PROPRIETARY", "CC-PDDC", "CC0-1.0"]
        .iter()
        .any(|l| l.eq_ignore_ascii_case(name))
}
