mod config_file;
mod flags;

use std::env;
use std::path::PathBuf;

pub use config_file::ConfigFile;
pub use flags::*;

use berlin_core::anyhow::Error;
use berlin_core::ModuleSpecifier;

use crate::cache::BerlinDir;

/// Holds the resolved options of many sources used by sub commands
/// and provides some helper function for creating common objects.
pub struct CliOptions {
    // the source of the options is a detail the rest of the
    // application need not concern itself with, so keep these private
    flags: Flags,
    maybe_config_file: Option<ConfigFile>,
}

impl CliOptions {
    pub fn new(flags: Flags, maybe_config_file: Option<ConfigFile>) -> Self {
        Self {
            maybe_config_file,
            flags,
        }
    }

    pub fn from_flags(flags: Flags) -> Result<Self, Error> {
        let maybe_config_file = ConfigFile::discover(&flags)?;
        Ok(Self::new(flags, maybe_config_file))
    }

    pub fn maybe_config_file_specifier(&self) -> Option<ModuleSpecifier> {
        self.maybe_config_file.as_ref().map(|f| f.specifier.clone())
    }

    pub fn resolve_berlin_dir(&self) -> Result<BerlinDir, Error> {
        Ok(BerlinDir::new(self.maybe_custom_root())?)
    }

    pub fn maybe_custom_root(&self) -> Option<PathBuf> {
        self.flags
            .cache_path
            .clone()
            .or_else(|| env::var("BERLIN_DIR").map(String::into).ok())
    }

    pub fn watch_paths(&self) -> &Option<Vec<PathBuf>> {
        &self.flags.watch
    }
}
