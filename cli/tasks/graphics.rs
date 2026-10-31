//! Trusted LaTeX artwork rendered by the actual compiler, independently of the
//! paper-to-semantic-document profile. Callers own authorization and publication.
use super::cache::{Artifact, ArtifactCache, CacheKey, hash};
use anyhow::{Context, Error, bail};
use serde::Serialize;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) use berlin_core::TexEngine as Engine;

#[derive(Debug, Serialize)]
pub(crate) struct Input {
    pub relative: PathBuf,
    pub bytes: Vec<u8>,
}

pub(super) fn output_names(sources: &[super::SourceFile]) -> Result<Vec<String>, Error> {
    if sources.is_empty() {
        bail!("diagram source pattern matched no files");
    }
    let mut seen = std::collections::HashSet::new();
    sources
        .iter()
        .map(|source| {
            let stem = source
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .context("diagram needs a UTF-8 filename")?;
            if stem.is_empty()
                || !stem
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            {
                bail!("diagram filenames use ASCII letters, digits, '_' and '-'");
            }
            let name = format!("{stem}.svg");
            if !seen.insert(name.to_lowercase()) {
                bail!("duplicate diagram destination '{name}'");
            }
            Ok(name)
        })
        .collect()
}

pub(crate) fn snapshot_files(root: &Path, files: &[PathBuf]) -> Result<Vec<Input>, Error> {
    let mut inputs = Vec::new();
    let mut total = 0;
    for file in files {
        let relative = file.strip_prefix(root)?.to_path_buf();
        if relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
            || !file.canonicalize()?.starts_with(root.canonicalize()?)
        {
            bail!("diagram input escapes its declared root");
        }
        if relative.components().next().is_some_and(|c| {
            c.as_os_str()
                .to_string_lossy()
                .starts_with("berlin-artwork")
        }) {
            bail!("berlin-artwork is reserved for compiler scratch files");
        }
        let bytes = read_bounded(file, 32 * 1024 * 1024)?;
        total += bytes.len();
        if total > 64 * 1024 * 1024 || inputs.len() >= 256 {
            bail!("diagram inputs exceed 64 MiB or 256 files");
        }
        inputs.push(Input { relative, bytes });
    }
    inputs.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(inputs)
}

/// A declared input directory is a conservative dependency closure, not a guess
/// at TeX's macro-expanded filenames. Changes to any file invalidate the result.
pub(crate) fn snapshot(root: Option<&Path>) -> Result<Vec<Input>, Error> {
    let mut inputs = Vec::new();
    if let Some(root) = root {
        collect(root, root, &mut inputs, &mut 0, 0)?;
    }
    inputs.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(inputs)
}
fn collect(
    root: &Path,
    directory: &Path,
    inputs: &mut Vec<Input>,
    total: &mut usize,
    depth: usize,
) -> Result<(), Error> {
    if depth > 32 {
        bail!("graphics input directory exceeds 32 levels");
    }
    if fs::symlink_metadata(directory)?.file_type().is_symlink() {
        bail!("graphics inputs cannot be symlinks");
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        // Build state and editor files are not inputs. No hidden authoring data
        // is copied implicitly into a compiler workspace.
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let metadata = entry.file_type()?;
        if metadata.is_symlink() {
            bail!("graphics input is a symlink: {}", entry.path().display());
        }
        if metadata.is_dir() {
            collect(root, &entry.path(), inputs, total, depth + 1)?;
        } else if metadata.is_file() {
            let bytes = read_bounded(&entry.path(), 32 * 1024 * 1024)?;
            *total += bytes.len();
            if *total > 64 * 1024 * 1024 || inputs.len() >= 256 {
                bail!("graphics inputs exceed 64 MiB or 256 files");
            }
            let relative = entry.path().strip_prefix(root)?.to_path_buf();
            if relative.components().next().is_some_and(|c| {
                c.as_os_str()
                    .to_string_lossy()
                    .starts_with("berlin-artwork")
            }) {
                bail!("berlin-artwork is reserved for compiler scratch files");
            }
            inputs.push(Input { relative, bytes });
        } else {
            bail!("graphics input is not a regular file");
        }
    }
    Ok(())
}

pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        bail!("graphics file exceeds {limit} bytes: {}", path.display());
    }
    Ok(bytes)
}

