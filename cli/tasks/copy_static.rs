use std::path::Path;
use std::path::PathBuf;

use anyhow::Error;

pub(super) fn copy(
    sources: &[PathBuf],
    source_root: &Path,
    output_root: &Path,
) -> Result<(), Error> {
    std::fs::create_dir_all(output_root)?;
    for source in sources {
        let relative_path = source.strip_prefix(source_root)?;
        let output = output_root.join(relative_path);
        std::fs::create_dir_all(output.parent().expect("asset output has no parent"))?;
        std::fs::copy(source, output)?;
    }
    Ok(())
}
