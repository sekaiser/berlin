use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Error;
use glob::glob;

pub fn load_files(cwd: &Path, pattern: &str) -> Result<Vec<PathBuf>, Error> {
    let pattern_path = cwd.join(pattern);
    let pattern_path_str = pattern_path
        .to_str()
        .context("Input pattern is not valid UTF-8")?;
    let mut files = glob(pattern_path_str)?.collect::<Result<Vec<_>, _>>()?;
    files.sort();

    Ok(files)
}
