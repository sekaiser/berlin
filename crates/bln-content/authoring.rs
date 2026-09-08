//! Read-only editorial observations and reference errors for one publication.

use std::collections::HashSet;

use berlin_document::{ComponentId, ContentId, Document, DocumentKind};
use serde::{Deserialize, Serialize};

use crate::{CollectionError, DocumentCollection, ReferenceIndex};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OriginHeading {
    pub id: Option<String>,
    pub outline: Vec<String>,
}

/// Optional local authoring navigation; never attached to published documents.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AuthoringOrigin {
    pub source: String,
    pub source_hash: String,
    pub heading: Option<OriginHeading>,
    pub stale: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Observation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum AuthoringIssue {
    UnresolvedReference { target: ContentId },
    DisconnectedNote,
    NotInGuide,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AuthoringLocation {
    pub document: ContentId,
    /// The parsed source URI, not an inferred Org filename.
    pub source: String,
    /// Generated passage anchor; this is not a source line number or permanent ID.
    pub anchor: Option<ComponentId>,
    pub excerpt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<AuthoringOrigin>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AuthoringFinding {
    pub severity: Severity,
    #[serde(flatten)]
    pub issue: AuthoringIssue,
    pub location: AuthoringLocation,
}

impl AuthoringFinding {
    fn observation(document: &Document, issue: AuthoringIssue) -> Self {
        Self {
            severity: Severity::Observation,
            issue,
            location: AuthoringLocation {
                document: document.id.clone(),
                source: document.provenance.source.clone(),
                anchor: None,
                excerpt: None,
                origin: None,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AuthoringReport {
    pub published_documents: usize,
    pub excluded_drafts: usize,
    pub guides: usize,
    pub findings: Vec<AuthoringFinding>,
}

impl AuthoringReport {
    pub fn analyze(documents: &DocumentCollection) -> Result<Self, CollectionError> {
        documents.validate()?;
        let analysis = ReferenceIndex::analyze(documents);
        let published: Vec<_> = documents.non_drafts().collect();
        let guide_ids: HashSet<_> = published
            .iter()
            .filter(|document| document.kind == DocumentKind::Guide)
            .map(|document| &document.id)
            .collect();
        let mut connected = HashSet::new();
        let mut covered = HashSet::new();
        for document in &published {
            for backlink in analysis.index.incoming(&document.id) {
                connected.insert(&document.id);
                connected.insert(&backlink.source.0);
                if guide_ids.contains(&backlink.source.0) {
                    covered.insert(&document.id);
                }
            }
        }

        let mut findings: Vec<_> = analysis
            .unresolved
            .into_iter()
            .map(|reference| AuthoringFinding {
                severity: Severity::Error,
                issue: AuthoringIssue::UnresolvedReference {
                    target: reference.target,
                },
                location: AuthoringLocation {
                    document: reference.source.0,
                    source: reference.source_uri,
                    anchor: Some(reference.anchor),
                    excerpt: Some(reference.excerpt),
                    origin: None,
                },
            })
            .collect();
        for document in &published {
            if !connected.contains(&document.id) {
                findings.push(AuthoringFinding::observation(
                    document,
                    AuthoringIssue::DisconnectedNote,
                ));
            }
            // Guides are entry points themselves; coverage means a direct,
            // authored reference, not transitive reachability or a shared tag.
            if document.kind != DocumentKind::Guide && !covered.contains(&document.id) {
                findings.push(AuthoringFinding::observation(
                    document,
                    AuthoringIssue::NotInGuide,
                ));
            }
        }
        findings.sort_by(|left, right| {
            left.location
                .document
                .0
                .cmp(&right.location.document.0)
                .then_with(|| {
                    left.location
                        .anchor
                        .as_ref()
                        .map(|id| &id.0)
                        .cmp(&right.location.anchor.as_ref().map(|id| &id.0))
                })
        });
        Ok(Self {
            published_documents: published.len(),
            excluded_drafts: documents.len() - published.len(),
            guides: guide_ids.len(),
            findings,
        })
    }

    pub fn has_errors(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == Severity::Error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use berlin_document::{Block, DocumentLink, Inline, Metadata, Provenance, SourceFormat};

    fn document(id: &str, targets: &[&str]) -> Document {
        Document {
            id: ContentId(id.into()),
            kind: DocumentKind::Article,
            metadata: Metadata::default(),
            blocks: targets
                .iter()
                .enumerate()
                .map(|(index, target)| Block::Paragraph {
                    content: vec![
                        Inline::Text {
                            value: "See ".into(),
                        },
                        Inline::DocumentLink(DocumentLink {
                            target: ContentId((*target).into()),
                            fragment: None,
                            anchor: ComponentId(format!("bln-ref-{}", index + 1)),
                            title: None,
                            content: vec![Inline::Text {
                                value: format!("the {target} example"),
                            }],
                        }),
                    ],
                })
                .collect(),
            relations: Vec::new(),
            provenance: Provenance {
                source: format!("file:///{id}.md"),
                source_format: SourceFormat::Markdown,
                source_hash: "0".repeat(64),
            },
        }
    }

    #[test]
    fn collects_every_unresolved_occurrence_with_its_actual_source_passage() {
        let documents =
            DocumentCollection::new(vec![document("note", &["absent", "also-absent", "absent"])]);
        let report = AuthoringReport::analyze(&documents).unwrap();
        let errors: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.severity == Severity::Error)
            .collect();
        assert_eq!(errors.len(), 3);
        assert!(report.has_errors());
        for (index, finding) in errors.iter().enumerate() {
            assert_eq!(finding.location.source, "file:///note.md");
            assert_eq!(finding.location.document.0, "note");
            assert_eq!(
                finding.location.anchor.as_ref().unwrap().0,
                format!("bln-ref-{}", index + 1)
            );
            assert!(
                finding
                    .location
                    .excerpt
                    .as_ref()
                    .unwrap()
                    .starts_with("See the ")
            );
        }
        // Ordinary assembly retains its fail-fast contract and original error.
        assert!(
            matches!(ReferenceIndex::new(&documents), Err(CollectionError::UnresolvedReference { target, .. }) if target.0 == "absent")
        );
    }

    #[test]
    fn separates_direct_guide_coverage_from_connections_and_shared_tags() {
        let mut guide = document("guide", &["a"]);
        guide.kind = DocumentKind::Guide;
        let mut unrelated = document("unrelated", &["unrelated"]);
        guide.metadata.tags = vec!["shared".into()];
        unrelated.metadata.tags = guide.metadata.tags.clone();
        let documents = DocumentCollection::new(vec![
            guide,
            document("a", &["b"]),
            document("b", &[]),
            unrelated,
        ]);
        let report = AuthoringReport::analyze(&documents).unwrap();
        assert!(!report.has_errors());
        assert_eq!(report.guides, 1);
        let issues: Vec<_> = report
            .findings
            .iter()
            .map(|finding| (finding.location.document.0.as_str(), &finding.issue))
            .collect();
        assert_eq!(
            issues,
            vec![
                ("b", &AuthoringIssue::NotInGuide),
                ("unrelated", &AuthoringIssue::DisconnectedNote),
                ("unrelated", &AuthoringIssue::NotInGuide),
            ]
        );
        let reversed =
            DocumentCollection::new(documents.into_vec().into_iter().rev().collect::<Vec<_>>());
        assert_eq!(AuthoringReport::analyze(&reversed).unwrap(), report);
    }

    #[test]
    fn draft_links_neither_connect_cover_nor_leak_their_passages() {
        let mut draft = document("private", &["a", "missing"]);
        draft.kind = DocumentKind::Guide;
        draft.metadata.draft = true;
        draft.metadata.title = Some("Secret title".into());
        let documents =
            DocumentCollection::new(vec![draft, document("a", &[]), document("b", &["private"])]);
        let report = AuthoringReport::analyze(&documents).unwrap();
        assert_eq!(report.published_documents, 2);
        assert_eq!(report.excluded_drafts, 1);
        assert_eq!(report.guides, 0);
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|finding| finding.severity == Severity::Error)
                .count(),
            1
        );
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.location.document.0 != "private")
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.location.document.0 == "a"
                    && finding.issue == AuthoringIssue::DisconnectedNote)
        );
    }

    #[test]
    fn empty_and_guide_only_collections_do_not_require_a_guide_to_cover_itself() {
        let empty = AuthoringReport::analyze(&DocumentCollection::default()).unwrap();
        assert_eq!(empty.published_documents, 0);
        assert!(empty.findings.is_empty());
        let mut guide = document("guide", &[]);
        guide.kind = DocumentKind::Guide;
        let report = AuthoringReport::analyze(&DocumentCollection::new(vec![guide])).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].issue, AuthoringIssue::DisconnectedNote);
        assert!(!report.has_errors());
    }
}