pub(crate) fn render(
    source: &str,
    inputs: &[Input],
    engine: Engine,
    cache_root: &Path,
    no_cache: bool,
) -> Result<Vec<u8>, Error> {
    if source.len() > 1024 * 1024 {
        bail!("graphics source exceeds 1 MiB");
    }
    let tools = if no_cache {
        None
    } else {
        match super::paper::toolchain::graphics_fingerprint(engine.program()) {
            Ok(value) => value,
            Err(error) if error.is::<crate::util::process::Interrupted>() => return Err(error),
            Err(error) => {
                log::debug!("Graphics cache unavailable: {error:#}");
                None
            }
        }
    };
    let input_hashes = inputs
        .iter()
        .map(|input| (&input.relative, hash(&input.bytes)))
        .collect::<Vec<_>>();
    let key = tools
        .map(|tools| {
            CacheKey::new(
                "latex-graphics/v1",
                &(hash(source.as_bytes()), input_hashes, engine, tools),
            )
        })
        .transpose()?;
    let cache = ArtifactCache::new(cache_root);
    if let Some(artifact) = key.as_ref().and_then(|key| cache.load::<()>(key))
        && valid_svg(&artifact.bytes)
    {
        log::info!("Reusing LaTeX graphic");
        return Ok(artifact.bytes);
    }
    log::info!("Compiling LaTeX graphic ({})", engine.program());
    let work = tempfile::tempdir()?;
    for input in inputs {
        let path = work.path().join(&input.relative);
        fs::create_dir_all(path.parent().context("input has no parent")?)?;
        fs::write(path, &input.bytes)?;
    }
    let svg = render_in(work.path(), "berlin-artwork", source, engine)?;
    if let Some(key) = key
        && super::paper::toolchain::immutable_inputs(work.path()).unwrap_or(false)
    {
        cache.store(
            &key,
            &Artifact {
                value: (),
                bytes: svg.clone(),
            },
        );
    }
    Ok(svg)
}

/// Caller-supplied workspaces and stems are internal, never chosen by TeX output.
pub(crate) fn render_in(
    work: &Path,
    stem: &str,
    source: &str,
    engine: Engine,
) -> Result<Vec<u8>, Error> {
    fs::write(work.join(format!("{stem}.tex")), source)?;
    run(
        work,
        engine.program(),
        &[
            "-recorder",
            "-no-shell-escape",
            "-halt-on-error",
            "-interaction=nonstopmode",
            &format!("{stem}.tex"),
        ],
    )?;
    let pdf = format!("{stem}.pdf");
    let info = run(work, "pdfinfo", &[&pdf])?;
    if !info
        .lines()
        .find_map(|line| line.strip_prefix("Pages:"))
        .is_some_and(|v| v.trim() == "1")
    {
        bail!("a LaTeX graphic must produce exactly one PDF page");
    }
    pdf_to_svg(work, &pdf, &format!("{stem}.svg"))
}

pub(crate) fn pdf_to_svg(work: &Path, input: &str, output: &str) -> Result<Vec<u8>, Error> {
    run(
        work,
        "dvisvgm",
        &[
            "--pdf",
            "--no-fonts",
            "--page=1",
            &format!("--output={output}"),
            input,
        ],
    )?;
    let svg = read_bounded(&work.join(output), 32 * 1024 * 1024)?;
    if !valid_svg(&svg) {
        bail!("converter did not produce SVG");
    }
    Ok(svg)
}
fn valid_svg(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok_and(|text| text.contains("<svg") && text.contains("</svg>"))
}

pub(crate) fn run(work: &Path, program: &str, args: &[&str]) -> Result<String, Error> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(work)
        // Lua's font loader uses io.open on absolute kpse-resolved installed
        // files. Restricted reads support that; this is never a sandbox.
        .env("openin_any", if program == "lualatex" { "r" } else { "p" })
        .env("openout_any", "p")
        .env("SOURCE_DATE_EPOCH", "315532800")
        .env("FORCE_SOURCE_DATE", "1");
    if program == "lualatex" {
        fs::create_dir_all(work.join("berlin-artwork-cache"))?;
        command.env("TEXMFCACHE", "berlin-artwork-cache");
    }
    crate::util::process::run(&mut command, crate::util::process::Limits::default())
        .with_context(|| format!("LaTeX tool {program}"))
}
