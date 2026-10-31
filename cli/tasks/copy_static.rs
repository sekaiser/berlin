use std::path::Path;

use anyhow::Error;

pub(super) fn copy(sources: &[super::graphics::Input], output_root: &Path) -> Result<(), Error> {
    std::fs::create_dir_all(output_root)?;
    for source in sources {
        let output = output_root.join(&source.relative);
        std::fs::create_dir_all(output.parent().expect("asset output has no parent"))?;
        std::fs::write(output, &source.bytes)?;
    }
    Ok(())
}
