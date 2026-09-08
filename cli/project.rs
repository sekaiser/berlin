use std::path::Path;
use std::path::PathBuf;

use anyhow::Error;

#[derive(Clone)]
pub struct Project {
    root: PathBuf,
    pipeline_files: Vec<PathBuf>,
}

impl Project {
    pub fn load(pipeline_files: Vec<PathBuf>) -> Result<Self, Error> {
        let root = root_from_environment()?;
        Ok(Self::new(root, pipeline_files))
    }

    pub fn new(root: PathBuf, pipeline_files: Vec<PathBuf>) -> Self {
        let pipeline_files = if pipeline_files.is_empty() {
            vec![root.join("berlin.pipeline.rhai")]
        } else {
            pipeline_files
                .into_iter()
                .map(|path| root.join(path))
                .collect()
        };
        Self {
            root,
            pipeline_files,
        }
    }

    pub fn pipeline_files(&self) -> &[PathBuf] {
        &self.pipeline_files
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
