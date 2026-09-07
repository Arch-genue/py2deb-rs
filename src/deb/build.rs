use crate::package::Package;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::{fs, io};
use std::io::{Read, Write};

use anyhow::{Result, Context, bail};
use ignore::{WalkBuilder, overrides::OverrideBuilder};

use flate2::write::GzEncoder;
use flate2::Compression;
use tar::{Header as TarHeader, Builder as TarBuilder};
use md5::{Md5, Digest};

use colored::Colorize;

pub struct DebianBuild {
    package: Package,
    current_path: PathBuf,
    source_path: PathBuf,

    tar_header: Option<TarHeader>,
    tar_root_path: PathBuf,

    /// MD5 digest of every regular file written to `data.tar.gz`, keyed by its
    /// archive path without the leading `./` — exactly the form `md5sums`
    /// needs in `control.tar.gz`. Filled in while the data archive is built,
    /// so the source tree is only walked once.
    md5sums: HashMap<String, String>,
}

impl DebianBuild {
    pub fn new(package: Package, current_path: PathBuf) -> Self {
        Self { 
            package, 
            current_path: current_path.clone(),
            source_path: current_path,
            tar_header: None,
            tar_root_path: PathBuf::from("usr/lib/python3/dist-packages"),
            md5sums: HashMap::new(),
        }
    }

    pub fn build(&mut self) -> Result<()> {
        let source_path = self.current_path.join(&self.package.src);
        println!("Source path: {}", source_path.to_string_lossy().blue());
        self.source_path = self.current_path.join(&self.package.src);
        if !source_path.exists() {
            bail!("Cannot find source path!");
        }        
        let filename = self.current_path.file_name().and_then(|f| f.to_str()).context("Parent path has no file name")?;
        let dest_name = self.package.dest.clone().unwrap_or(filename.to_string());
        
        if dest_name.is_empty() {
            bail!("Destination name is empty!");
        }

        let mut overrides = OverrideBuilder::new(&source_path);
        for exclude_rule in &self.package.exclude {
            let exclude_item = &("!".to_owned()+exclude_rule);
            overrides.add(exclude_item)?;
        }

        let walker = WalkBuilder::new(&source_path)
            .hidden(false)
            .git_ignore(true)
            .git_global(false)
            .git_exclude(false)
            .overrides(overrides.build()?)
            .build();

        self.tar_root_path = self.tar_root_path.join(&dest_name);

        println!("Create data.tar.gz archive in target/debian");
        let target_deb_path = self.current_path.join("target").join("debian");
        if !target_deb_path.exists() {
            fs::create_dir(&target_deb_path).context("Cannot create target/debian directory")?;
        }
        
        // Tar pack
        let tar_file = fs::File::create(target_deb_path.join("data.tar.gz")).context("Cannot create data.tar.gz file")?;
        let encoder = GzEncoder::new(tar_file, Compression::default());
        let mut data_archive = TarBuilder::new(encoder);

        let mut header = TarHeader::new_gnu();
        header.set_mode(0o755);
        header.set_uid(0);
        header.set_gid(0);
        header.set_username("root")?;
        header.set_groupname("root")?;

        self.tar_header = Some(header);

        for entry in walker {
            match entry {
                Ok(entry) => {
                    let path = entry.path();

                    let nd_path = path.strip_prefix(&self.source_path)?;
                    let relative_path = self.tar_root_path.join(nd_path);
                    if path.is_dir() {
                        self.write_dir_entry(&mut data_archive, format!("./{}/", relative_path.display()))?;
                    } else {
                        self.write_file_entry(&mut data_archive, path, format!("./{}", relative_path.display()), true)?;
                    }
                },
                Err(e) => eprintln!("Glob error: {e}"),
            }
        }
        let encoder = data_archive.into_inner()?;
        encoder.finish()?;
        //TODO data archive to fn
        self.create_control_archive().context("Unable to create control.tar.gz")?;
        self.create_ar_archive()?;

        Ok(())
    }

    fn create_control_archive(&mut self) -> Result<()> {
        let target_deb_path = self.current_path.join("target").join("debian");
        if !target_deb_path.exists() {
            fs::create_dir(&target_deb_path).context("Cannot create target/debian directory")?;
        }

        let tar_file = fs::File::create(target_deb_path.join("control.tar.gz")).context("Cannot create control.tar.gz file")?;
        let encoder = GzEncoder::new(tar_file, Compression::default());
        let mut data_archive = TarBuilder::new(encoder);

        let mut header = TarHeader::new_gnu();
        header.set_mode(0o755);
        header.set_uid(0);
        header.set_gid(0);
        header.set_username("root")?;
        header.set_groupname("root")?;
        self.tar_header = Some(header);

        let control_path = target_deb_path.join("control");
        fs::write(&control_path, self.package.save_string()).context("Cannot create control file")?;

        let md5sums_path = target_deb_path.join("md5sums");
        let mut md5_strings = String::new();
        for (path, hash) in &self.md5sums {
            let stripped = path.strip_prefix("./").unwrap_or(path);
            md5_strings.push_str(format!("{}  {}\n", hash, stripped).as_str());
        }
        fs::write(&md5sums_path, md5_strings).context("Cannot create md5sums file")?;

        self.write_file_entry(&mut data_archive, control_path.as_path(), "./control".to_string(), false)?;
        self.write_file_entry(&mut data_archive, md5sums_path.as_path(), "./md5sums".to_string(), false)?;
        
        let encoder = data_archive.into_inner()?;
        encoder.finish()?;

        Ok(())
    }

    fn create_ar_archive(&mut self) -> Result<()> {
        let target_deb_path = self.current_path.join("target").join("debian");
        let debian_package_path = target_deb_path.join(self.package.get_package_file_name());

        let mut builder = ar::Builder::new(fs::File::create(debian_package_path)?);

        let header = ar::Header::new(b"debian-binary".to_vec(), 4);
        builder.append(&header, &b"2.0\n"[..])?;

        builder.append_path(target_deb_path.join("control.tar.gz"))?;
        builder.append_path(target_deb_path.join("data.tar.gz"))?;
        // control.tar.gz, затем data.tar.gz

        Ok(())
    }

    fn write_dir_entry<W: Write>(&mut self, archive: &mut TarBuilder<W>, rel_str: String) -> Result<()> {
        let header = self.tar_header.as_mut().expect("TarHeader doesnt exists");

        println!("Create folder {}", rel_str);
        header.set_path(rel_str)?;
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();

        archive.append(&header, io::empty())?;
        Ok(())
    }
    fn write_file_entry<W: Write>(&mut self, archive: &mut TarBuilder<W>, path: &Path, rel_str: String, data_tar: bool) -> Result<()> {
        println!("Create file {}", rel_str);

        let contents = fs::read(path)?;

        // md5sums and Installed-Size describe the payload only, so they are
        // collected for data.tar alone. Both touch `self`, so they run before
        // `tar_header` is borrowed — that borrow covers all of `self`.
        if data_tar {
            let hex: String = Md5::digest(&contents)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            self.md5sums.insert(rel_str.clone(), hex);
            self.package.installed_size += contents.len().div_ceil(1024) as u64;
        }

        let header = self.tar_header.as_mut().expect("TarHeader doesnt exists");
        header.set_path(rel_str)?;
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();

        // `&[u8]` is a `Read`, so the file is not opened a second time.
        archive.append(&header, contents.as_slice())?;
        Ok(())
    }
}