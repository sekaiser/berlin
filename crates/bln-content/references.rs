//! Backlinks derived from authored links within one publication, never from tags.

use std::collections::{BTreeMap, HashSet};

use berlin_document::{ComponentId, ContentId};
use serde::{Deserialize, Serialize};

use crate::{CollectionError, DocumentCollection, DocumentRef};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Backlink {
    pub source: DocumentRef,
    pub anchor: ComponentId,
    pub excerpt: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReferenceIndex {
    incoming: BTreeMap<String, Vec<Backlink>>,
}

/// A reference whose target is absent from this publication, including drafts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnresolvedReference {
    pub source: DocumentRef,
    pub source_uri: String,
    pub anchor: ComponentId,
    pub target: ContentId,
    pub excerpt: String,
}

/// Retains valid connections while collecting every unresolved occurrence.
pub struct ReferenceAnalysis {
    pub index: ReferenceIndex,
    pub unresolved: Vec<UnresolvedReference>,
}

impl ReferenceIndex {
    /// Only non-draft documents can contribute targets, excerpts or backlinks.
    pub fn new(documents: &DocumentCollection) -> Result<Self, CollectionError> {
        let analysis = Self::analyze(documents);
        if let Some(reference) = analysis.unresolved.into_iter().next() {
            return Err(CollectionError::UnresolvedReference {
                source: reference.source_uri,
                anchor: reference.anchor.0,
                target: reference.target,
            });
        }
        Ok(analysis.index)
    }

    /// Analyzes the same publication scope as `new`, without failing at its first
    /// unresolved link. Callers must validate document identities beforehand.
    pub fn analyze(documents: &DocumentCollection) -> ReferenceAnalysis {
        let targets: HashSet<_> = documents
            .non_drafts()
            .map(|document| &document.id)
            .collect();
        let mut incoming = BTreeMap::<String, Vec<Backlink>>::new();
        let mut unresolved = Vec::new();
        for document in documents.non_drafts() {
            let mut seen = HashSet::new();
            for reference in document.references() {
                let link = reference.link;
                if !targets.contains(&link.target) {
                    unresolved.push(UnresolvedReference {
                        source: DocumentRef::from(document),
                        source_uri: document.provenance.source.clone(),
                        anchor: link.anchor.clone(),
                        target: link.target.clone(),
                        excerpt: reference.excerpt,
                    });
                    continue;
                }
                // Self-links remain usable but do not advertise this page to itself.
                if link.target == document.id {
                    continue;
                }
                let backlinks = incoming.entry(link.target.0.clone()).or_default();
                // Multiple mentions in one passage need only one contextual entry.
                if !seen.insert((link.target.clone(), reference.passage)) {
                    continue;
                }
                backlinks.push(Backlink {
                    source: DocumentRef::from(document),
                    anchor: link.anchor.clone(),
                    excerpt: reference.excerpt,
                });
            }
        }
        for backlinks in incoming.values_mut() {
            backlinks.sort_by(|left, right| {
                left.source
                    .0
                    .0
                    .cmp(&right.source.0.0)
                    .then_with(|| left.anchor.0.cmp(&right.anchor.0))
            });
        }
        ReferenceAnalysis {
            index: Self { incoming },
            unresolved,
        }
    }

    pub fn incoming(&self, target: &ContentId) -> &[Backlink] {
        self.incoming
            .get(&target.0)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}
