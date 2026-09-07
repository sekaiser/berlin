use std::path::Path;
use std::path::PathBuf;

use anyhow::Error;

pub struct Project {
    root: PathBuf,
}

impl Project {
    pub fn load() -> Result<Self, Error> {
        let root = root_from_environment()?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

pub(crate) fn root_from_environment() -> Result<PathBuf, Error> {
    let current_dir = std::env::current_dir()?;
    let root = std::env::var_os("BERLIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| current_dir.clone());
    Ok(if root.is_absolute() {
        root
    } else {
        current_dir.join(root)
    })
}
