use crate::{args::diagram::Options, tasks::graphics};
use anyhow::{Context, Error, bail};
use std::{fs, io::Write, path::Path};

pub fn run(options: Options) -> Result<(), Error> {
    if !options.allow_tex {
        bail!(
            "render-diagram requires --allow-tex for trusted source and toolchains; not a sandbox"
        );
    }
    if options.output.extension().is_none_or(|ext| ext != "svg") {
        bail!("diagram output must be an SVG file");
    }
    let output = std::path::absolute(&options.output)?;
    let parent = output.parent().context("output has no parent")?;
    if !parent.is_dir() {
        bail!("create the output directory before rendering");
    }
    if output == std::path::absolute(&options.source)?
        || fs::symlink_metadata(&output).is_ok_and(|m| m.file_type().is_symlink())
    {
        bail!("output cannot replace source or a symlink");
    }
    if let Some(root) = &options.input_root {
        let root = root.canonicalize()?;
        if parent.canonicalize()?.starts_with(&root) {
            bail!(
                "output must be outside the declared input directory to avoid a cache feedback loop"
            );
        }
    }
    let source = String::from_utf8(graphics::read_bounded(&options.source, 1024 * 1024)?)?;
    let inputs = graphics::snapshot(options.input_root.as_deref())?;
    let cache_root: &Path = options.cache_root.as_deref().unwrap_or(parent);
    let svg = graphics::render(
        &source,
        &inputs,
        options.engine.parse().map_err(Error::msg)?,
        cache_root,
        options.no_cache,
    )?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(&svg)?;
    staged.persist(&output).map_err(|e| e.error)?;
    println!("{}", output.display());
    Ok(())
}
