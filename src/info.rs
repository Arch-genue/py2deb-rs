use std::{path::PathBuf, time::Duration};

#[derive(Debug)]
pub struct BuildInfo {
    pub deb_path: PathBuf,
    pub time: Duration
}

impl BuildInfo {
    pub fn new(deb_path: PathBuf, time: Duration) -> Self {
        Self {
            deb_path,
            time
        }
    }
}