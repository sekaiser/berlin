//! Versioned fingerprints of the mapped, public documents supplied to a website.
//! Stores identities and hashes, never source paths, full bodies, or draft content.

use super::{ReleaseFile, digest, validate_id};
use anyhow::{Context, Error, bail};
use berlin_content::WebsiteAssembly;
use berlin_document::{Block, ContentId, Document, DocumentKind, Relation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// Bump the fingerprint version when changing either projection's serialization.
/// This version compares structural document values, not equivalent meanings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentInventory {
    pub fingerprint_version: u8,
    pub entries: Vec<DocumentRevision>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentRevision {
    pub id: ContentId,
    pub title: String,
    pub path: String,
    pub content_sha256: String,
    pub metadata_sha256: String,
}

#[derive(Serialize)]
struct Content<'a> {
    kind: &'a DocumentKind,
    blocks: &'a [Block],
    relations: &'a [Relation],
}

impl DocumentRevision {
    fn from_document(document: &Document) -> Result<Self, Error> {
        let content = Content {
            kind: &document.kind,
            blocks: &document.blocks,
            relations: &document.relations,
        };
        Ok(Self {
            id: document.id.clone(),
            title: document
                .metadata
                .title
                .clone()
                .context("document has no title")?,
            path: crate::tasks::website::document_route(document)?,
            content_sha256: digest(&serde_json::to_vec(&content)?),
            // Authored dates, tags, descriptions, slugs, etc. are metadata changes,
            // not body edits. Provenance and generated HTML enter neither hash.
            metadata_sha256: digest(&serde_json::to_vec(&document.metadata)?),
        })
    }
}

impl DocumentInventory {
    pub(super) fn from_site(site: &berlin_publication::SiteAssembly) -> Result<Self, Error> {
        let mut entries = Vec::with_capacity(site.document_routes().len());
        for (id, route) in site.document_routes() {
            let document = site.catalog().get(id)?.handle().try_local_document()?;
            let mut revision = DocumentRevision::from_document(document)?;
            revision.path = route.clone();
            entries.push(revision);
        }
        entries.sort_by(|left, right| left.id.0.cmp(&right.id.0));
        Ok(Self {
            fingerprint_version: 1,
            entries,
        })
    }
    pub(super) fn from_website(website: &WebsiteAssembly) -> Result<Self, Error> {
        let mut entries = website
            .documents()
            .as_slice()
            .iter()
            .map(DocumentRevision::from_document)
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by(|left, right| left.id.0.cmp(&right.id.0));
        Ok(Self {
            fingerprint_version: 1,
            entries,
        })
    }

    pub(super) fn validate(&self, files: &[ReleaseFile]) -> Result<(), Error> {
        if self.fingerprint_version != 1 {
            bail!(
                "unsupported document fingerprint version {}",
                self.fingerprint_version
            );
        }
        if self
            .entries
            .windows(2)
            .any(|pair| pair[0].id.0 >= pair[1].id.0)
        {
            bail!("document inventory IDs must be unique and sorted");
        }
        let paths = files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<HashSet<_>>();
        let mut document_paths = HashSet::new();
        for entry in &self.entries {
            if entry.id.0.trim().is_empty()
                || entry.id.0.chars().any(char::is_control)
                || entry.title.trim().is_empty()
            {
                bail!("document inventory requires nonempty titles and valid IDs");
            }
            if !paths.contains(entry.path.as_str()) || !document_paths.insert(&entry.path) {
                bail!(
                    "document '{}' must identify a unique file in the release",
                    entry.id.0
                );
            }
            validate_id(&entry.content_sha256).context("invalid document content fingerprint")?;
            validate_id(&entry.metadata_sha256).context("invalid document metadata fingerprint")?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum DocumentComparison {
    Unavailable {
        before_available: bool,
        after_available: bool,
    },
    Compared {
        changes: DocumentChanges,
    },
}

impl DocumentComparison {
    pub(super) fn between(
        before: Option<&DocumentInventory>,
        after: Option<&DocumentInventory>,
    ) -> Self {
        match (before, after) {
            (Some(before), Some(after)) => Self::Compared {
                changes: DocumentChanges::between(&before.entries, &after.entries),
            },
            _ => Self::Unavailable {
                before_available: before.is_some(),
                after_available: after.is_some(),
            },
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct DocumentChanges {
    pub added: Vec<DocumentRevision>,
    pub modified: Vec<ModifiedDocument>,
    pub removed: Vec<DocumentRevision>,
    pub unchanged: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct ModifiedDocument {
    pub before: DocumentRevision,
    pub after: DocumentRevision,
    pub content_changed: bool,
    pub metadata_changed: bool,
    pub route_changed: bool,
}

impl DocumentChanges {
    fn between(before: &[DocumentRevision], after: &[DocumentRevision]) -> Self {
        let mut remaining = before
            .iter()
            .map(|entry| (&entry.id.0, entry))
            .collect::<BTreeMap<_, _>>();
        let mut changes = Self::default();
        // Inventories and the remaining-ID map retain deterministic ID ordering.
        for new in after {
            match remaining.remove(&new.id.0) {
                None => changes.added.push(new.clone()),
                Some(old) if old == new => changes.unchanged += 1,
                Some(old) => changes.modified.push(ModifiedDocument {
                    before: old.clone(),
                    after: new.clone(),
                    content_changed: old.content_sha256 != new.content_sha256,
                    metadata_changed: old.metadata_sha256 != new.metadata_sha256
                        || old.title != new.title,
                    route_changed: old.path != new.path,
                }),
            }
        }
        changes.removed.extend(remaining.into_values().cloned());
        changes
    }
}

#[cfg(test)]
mod tests;
