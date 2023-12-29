use libs::anyhow::Error;
use std::path::Path;
use std::path::PathBuf;

use libs::glob::glob;

use crate::ModuleSpecifier;

/// Similar to `std::fs::canonicalize()` but strips UNC prefixes on Windows.
pub fn canonicalize_path(path: &Path) -> Result<PathBuf, Error> {
    let path = path.canonicalize()?;
    #[cfg(windows)]
    return Ok(strip_unc_prefix(path));
    #[cfg(not(windows))]
    return Ok(path);
}

pub fn load_files(cwd: &PathBuf, pattern: &str) -> Vec<PathBuf> {
    let pattern_path = cwd.join(pattern);
    let pattern_path_str = pattern_path.to_str().unwrap();

    glob(pattern_path_str).unwrap().flatten().collect()
}

pub fn consume_files<F>(path: PathBuf, pattern: &str, consumer: F)
where
    F: FnOnce(Vec<ModuleSpecifier>),
{
    let mut specifiers = Vec::<ModuleSpecifier>::new();
    for f in load_files(&path, pattern) {
        let specifier = ModuleSpecifier::from_file_path(f).expect("Invalid path.");
        specifiers.push(specifier);
    }
    consumer(specifiers);
}
