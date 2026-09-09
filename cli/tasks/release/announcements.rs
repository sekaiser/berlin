//! Local announcement planning from sealed revisions and opt-in, no-clobber export.
//! Does not execute pipelines, infer live state, or send anything to a platform.

use super::{
    diff::ReleaseSummary,
    documents::{DocumentComparison, DocumentRevision},
};
use crate::project::Project;
use anyhow::{Context, Error, bail};
use berlin_content::DocumentRef;
use berlin_document_linkedin::{Announcement, AnnouncementKind, LinkedInPostDraft, Renderer};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};
use url::Url;

#[derive(Debug, Serialize)]
pub(crate) struct AnnouncementPlan {
    schema_version: u8,
    channel: &'static str,
    pub before: ReleaseSummary,
    pub after: ReleaseSummary,
    review_required: bool,
    live_status_checked: bool,
    pub review_notes: &'static str,
    pub drafts: Vec<AnnouncementDraft>,
    pub skipped: SkippedDocuments,
}

#[derive(Debug, Serialize)]
pub(crate) struct AnnouncementDraft {
    pub kind: AnnouncementKind,
    pub document: DocumentRevision,
    pub url: Url,
    pub draft: LinkedInPostDraft,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct SkippedDocuments {
    pub metadata_or_route_only: usize,
    pub removed: usize,
    pub unchanged: usize,
}

pub(crate) fn prepare(
    project: &Project,
    before: &str,
    after: &str,
) -> Result<AnnouncementPlan, Error> {
    let comparison = super::diff::compare(project, before, after)?;
    AnnouncementPlan::new(comparison.before, comparison.after, comparison.documents)
}

impl AnnouncementPlan {
    fn new(
        before: ReleaseSummary,
        after: ReleaseSummary,
        documents: DocumentComparison,
    ) -> Result<Self, Error> {
        let DocumentComparison::Compared { changes } = documents else {
            bail!(
                "announcement drafts require document inventories in both releases; legacy and imported releases support file comparison only"
            );
        };
        if before.metadata.destination != after.metadata.destination
            || before.metadata.url != after.metadata.url
        {
            bail!(
                "announcement drafts require the same website URL and destination; use release-diff to inspect a migration"
            );
        }
        let renderer = Renderer::default();
        let url = &after.metadata.url;
        let mut drafts = changes
            .added
            .into_iter()
            .map(|document| AnnouncementDraft::new(document, AnnouncementKind::New, url, &renderer))
            .collect::<Result<Vec<_>, _>>()?;
        let mut skipped = SkippedDocuments {
            removed: changes.removed.len(),
            unchanged: changes.unchanged,
            ..SkippedDocuments::default()
        };
        for modified in changes.modified {
            if modified.content_changed {
                drafts.push(AnnouncementDraft::new(
                    modified.after,
                    AnnouncementKind::Updated,
                    url,
                    &renderer,
                )?);
            } else {
                skipped.metadata_or_route_only += 1;
            }
        }
        drafts.sort_by(|left, right| left.document.id.0.cmp(&right.document.id.0));
        Ok(Self {
            schema_version: 1,
            channel: "linkedin",
            before,
            after,
            review_required: true,
            live_status_checked: false,
            review_notes: "Draft starting points, not summaries. Add context, decide whether the change merits an announcement, and verify the intended release and URL are live before posting manually. Nothing has been published or approved.",
            drafts,
            skipped,
        })
    }
}

impl AnnouncementDraft {
    fn new(
        document: DocumentRevision,
        kind: AnnouncementKind,
        base: &Url,
        renderer: &Renderer,
    ) -> Result<Self, Error> {
        let url = document_url(base, &document.path)?;
        let source = DocumentRef(document.id.clone());
        let draft = renderer.render_announcement(&Announcement {
            source: &source,
            title: &document.title,
            url: &url,
            kind,
        });
        Ok(Self {
            kind,
            document,
            url,
            draft,
        })
    }
}

/// Treat the release URL's path as a site prefix, including when it lacks a
/// trailing slash. Encode each literal output component, never interpret it as
/// an absolute URL, query, fragment, or traversal instruction.
fn document_url(base: &Url, path: &str) -> Result<Url, Error> {
    let mut url = base.clone();
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| anyhow::anyhow!("cannot append a document path to the website URL"))?;
    segments.pop_if_empty();
    for segment in path.split('/') {
        if segment.is_empty()
            || matches!(segment, "." | "..")
            || segment.contains('\\')
            || segment.chars().any(char::is_control)
        {
            bail!("unsafe document route in announcement: {path:?}");
        }
        segments.push(segment);
    }
    drop(segments);
    Ok(url)
}

/// Explicit export to a new review directory. Existing directories (including
/// empty ones) are never reused, so rerunning cannot replace hand-edited drafts.
pub(crate) fn export(
    project: &Project,
    plan: &AnnouncementPlan,
    output: &Path,
) -> Result<PathBuf, Error> {
    let output = export_directory(project, output)?;
    let mut names = HashSet::new();
    let mut files = Vec::with_capacity(plan.drafts.len() + 1);
    for item in &plan.drafts {
        let name = &item.draft.artifact_name;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            bail!("invalid announcement filename: {name:?}");
        }
        let filename = format!("{name}.txt");
        if !names.insert(filename.to_ascii_lowercase()) {
            bail!("announcement filenames collide: {filename:?}");
        }
        files.push((filename, format!("{}\n", item.draft.text)));
    }
    // Write the complete manifest last. If I/O fails, leave the new directory
    // for inspection; do not risk deleting a review somebody has started editing.
    files.push((
        "manifest.json".into(),
        format!("{}\n", serde_json::to_string_pretty(plan)?),
    ));
    fs::create_dir(&output).with_context(|| {
        format!(
            "cannot create review directory {}; it must not already exist",
            output.display()
        )
    })?;
    for (name, text) in files {
        write_new_file(&output.join(name), text.as_bytes()).with_context(|| {
            format!(
                "draft export incomplete at {}; existing files were not replaced",
                output.display()
            )
        })?;
    }
    Ok(output)
}

fn export_directory(project: &Project, output: &Path) -> Result<PathBuf, Error> {
    let output = project.root().join(output);
    let name = output
        .file_name()
        .context("review directory requires a name")?;
    let parent = output
        .parent()
        .context("review directory requires a parent")?
        .canonicalize()
        .context("review directory's parent must already exist")?;
    let output = parent.join(name);
    let root = project.root().canonicalize()?;
    for protected in [".berlin/releases", ".berlin/publications", ".git"] {
        if output.starts_with(root.join(protected)) {
            bail!("review drafts cannot be exported into release, publication, or Git state");
        }
    }
    Ok(output)
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut file = File::options().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests;
