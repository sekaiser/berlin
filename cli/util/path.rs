use berlin_core::anyhow::Error;
use berlin_core::error::uri_error;
use berlin_core::ModuleSpecifier;
use std::path::PathBuf;

// Attempts to convert a specifier to a file path. By default, uses the Url
/// crate's `to_file_path()` method, but falls back to try and resolve unix-style
/// paths on Windows.
pub fn specifier_to_file_path(specifier: &ModuleSpecifier) -> Result<PathBuf, Error> {
    let result = if cfg!(windows) {
        match specifier.to_file_path() {
            Ok(path) => Ok(path),
            Err(()) => {
                // This might be a unix-style path which is used in the tests even on Windows.
                // Attempt to see if we can convert it to a `PathBuf`. This code should be removed
                // once/if https://github.com/servo/rust-url/issues/730 is implemented.
                if specifier.scheme() == "file"
                    && specifier.host().is_none()
                    && specifier.port().is_none()
                    && specifier.path_segments().is_some()
                {
                    let path_str = specifier.path();
                    match String::from_utf8(
                        percent_encoding::percent_decode(path_str.as_bytes()).collect(),
                    ) {
                        Ok(path_str) => Ok(PathBuf::from(path_str)),
                        Err(_) => Err(()),
                    }
                } else {
                    Err(())
                }
            }
        }
    } else {
        specifier.to_file_path()
    };
    match result {
        Ok(path) => Ok(path),
        Err(()) => Err(uri_error(format!(
            "Invalid file path.\n  Specifier: {}",
            specifier
        ))),
    }
}

/// Gets the parent of this module specifier.
pub fn specifier_parent(specifier: &ModuleSpecifier) -> ModuleSpecifier {
    let mut specifier = specifier.clone();
    // don't use specifier.segments() because it will strip the leading slash
    let mut segments = specifier.path().split('/').collect::<Vec<_>>();
    if segments.iter().all(|s| s.is_empty()) {
        return specifier;
    }
    if let Some(last) = segments.last() {
        if last.is_empty() {
            segments.pop();
        }
        segments.pop();
        let new_path = format!("{}/", segments.join("/"));
        specifier.set_path(&new_path);
    }
    specifier
}
