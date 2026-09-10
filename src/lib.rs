//! The parts of `py2deb` that are worth testing on their own.
//!
//! The binary is a thin CLI over this library; everything it does — reading
//! the config, walking the source tree, rendering the changelog and control
//! files — lives here so an integration test can drive it the same way
//! `main` does.

pub mod architecture;
pub mod deb;
pub mod git;
pub mod include_entry;
pub mod info;
pub mod package;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Verbosity {
    /// Only the final result on stdout.
    Quiet,
    /// Progress lines, the default.
    #[default]
    Normal,
    /// Every archive entry as it is written.
    Verbose,
}

impl Verbosity {
    pub fn from_flags(quiet: bool, verbose: u8) -> Self {
        match (quiet, verbose) {
            (true, _) => Self::Quiet,
            (_, 0) => Self::Normal,
            _ => Self::Verbose,
        }
    }

    /// Whether progress should be printed at all.
    pub fn is_normal(self) -> bool { self >= Self::Normal }
    /// Whether per-entry detail should be printed.
    pub fn is_verbose(self) -> bool { self >= Self::Verbose }
}
