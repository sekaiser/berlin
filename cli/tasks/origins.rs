//! Local Org navigation sidecars, bound to exact Markdown exports. No render data.

use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Error, bail};
use berlin_content::{AuthoringOrigin, AuthoringReport, DocumentCollection, OriginHeading};
use berlin_document::Document;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const DIRECTORY: &str = "_berlin/org-origins";

#[derive(Deserialize)]
pub(super) struct ExportOrigins {
    pub schema_version: u32,
    pub documents: Vec<ExportOrigin>,
}

#[derive(Deserialize)]
pub(super) struct ExportOrigin {
    pub document: String,
    pub source: PathBuf,
    pub source_hash: String,
    pub markdown: PathBuf,
    pub markdown_hash: String,
    pub headings: Vec<HeadingOrigin>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct HeadingOrigin {
    fragment: String,
    heading: OriginHeading,
}

#[derive(Deserialize, Serialize)]
struct StoredOrigin {
    schema_version: u32,
    document: String,
    /// Project-relative authoring path; no workstation path in the cache itself.
    source: PathBuf,
    source_hash: String,
    markdown_hash: String,
    headings: Vec<HeadingOrigin>,
}

/// Content-addressed maps from an uncommitted export cannot match older Markdown.
/// Old maps may coexist; checks never create or update this directory.
pub(super) fn store(
    root: &Path,
    published_markdown: &Path,
    origin: ExportOrigin,
) -> Result<(), Error> {
    let source = origin
        .source
        .strip_prefix(root)
        .context("Org origin is outside the project")?
        .to_owned();
    if !confined(&source) {
        bail!("Org origin must be a confined project-relative path");
    }
    let stored = StoredOrigin {
        schema_version: 1,
        document: origin.document,
        source,
        source_hash: origin.source_hash,
        markdown_hash: origin.markdown_hash,
        headings: origin.headings,
    };
    let destination = cache_path(root, published_markdown, &stored.markdown_hash)?;
    let directory = root.join(DIRECTORY);
    fs::create_dir_all(&directory)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    serde_json::to_writer(&mut temporary, &stored)?;
    temporary
        .persist(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

pub(super) fn enrich(root: &Path, documents: &DocumentCollection, report: &mut AuthoringReport) {
    let documents: HashMap<_, _> = documents
        .non_drafts()
        .map(|document| (&document.id, document))
        .collect();
    let mut origins = HashMap::new();
    for finding in &mut report.findings {
        let Some(document) = documents.get(&finding.location.document) else {
            continue;
        };
        let origin = origins.entry(&document.id).or_insert_with(|| {
            load(root, document).unwrap_or_else(|error| {
                log::warn!("Ignoring local Org origin for '{}': {error}", document.id.0);
                None
            })
        });
        let Some((stored, source_uri, stale)) = origin else {
            continue;
        };
        let section = finding.location.anchor.as_ref().and_then(|anchor| {
            document
                .references()
                .into_iter()
                .find(|reference| &reference.link.anchor == anchor)
                .and_then(|reference| reference.section)
        });
        let heading = if *stale {
            None
        } else {
            section.and_then(|section| unique_heading(&stored.headings, &section.0))
        };
        finding.location.origin = Some(AuthoringOrigin {
            source: source_uri.clone(),
            source_hash: stored.source_hash.clone(),
            heading,
            stale: *stale,
        });
    }
}

fn unique_heading(headings: &[HeadingOrigin], fragment: &str) -> Option<OriginHeading> {
    let mut matches = headings
        .iter()
        .filter(|heading| heading.fragment == fragment);
    let first = matches.next()?;
    matches.next().is_none().then(|| first.heading.clone())
}

fn load(root: &Path, document: &Document) -> Result<Option<(StoredOrigin, String, bool)>, Error> {
    let Some(markdown) = url::Url::parse(&document.provenance.source)
        .ok()
        .and_then(|uri| uri.to_file_path().ok())
    else {
        return Ok(None);
    };
    let path = cache_path(root, &markdown, &document.provenance.source_hash)?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let stored: StoredOrigin = serde_json::from_slice(&bytes)?;
    if stored.schema_version != 1
        || stored.document != document.id.0
        || stored.markdown_hash != document.provenance.source_hash
        || !confined(&stored.source)
    {
        bail!("Source map does not match this document or contains an unsafe path");
    }
    let source = root.join(&stored.source);
    let canonical = match source.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !canonical.starts_with(root.canonicalize()?) {
        bail!("Org source resolves outside the project");
    }
    let stale = digest(&fs::read(&source)?) != stored.source_hash;
    let uri = url::Url::from_file_path(&source)
        .map_err(|_| anyhow::anyhow!("Invalid Org source path"))?;
    Ok(Some((stored, uri.to_string(), stale)))
}

fn confined(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn cache_path(root: &Path, markdown: &Path, hash: &str) -> Result<PathBuf, Error> {
    let relative = markdown
        .strip_prefix(root)
        .context("Markdown source is outside the project")?;
    if !confined(relative) {
        bail!("Invalid Markdown source location");
    }
    let key = digest(format!("{}\0{hash}", relative.display()).as_bytes());
    Ok(root.join(DIRECTORY).join(format!("{key}.json")))
}

pub(super) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path) -> (DocumentCollection, ExportOrigin) {
        let source = root.join("note.org");
        let markdown = root.join("note.md");
        fs::write(&source, "* Details\nAn Org source.\n").unwrap();
        let text = "---\nid: note\n---\n## Details {#details}\n\nSee [missing](id:missing).\n";
        fs::write(&markdown, text).unwrap();
        let uri = url::Url::from_file_path(&markdown).unwrap();
        let document = markdown::Parser::new()
            .parse(&markdown::Source::new(text, uri.as_str()).unwrap())
            .unwrap();
        let origin = ExportOrigin {
            document: "note".into(),
            source,
            source_hash: digest(b"* Details\nAn Org source.\n"),
            markdown,
            markdown_hash: digest(text.as_bytes()),
            headings: vec![HeadingOrigin {
                fragment: "details".into(),
                heading: OriginHeading {
                    id: Some("heading-id".into()),
                    outline: vec!["Details".into()],
                },
            }],
        };
        (DocumentCollection::new(vec![document]), origin)
    }

    fn report(root: &Path, documents: &DocumentCollection) -> AuthoringReport {
        let mut report = AuthoringReport::analyze(documents).unwrap();
        enrich(root, documents, &mut report);
        report
    }

    #[test]
    fn attaches_heading_to_reference_and_only_file_to_document_observations() {
        let root = tempfile::tempdir().unwrap();
        let (documents, origin) = fixture(root.path());
        store(root.path(), &origin.markdown.clone(), origin).unwrap();
        let report = report(root.path(), &documents);
        for finding in report.findings {
            let origin = finding.location.origin.unwrap();
            assert!(!origin.stale);
            assert!(origin.source.ends_with("/note.org"));
            assert_eq!(origin.heading.is_some(), finding.location.anchor.is_some());
            if let Some(heading) = origin.heading {
                assert_eq!(heading.id.as_deref(), Some("heading-id"));
            }
        }
        let bytes = fs::read(
            cache_path(
                root.path(),
                &root.path().join("note.md"),
                &documents.as_slice()[0].provenance.source_hash,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(
            !String::from_utf8(bytes)
                .unwrap()
                .contains(root.path().to_str().unwrap())
        );
    }

    #[test]
    fn changed_org_falls_back_to_file_and_changed_markdown_has_no_mapping() {
        let root = tempfile::tempdir().unwrap();
        let (documents, origin) = fixture(root.path());
        store(root.path(), &origin.markdown.clone(), origin).unwrap();
        fs::write(root.path().join("note.org"), "* Changed\n").unwrap();
        for finding in report(root.path(), &documents).findings {
            let origin = finding.location.origin.unwrap();
            assert!(origin.stale);
            assert!(origin.heading.is_none());
        }
        let mut changed = documents.into_vec();
        changed[0].provenance.source_hash = digest(b"different export");
        assert!(
            report(root.path(), &DocumentCollection::new(changed))
                .findings
                .iter()
                .all(|finding| finding.location.origin.is_none())
        );
    }

    #[test]
    fn missing_and_invalid_sidecars_do_not_prevent_analysis() {
        let root = tempfile::tempdir().unwrap();
        let (documents, origin) = fixture(root.path());
        assert!(
            report(root.path(), &documents)
                .findings
                .iter()
                .all(|f| f.location.origin.is_none())
        );
        let cache = cache_path(root.path(), &origin.markdown, &origin.markdown_hash).unwrap();
        store(root.path(), &origin.markdown.clone(), origin).unwrap();
        fs::write(cache, "invalid json").unwrap();
        let report = report(root.path(), &documents);
        assert!(report.has_errors());
        assert!(report.findings.iter().all(|f| f.location.origin.is_none()));
    }

    #[test]
    fn rejects_unsafe_and_ambiguous_origins() {
        let root = tempfile::tempdir().unwrap();
        let (documents, mut origin) = fixture(root.path());
        origin.headings.push(origin.headings[0].clone());
        store(root.path(), &origin.markdown.clone(), origin).unwrap();
        assert!(
            report(root.path(), &documents).findings.iter().all(|f| f
                .location
                .origin
                .as_ref()
                .unwrap()
                .heading
                .is_none())
        );
        for path in ["", "../outside", "/absolute", "notes/../../outside"] {
            assert!(!confined(Path::new(path)));
        }
        let cache = cache_path(
            root.path(),
            &root.path().join("note.md"),
            &documents.as_slice()[0].provenance.source_hash,
        )
        .unwrap();
        let mut stored: StoredOrigin = serde_json::from_slice(&fs::read(&cache).unwrap()).unwrap();
        stored.source = "../outside.org".into();
        fs::write(&cache, serde_json::to_vec(&stored).unwrap()).unwrap();
        assert!(load(root.path(), &documents.as_slice()[0]).is_err());
    }

    #[test]
    fn different_export_maps_coexist_without_replacing_the_current_export() {
        let root = tempfile::tempdir().unwrap();
        let (documents, origin) = fixture(root.path());
        store(root.path(), &origin.markdown.clone(), origin).unwrap();
        let (_, mut next) = fixture(root.path());
        next.markdown_hash = digest(b"next export not committed");
        next.headings.clear();
        store(root.path(), &next.markdown.clone(), next).unwrap();
        assert_eq!(
            fs::read_dir(root.path().join(DIRECTORY)).unwrap().count(),
            2
        );
        let report = report(root.path(), &documents);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.location.origin.as_ref().unwrap().heading.is_some())
        );
    }
}
