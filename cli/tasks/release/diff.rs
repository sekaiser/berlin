//! Comparison of verified local release snapshots, not deployment state.
use super::documents::DocumentComparison;
use super::{Manifest, ReleaseFile, WebsiteRelease};
use crate::project::Project;
use anyhow::{Context, Error};
use berlin_core::DeploymentTarget;
use serde::Serialize;
use std::cmp::Ordering;
use url::Url;

#[derive(Debug, Serialize)]
pub(crate) struct ReleaseComparison {
    schema_version: u8,
    pub before: ReleaseSummary,
    pub after: ReleaseSummary,
    pub metadata_changed: bool,
    pub files: FileChanges,
    pub documents: DocumentComparison,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReleaseSummary {
    pub id: String,
    pub metadata: ReleaseMetadata,
}

impl From<&WebsiteRelease> for ReleaseSummary {
    fn from(release: &WebsiteRelease) -> Self {
        Self {
            id: release.id.clone(),
            metadata: ReleaseMetadata::from(&release.manifest),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ReleaseMetadata {
    pub berlin_version: String,
    pub pipeline: String,
    pub website_node: String,
    pub url: Url,
    pub destination: DeploymentTarget,
}

impl From<&Manifest> for ReleaseMetadata {
    fn from(manifest: &Manifest) -> Self {
        Self {
            berlin_version: manifest.berlin_version.clone(),
            pipeline: manifest.pipeline.clone(),
            website_node: manifest.website_node.clone(),
            url: manifest.url.clone(),
            destination: manifest.destination.clone(),
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct FileChanges {
    pub added: Vec<ReleaseFile>,
    pub modified: Vec<ModifiedFile>,
    pub removed: Vec<ReleaseFile>,
    pub unchanged: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct ModifiedFile {
    pub before: ReleaseFile,
    pub after: ReleaseFile,
}

impl FileChanges {
    /// Verified inventories are unique and sorted by path. Merge them in linear
    /// time without an index or re-sorting; unchanged files need no copies.
    fn between(before: &[ReleaseFile], after: &[ReleaseFile]) -> Self {
        let mut before = before.iter().peekable();
        let mut after = after.iter().peekable();
        let mut changes = Self::default();
        while let (Some(old), Some(new)) = (before.peek(), after.peek()) {
            match old.path.cmp(&new.path) {
                Ordering::Less => {
                    changes.removed.push((*old).clone());
                    before.next();
                }
                Ordering::Greater => {
                    changes.added.push((*new).clone());
                    after.next();
                }
                Ordering::Equal => {
                    if old == new {
                        changes.unchanged += 1;
                    } else {
                        changes.modified.push(ModifiedFile {
                            before: (*old).clone(),
                            after: (*new).clone(),
                        });
                    }
                    before.next();
                    after.next();
                }
            }
        }
        changes.removed.extend(before.cloned());
        changes.added.extend(after.cloned());
        changes
    }
}

/// Deliberately uses no project lock: acquiring the build lock can recover a
/// transaction and write outputs. Both bundles must verify before a report exists.
pub(crate) fn compare(
    project: &Project,
    before: &str,
    after: &str,
) -> Result<ReleaseComparison, Error> {
    let before = super::open(project, before).context("unable to verify BEFORE release")?;
    let after = super::open(project, after).context("unable to verify AFTER release")?;
    let files = FileChanges::between(&before.manifest.files, &after.manifest.files);
    let documents = DocumentComparison::between(
        before.manifest.documents.as_ref(),
        after.manifest.documents.as_ref(),
    );
    let before = ReleaseSummary::from(&before);
    let after = ReleaseSummary::from(&after);
    Ok(ReleaseComparison {
        schema_version: 2,
        metadata_changed: before.metadata != after.metadata,
        before,
        after,
        files,
        documents,
    })
}
