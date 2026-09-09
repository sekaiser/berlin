//! Pipeline-owned paper preparation. Sources are snapshots; PDFs stay in the
//! output transaction and compiler scratch files are cleaned automatically.
use super::{
    SourceFile,
    cache::{Artifact, ArtifactCache, CacheKey},
};
use crate::project::Project;
use anyhow::{Context, Error, bail};
use berlin_content::DocumentCollection;
use berlin_document::{ContentId, Document, Metadata, Provenance, PublicationDate, SourceFormat};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

mod figures;
mod sources;
pub(super) mod toolchain;

/// Watch dependency parents so atomic saves and includes outside the main file's
/// directory are observed. Invalid drafts retain the base watcher until fixed.
pub(crate) fn dependency_directories(project: &Project, pattern: &str) -> Vec<std::path::PathBuf> {
    load(project, pattern)
        .unwrap_or_default()
        .into_iter()
        .flat_map(|source| {
            source
                .snapshot
                .dependencies
                .into_iter()
                .map(|file| file.path)
                .chain(source.snapshot.resources.into_iter().map(|file| file.path))
                .filter_map(|path| path.parent().map(Path::to_path_buf))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[derive(Clone, Debug)]
pub(super) struct Source {
    pub text: SourceFile,
    pub snapshot: sources::Snapshot,
    id: ContentId,
    published: PublicationDate,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicationMetadata {
    id: Option<String>,
    published: Option<PublicationDate>,
}

pub(super) fn load(project: &Project, pattern: &str) -> Result<Vec<Source>, Error> {
    let (root, _) = crate::util::fs::source_location(project, pattern)?;
    let root = root.canonicalize()?;
    let mut sources = Vec::new();
    let mut ids = std::collections::HashSet::new();
    for path in crate::util::fs::load_files(project, pattern)? {
        let path = path.canonicalize()?;
        if !path.starts_with(&root) {
            bail!(
                "LaTeX source escapes its configured root: {}",
                path.display()
            );
        }
        let text = read_source(path.clone())?;
        let snapshot = sources::Snapshot::load(&text, &root)
            .with_context(|| format!("invalid LaTeX source {}", path.display()))?;
        let metadata = metadata(&text.text)
            .with_context(|| format!("invalid publication metadata in {}", path.display()))?;
        let published = metadata
            .published
            .or_else(|| {
                snapshot
                    .details
                    .date
                    .as_deref()
                    .and_then(|date| date.parse().ok())
            })
            .with_context(|| {
                format!(
                    "{} needs an ISO date in \\date{{YYYY-MM-DD}} or a % berlin: published field",
                    path.display()
                )
            })?;
        let relative = path
            .strip_prefix(&root)?
            .to_str()
            .context("LaTeX path is not UTF-8")?;
        let namespace = if pattern.starts_with('@') {
            pattern.split('/').next().unwrap_or_default()
        } else {
            ""
        };
        let id = ContentId(
            metadata
                .id
                .unwrap_or_else(|| format!("latex:{namespace}/{relative}")),
        );
        if id.0.trim().is_empty() || id.0.chars().any(char::is_control) || !ids.insert(id.clone()) {
            bail!(
                "invalid or duplicate paper ID '{}' in {}",
                id.0,
                path.display()
            );
        }
        sources.push(Source {
            text,
            snapshot,
            id,
            published,
        });
    }
    if sources.is_empty() {
        bail!("LaTeX pattern '{pattern}' matched no source files");
    }
    Ok(sources)
}

fn metadata(source: &str) -> Result<PublicationMetadata, Error> {
    let mut found = None;
    for line in source.lines() {
        if let Some(json) = line.trim_start().strip_prefix("% berlin:") {
            if found.is_some() {
                bail!("only one % berlin: metadata line is allowed");
            }
            found = Some(serde_json::from_str(json)?);
        }
    }
    Ok(found.unwrap_or_default())
}

fn read_source(path: std::path::PathBuf) -> Result<SourceFile, Error> {
    if fs::metadata(&path)?.len() > 16 * 1024 * 1024 {
        bail!("source exceeds 16 MiB: {}", path.display());
    }
    SourceFile::read(path)
}

struct CompiledPaper {
    document: Document,
    pdf: Vec<u8>,
    assets: Vec<figures::Asset>,
    cacheable: bool,
}

#[derive(Serialize, Deserialize)]
struct PaperResult {
    document: Document,
    assets: Vec<figures::Asset>,
}

impl Source {
    fn tools(&self) -> (Option<String>, bool) {
        (
            self.snapshot
                .details
                .bibliography
                .as_ref()
                .map(|_| self.snapshot.details.bibliography_style.clone()),
            self.snapshot.expanded.contains("\\begin{tikzpicture}")
                || self
                    .snapshot
                    .resources
                    .iter()
                    .any(|r| r.relative.extension().is_some_and(|e| e == "pdf")),
        )
    }
}

pub(super) fn compile(
    project_root: &Path,
    sources: &[Source],
    output: &Path,
    url_prefix: &str,
    no_cache: bool,
) -> Result<DocumentCollection, Error> {
    let cache = ArtifactCache::new(project_root);
    let fingerprints = toolchain_fingerprints(sources, no_cache)?;
    let mut documents = Vec::with_capacity(sources.len());
    fs::create_dir_all(output)?;
    for source in sources {
        let key = fingerprints
            .get(&source.tools())
            .map(|tools| cache_key(source, url_prefix, tools))
            .transpose()?;
        let cached = key
            .as_ref()
            .and_then(|key| cache.load::<PaperResult>(key))
            .filter(|artifact| valid_cached_paper(artifact, source));
        let artifact = match cached {
            Some(artifact) => {
                log::info!("Reusing paper {}", source.text.path.display());
                artifact
            }
            None => {
                log::info!("Compiling paper {}", source.text.path.display());
                let paper = compile_one(source, url_prefix)
                    .with_context(|| format!("unable to compile {}", source.text.path.display()))?;
                let artifact = Artifact {
                    value: PaperResult {
                        document: paper.document,
                        assets: paper.assets,
                    },
                    bytes: paper.pdf,
                };
                if paper.cacheable
                    && let Some(key) = &key
                {
                    cache.store(key, &artifact);
                }
                artifact
            }
        };
        // Copy bytes into this run's transaction even on a hit. A cache entry is
        // never an alternative publication root or an excuse to skip validation.
        fs::write(
            output.join(format!("{}.pdf", digest(&artifact.bytes))),
            &artifact.bytes,
        )?;
        if !artifact.value.assets.is_empty() {
            fs::create_dir_all(output.join("assets"))?;
        }
        for asset in artifact.value.assets {
            if !asset.valid() {
                bail!("invalid cached figure identity");
            }
            fs::write(output.join("assets").join(asset.name), asset.bytes)?;
        }
        documents.push(artifact.value.document);
    }
    Ok(DocumentCollection::new(documents).validated()?)
}

fn toolchain_fingerprints(
    sources: &[Source],
    no_cache: bool,
) -> Result<std::collections::BTreeMap<(Option<String>, bool), String>, Error> {
    let mut fingerprints = std::collections::BTreeMap::new();
    if no_cache {
        return Ok(fingerprints);
    }
    // A newly added BibTeX paper must not change keys for papers without a
    // bibliography. Probe each required tool set once, not once per source.
    for tools in sources
        .iter()
        .map(Source::tools)
        .collect::<std::collections::BTreeSet<_>>()
    {
        match toolchain::fingerprint(tools.0.as_deref(), tools.1) {
            Ok(Some(value)) => {
                fingerprints.insert(tools, value);
            }
            Err(error) if error.is::<crate::util::process::Interrupted>() => return Err(error),
            result => {
                log::info!("Paper cache disabled: toolchain cannot be verified as immutable");
                if let Err(error) = result {
                    log::debug!("Paper cache probe: {error:#}");
                }
            }
        }
    }
    Ok(fingerprints)
}

fn cache_key(source: &Source, url_prefix: &str, tools: &str) -> Result<CacheKey, Error> {
    CacheKey::new(
        "compile-papers/v2",
        &(
            digest(source.text.text.as_bytes()),
            digest(source.snapshot.expanded.as_bytes()),
            source
                .snapshot
                .resources
                .iter()
                .map(|r| (&r.relative, digest(&r.bytes)))
                .collect::<Vec<_>>(),
            source
                .snapshot
                .dependencies
                .iter()
                .map(|s| (&s.uri, digest(s.text.as_bytes())))
                .collect::<Vec<_>>(),
            &source.text.uri,
            &source.id,
            &source.published,
            url_prefix,
            tools,
        ),
    )
}

fn valid_cached_paper(artifact: &Artifact<PaperResult>, source: &Source) -> bool {
    let document = &artifact.value.document;
    artifact.bytes.starts_with(b"%PDF-")
        && artifact.value.assets.iter().all(figures::Asset::valid)
        && document.id == source.id
        && document.provenance.source == source.text.uri
        && document.provenance.source_hash == digest(source.text.text.as_bytes())
        && document.provenance.source_format == SourceFormat::Latex
        && document.metadata.published.as_ref() == Some(&source.published)
        && document.validate().is_ok()
        && berlin_document_html::Renderer::default()
            .render(document)
            .is_ok()
}

fn compile_one(source: &Source, url_prefix: &str) -> Result<CompiledPaper, Error> {
    let text = &source.snapshot.expanded;
    let bibliography = source.snapshot.details.bibliography.is_some();
    let work = tempfile::Builder::new().prefix("berlin-paper-").tempdir()?;
    fs::write(work.path().join("paper.tex"), text)?;
    for resource in &source.snapshot.resources {
        let path = work.path().join(&resource.relative);
        fs::create_dir_all(path.parent().context("resource has no parent")?)?;
        fs::write(path, &resource.bytes)?;
    }
    let latex_args = [
        "-recorder",
        "-no-shell-escape",
        "-halt-on-error",
        "-interaction=nonstopmode",
        "paper.tex",
    ];
    run(work.path(), "pdflatex", &latex_args)?;
    if bibliography {
        run(work.path(), "bibtex", &["paper"])?;
    }
    run(work.path(), "pdflatex", &latex_args)?;
    let previous_aux = read_text(&work.path().join("paper.aux"))?;
    run(work.path(), "pdflatex", &latex_args)?;
    let aux = read_text(&work.path().join("paper.aux"))?;
    if previous_aux != aux {
        bail!("paper labels did not stabilize after compilation");
    }
    let log = read_text(&work.path().join("paper.log"))?;
    for warning in [
        "undefined references",
        "multiply defined",
        "Rerun to get",
        "There were undefined citations",
    ] {
        if log.contains(warning) {
            bail!("LaTeX reported {warning}");
        }
    }
    let figures = figures::prepare(source, work.path(), url_prefix)?;
    fs::write(work.path().join("web.tex"), &figures.html_source)?;
    let json = run(
        work.path(),
        "pandoc",
        &[
            "web.tex",
            "--from=latex+raw_tex+latex_macros",
            "--to=json",
            "--fail-if-warnings",
        ],
    )?;
    let mut bibliography_entries = Vec::new();
    if bibliography {
        for (key, latex) in
            berlin_paper::bibtex_entries(&read_text(&work.path().join("paper.bbl"))?)?
        {
            fs::write(work.path().join("bibliography-item.tex"), latex)?;
            let pandoc_json = run(
                work.path(),
                "pandoc",
                &[
                    "bibliography-item.tex",
                    "--from=latex+raw_tex",
                    "--to=json",
                    "--fail-if-warnings",
                ],
            )?;
            bibliography_entries.push(berlin_paper::BibliographyEntry { key, pandoc_json });
        }
    }
    let pdf = fs::read(work.path().join("paper.pdf"))?;
    if !pdf.starts_with(b"%PDF-") {
        bail!("compiler did not produce a PDF");
    }

    let filename = format!("{}.pdf", digest(&pdf));
    let mut ast: serde_json::Value = serde_json::from_str(&json)?;
    figures::rewrite_images(&mut ast, &figures.urls)?;
    let document = berlin_paper::import(
        &serde_json::to_string(&ast)?,
        &berlin_paper::Labels::parse(&aux)?,
        &bibliography_entries,
        berlin_paper::Publication {
            id: source.id.clone(),
            metadata: Metadata {
                published: Some(source.published.clone()),
                ..Default::default()
            },
            provenance: Provenance {
                source: source.text.uri.clone(),
                source_format: SourceFormat::Latex,
                source_hash: digest(source.text.text.as_bytes()),
            },
            pdf_url: format!("{}/{}", url_prefix.trim_end_matches('/'), filename),
        },
    )?;
    let _ = berlin_document_html::Renderer::default().render(&document)?;
    Ok(CompiledPaper {
        document,
        pdf,
        assets: figures.assets,
        cacheable: toolchain::immutable_inputs(work.path()).unwrap_or(false),
    })
}

fn run(directory: &Path, program: &str, args: &[&str]) -> Result<String, Error> {
    crate::util::process::run(
        Command::new(program)
            .args(args)
            .current_dir(directory)
            .env("openin_any", "p")
            .env("openout_any", "p")
            .env("SOURCE_DATE_EPOCH", "315532800")
            .env("FORCE_SOURCE_DATE", "1"),
        crate::util::process::Limits::default(),
    )
    .with_context(|| format!("paper tool {program}; use the optional Nix papers shell"))
}

fn read_text(path: &Path) -> Result<String, Error> {
    if fs::metadata(path)?.len() > 16 * 1024 * 1024 {
        bail!("input exceeds 16 MiB: {}", path.display());
    }
    fs::read_to_string(path).with_context(|| format!("unable to read {}", path.display()))
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "\\documentclass{article}\n\\usepackage{hyperref}\n\\title{Example}\n\\author{Author}\n\\date{2026-09-09}\n\\begin{document}\n\\section{Note}\\label{sec:note}\nText.\n\\end{document}";

    #[test]
    fn source_owned_metadata_overrides_iso_date_and_path_defaults() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("paper.tex");
        fs::write(&path, SOURCE).unwrap();
        let project = Project::new(root.path().to_owned(), vec![]);
        let sources = load(&project, "*.tex").unwrap();
        assert_eq!(sources[0].id.0, "latex:/paper.tex");
        assert_eq!(sources[0].published.to_string(), "2026-09-09");
        fs::write(
            &path,
            format!("% berlin: {{\"id\":\"stable\",\"published\":\"2020-01-02\"}}\n{SOURCE}"),
        )
        .unwrap();
        let sources = load(&project, "*.tex").unwrap();
        assert_eq!(sources[0].id.0, "stable");
        assert_eq!(sources[0].published.to_string(), "2020-01-02");
        fs::write(root.path().join("duplicate.tex"), fs::read(&path).unwrap()).unwrap();
        assert!(load(&project, "*.tex").is_err());
    }

    #[test]
    fn invalid_or_missing_dates_are_not_inferred_and_metadata_typos_fail() {
        let root = tempfile::tempdir().unwrap();
        let project = Project::new(root.path().to_owned(), vec![]);
        for source in [
            SOURCE.replace("2026-09-09", "September 9, 2026"),
            SOURCE.replace("\\date{2026-09-09}", ""),
            format!("% berlin: {{\"publised\":\"2020-01-02\"}}\n{SOURCE}"),
        ] {
            fs::write(root.path().join("paper.tex"), source).unwrap();
            assert!(load(&project, "*.tex").is_err());
        }
    }

    #[test]
    fn named_roots_supply_portable_identity_and_snapshot_external_sources() {
        let root = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        fs::write(data.path().join("paper.tex"), SOURCE).unwrap();
        fs::write(
            root.path().join("berlin.pipeline.rhai"),
            format!(
                "const SOURCE_ROOTS = #{{papers: {:?}}};",
                data.path().to_str().unwrap()
            ),
        )
        .unwrap();
        let project = Project::new(root.path().to_owned(), vec![]);
        let sources = load(&project, "@papers/*.tex").unwrap();
        assert_eq!(sources[0].id.0, "latex:@papers/paper.tex");
        fs::write(data.path().join("paper.tex"), "changed").unwrap();
        assert_eq!(sources[0].text.text, SOURCE);
    }

    #[test]
    fn paper_keys_cover_source_bibliography_identity_options_and_tools() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("paper.tex"), SOURCE).unwrap();
        let project = Project::new(root.path().to_owned(), vec![]);
        let source = load(&project, "*.tex").unwrap().remove(0);
        let key = cache_key(&source, "/papers", "toolchain-v1").unwrap();
        assert_ne!(
            key,
            cache_key(&source, "/downloads", "toolchain-v1").unwrap()
        );
        assert_ne!(key, cache_key(&source, "/papers", "toolchain-v2").unwrap());
        let mut variants = Vec::new();
        let mut changed = source.clone();
        changed.text.text.push_str("\n% edit");
        variants.push(changed);
        let mut changed = source.clone();
        changed.id = ContentId("another".into());
        variants.push(changed);
        let mut changed = source.clone();
        changed.published = "2020-01-01".parse().unwrap();
        variants.push(changed);
        let mut changed = source.clone();
        changed.text.uri.push_str(".moved");
        variants.push(changed);
        let mut changed = source.clone();
        changed.snapshot.dependencies.push(source.text.clone());
        let bib_key = cache_key(&changed, "/papers", "toolchain-v1").unwrap();
        variants.push(changed.clone());
        changed.snapshot.dependencies[0].text.push_str("edited");
        assert_ne!(
            bib_key,
            cache_key(&changed, "/papers", "toolchain-v1").unwrap()
        );
        for changed in variants {
            assert_ne!(key, cache_key(&changed, "/papers", "toolchain-v1").unwrap());
        }
    }
}
