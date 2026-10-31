use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use berlin_content::DocumentCollection;
use berlin_document::{Document, DocumentKind, PublicationDate};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    Code, ContentRef, Diagnostic, ExcerptRef, PublicationLimits, Result, Stage, fingerprint,
};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PublicationOptions {
    pub source_site_url: Option<Url>,
    pub limits: PublicationLimits,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkOrder {
    #[default]
    PublishedDesc,
    PublishedAsc,
    TitleAsc,
    Curated,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkQuery {
    pub ids: Option<Vec<String>>,
    pub kinds: Vec<String>,
    pub tags_any: Vec<String>,
    pub tags_all: Vec<String>,
    pub published_from: Option<PublicationDate>,
    pub published_to: Option<PublicationDate>,
    pub order: WorkOrder,
    pub limit: Option<usize>,
    pub allow_empty: bool,
}

#[derive(Clone, Debug)]
pub struct CatalogEntry {
    document: Arc<Document>,
    pub source_sha256: String,
    pub document_digest: String,
    pub original_url: Option<Url>,
    pub anchors: BTreeSet<String>,
}
impl CatalogEntry {
    pub fn document(&self) -> &Document {
        &self.document
    }
}

#[derive(Debug)]
struct CatalogSnapshot {
    entries: BTreeMap<String, CatalogEntry>,
    drafts: BTreeSet<String>,
    options: PublicationOptions,
    input_digest: String,
}

/// One index build and one owned document per source. Clones share the snapshot;
/// cards and repeated placements retain references, never copied source bodies.
#[derive(Clone, Debug)]
pub struct PublicationCatalog {
    snapshot: Arc<CatalogSnapshot>,
}

impl PublicationCatalog {
    pub fn new(documents: DocumentCollection, options: PublicationOptions) -> Result<Self> {
        Self::with_original_urls(documents, options, BTreeMap::new())
    }

    pub fn with_original_urls(
        documents: DocumentCollection,
        options: PublicationOptions,
        mappings: BTreeMap<String, Url>,
    ) -> Result<Self> {
        options.limits.validate()?;
        documents
            .validate()
            .map_err(|e| Diagnostic::new(Code::InvalidSource, Stage::Resolution, e.to_string()))?;
        let input_digest = fingerprint(
            "catalog-input/v1",
            &documents
                .as_slice()
                .iter()
                .map(|doc| {
                    (
                        &doc.id,
                        &doc.kind,
                        &doc.metadata,
                        &doc.blocks,
                        &doc.relations,
                        &doc.provenance.source_hash,
                    )
                })
                .collect::<Vec<_>>(),
        )?;
        let base = options
            .source_site_url
            .as_ref()
            .map(mount_url)
            .transpose()?;
        let mut entries = BTreeMap::new();
        let mut drafts = BTreeSet::new();
        for document in documents.into_vec() {
            if document.metadata.draft {
                drafts.insert(document.id.0);
                continue;
            }
            let original_url = if let Some(url) = mappings.get(&document.id.0) {
                validate_public_url(url)?;
                Some(url.clone())
            } else if let Some(base) = &base {
                let route = berlin_content::document_route(&document).map_err(|e| {
                    Diagnostic::new(Code::InvalidSource, Stage::Resolution, e.to_string())
                        .source(&document.id.0, None)
                })?;
                Some(base.join(&route).map_err(|e| {
                    Diagnostic::new(Code::UnsafeUrl, Stage::Resolution, e.to_string())
                })?)
            } else {
                None
            };
            // Post-mapping semantics include metadata but exclude local source
            // path/format. Source bytes are pinned separately by source_sha256.
            let document_digest = fingerprint(
                "document/v1",
                &(
                    &document.kind,
                    &document.metadata,
                    &document.blocks,
                    &document.relations,
                ),
            )?;
            let anchors = document
                .anchor_ids()
                .map_err(|e| {
                    Diagnostic::new(Code::InvalidSource, Stage::Resolution, e.to_string())
                        .source(&document.id.0, None)
                })?
                .into_iter()
                .collect();
            entries.insert(
                document.id.0.clone(),
                CatalogEntry {
                    source_sha256: document.provenance.source_hash.clone(),
                    document_digest,
                    original_url,
                    anchors,
                    document: Arc::new(document),
                },
            );
        }
        if let Some(id) = mappings.keys().find(|id| !entries.contains_key(*id)) {
            return Err(Diagnostic::new(
                Code::InvalidSource,
                Stage::Resolution,
                "original URL mapping must name an eligible source",
            )
            .source(id, None));
        }
        Ok(Self {
            snapshot: Arc::new(CatalogSnapshot {
                entries,
                drafts,
                options,
                input_digest,
            }),
        })
    }

    pub fn limits(&self) -> &PublicationLimits {
        &self.snapshot.options.limits
    }
    pub fn options(&self) -> &PublicationOptions {
        &self.snapshot.options
    }
    pub fn input_digest(&self) -> &str {
        &self.snapshot.input_digest
    }
    pub fn len(&self) -> usize {
        self.snapshot.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.snapshot.entries.is_empty()
    }

    pub fn entry(&self, id: &str) -> Result<&CatalogEntry> {
        self.snapshot.entries.get(id).ok_or_else(|| {
            Diagnostic::new(
                if self.snapshot.drafts.contains(id) {
                    Code::DraftSelection
                } else {
                    Code::InvalidSource
                },
                Stage::Resolution,
                "source does not exist or is not eligible; use a non-draft stable content ID",
            )
            .source(id, None)
        })
    }
    pub fn work_ref(&self, id: &str) -> Result<ContentRef> {
        self.entry(id)?;
        Ok(ContentRef::new(id))
    }
    pub fn pinned_work_ref(&self, reference: ContentRef) -> Result<ContentRef> {
        self.resolve(&reference)?;
        Ok(reference)
    }
    pub fn section_ref(&self, id: &str, section: &str) -> Result<ExcerptRef> {
        crate::model::identifier(section)?;
        Ok(ExcerptRef {
            source: self.work_ref(id)?,
            section: section.into(),
        })
    }
    pub fn resolve(&self, reference: &ContentRef) -> Result<&CatalogEntry> {
        let entry = self.entry(&reference.id.0)?;
        for (name, expected, actual) in [
            (
                "source SHA-256",
                &reference.expected_source_hash,
                &entry.source_sha256,
            ),
            (
                "mapped document digest",
                &reference.expected_document_digest,
                &entry.document_digest,
            ),
        ] {
            if let Some(expected) = expected
                && expected != actual
            {
                return Err(Diagnostic::new(
                    Code::SourceDigestMismatch,
                    Stage::Resolution,
                    format!("{name}: expected {expected}, observed {actual}"),
                )
                .source(&reference.id.0, None));
            }
        }
        Ok(entry)
    }

    pub fn select(&self, query: &WorkQuery) -> Result<Vec<ContentRef>> {
        if query.limit == Some(0) || query.limit.is_some_and(|n| n > self.limits().query_sources) {
            return Err(Diagnostic::new(
                Code::ResourceLimit,
                Stage::Resolution,
                "query limit must be positive and within query_sources budget",
            ));
        }
        if let (Some(from), Some(to)) = (&query.published_from, &query.published_to)
            && from > to
        {
            return Err(Diagnostic::new(
                Code::InvalidProperty,
                Stage::Resolution,
                "published_from is after published_to",
            ));
        }
        let kinds = query
            .kinds
            .iter()
            .map(|name| parse_kind(name))
            .collect::<Result<Vec<_>>>()?;
        let mut candidates = if let Some(ids) = &query.ids {
            let mut seen = BTreeSet::new();
            ids.iter()
                .map(|id| {
                    if !seen.insert(id) {
                        return Err(Diagnostic::new(
                            Code::InvalidSource,
                            Stage::Resolution,
                            "duplicate explicit source in curated list",
                        )
                        .source(id, None));
                    }
                    self.entry(id)
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            if query.order == WorkOrder::Curated {
                return Err(Diagnostic::new(
                    Code::InvalidProperty,
                    Stage::Resolution,
                    "curated order requires explicit ids",
                ));
            }
            self.snapshot.entries.values().collect()
        };
        candidates.retain(|entry| {
            let d = entry.document();
            (kinds.is_empty() || kinds.contains(&d.kind))
                && (query.tags_any.is_empty()
                    || query
                        .tags_any
                        .iter()
                        .any(|tag| d.metadata.tags.contains(tag)))
                && query
                    .tags_all
                    .iter()
                    .all(|tag| d.metadata.tags.contains(tag))
                && (query.published_from.is_none() && query.published_to.is_none()
                    || d.metadata.published.as_ref().is_some_and(|date| {
                        query
                            .published_from
                            .as_ref()
                            .is_none_or(|from| date >= from)
                            && query.published_to.as_ref().is_none_or(|to| date <= to)
                    }))
        });
        if query.order != WorkOrder::Curated {
            candidates.sort_by(|a, b| {
                let a = a.document();
                let b = b.document();
                let compare = match query.order {
                    WorkOrder::TitleAsc => a.metadata.title.cmp(&b.metadata.title),
                    WorkOrder::PublishedAsc | WorkOrder::PublishedDesc => {
                        match (&a.metadata.published, &b.metadata.published) {
                            (Some(a), Some(b)) => {
                                if query.order == WorkOrder::PublishedAsc {
                                    a.cmp(b)
                                } else {
                                    b.cmp(a)
                                }
                            }
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, None) => std::cmp::Ordering::Equal,
                        }
                    }
                    WorkOrder::Curated => std::cmp::Ordering::Equal,
                };
                compare.then_with(|| a.id.0.cmp(&b.id.0))
            });
        }
        if let Some(limit) = query.limit {
            candidates.truncate(limit);
        }
        if candidates.len() > self.limits().query_sources {
            return Err(Diagnostic::new(
                Code::ResourceLimit,
                Stage::Resolution,
                "query selection exceeds query_sources; use an explicit limit",
            ));
        }
        if candidates.is_empty() && !query.allow_empty {
            return Err(Diagnostic::new(
                Code::InvalidSource,
                Stage::Resolution,
                "query matched no eligible work; set allow_empty explicitly if intended",
            ));
        }
        Ok(candidates
            .into_iter()
            .map(|entry| ContentRef::new(&entry.document().id.0))
            .collect())
    }
}

fn parse_kind(name: &str) -> Result<DocumentKind> {
    Ok(match name {
        "article" => DocumentKind::Article,
        "guide" => DocumentKind::Guide,
        "note" => DocumentKind::Note,
        "social_post" => DocumentKind::SocialPost,
        custom if custom.starts_with("custom:") && custom.len() > 7 => {
            DocumentKind::Custom(custom[7..].into())
        }
        _ => {
            return Err(Diagnostic::new(
                Code::InvalidProperty,
                Stage::Resolution,
                format!("unknown document kind '{name}'"),
            ));
        }
    })
}

pub fn validate_public_url(url: &Url) -> Result<()> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Diagnostic::new(
            Code::UnsafeUrl,
            Stage::Resolution,
            "public URLs require HTTP(S), a host and no credentials",
        ));
    }
    Ok(())
}

pub fn mount_url(url: &Url) -> Result<Url> {
    validate_public_url(url)?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(Diagnostic::new(
            Code::UnsafeUrl,
            Stage::Resolution,
            "site mount cannot contain a query or fragment",
        ));
    }
    let mut url = url.clone();
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url)
}
