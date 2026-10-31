use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use berlin_content::DocumentCollection;
use berlin_document::Document;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    Code, ContentRef, Diagnostic, DuplicateResourceTarget, ExcerptRef, ExternalRecord,
    ExternalResourceCollection, PublicationLimits, ResourceCapability, ResourceDescriptor,
    ResourceId, ResourceInputEvidence, ResourceKind, ResourceLimits, ResourceOrigin, ResourceRef,
    ResourceSource, Result, Stage, fingerprint, topic_key,
};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PublicationOptions {
    pub source_site_url: Option<Url>,
    pub limits: PublicationLimits,
    pub resource_limits: ResourceLimits,
}

#[derive(Clone, Debug)]
pub struct CatalogEntry {
    documents: Arc<DocumentCollection>,
    position: usize,
    pub source_sha256: String,
    pub document_digest: String,
    pub original_url: Option<Url>,
    pub anchors: BTreeSet<String>,
}
impl CatalogEntry {
    pub fn document(&self) -> &Document {
        &self.documents.as_slice()[self.position]
    }
}

/// Shared source storage; external metadata never becomes a semantic Document.
#[derive(Clone, Debug)]
pub enum ResourceHandle {
    Local(Arc<CatalogEntry>),
    External(Arc<ExternalRecord>),
}
impl ResourceHandle {
    pub fn try_local_document(&self) -> Result<&Document> {
        match self {
            Self::Local(entry) => Ok(entry.document()),
            Self::External(_) => Err(Diagnostic::new(
                Code::UnsupportedChannelFeature,
                Stage::Capability,
                "external references do not contain excerptable semantic content",
            )),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResourceEntry {
    descriptor: ResourceDescriptor,
    source: ResourceSource,
    handle: ResourceHandle,
}
impl ResourceEntry {
    pub fn descriptor(&self) -> &ResourceDescriptor {
        &self.descriptor
    }
    pub fn source(&self) -> &ResourceSource {
        &self.source
    }
    pub fn handle(&self) -> &ResourceHandle {
        &self.handle
    }
    pub fn original_url(&self) -> Option<&str> {
        match &self.handle {
            ResourceHandle::Local(entry) => entry.original_url.as_ref().map(Url::as_str),
            ResourceHandle::External(entry) => Some(&entry.resource.url),
        }
    }
    pub fn reference(&self) -> ResourceRef {
        ResourceRef::new(self.descriptor.id.clone())
    }
}

#[derive(Debug)]
struct CatalogSnapshot {
    link_validation: std::sync::OnceLock<Result<()>>,
    entries: BTreeMap<ResourceId, ResourceEntry>,
    drafts: BTreeSet<String>,
    options: PublicationOptions,
    input_digest: String,
    original_urls_digest: String,
    kinds: BTreeMap<ResourceKind, BTreeSet<ResourceId>>,
    origins: BTreeMap<ResourceOrigin, BTreeSet<ResourceId>>,
    topics: BTreeMap<String, BTreeSet<ResourceId>>,
    duplicate_targets: Vec<DuplicateResourceTarget>,
    input_sources: Vec<ResourceInputEvidence>,
}

/// One index build and one owned document per source. Clones share the snapshot;
/// cards and repeated placements retain references, never copied source bodies.
#[derive(Clone, Debug)]
pub struct PublicationCatalog {
    snapshot: Arc<CatalogSnapshot>,
}

impl PublicationCatalog {
    pub fn excluded_drafts(&self) -> usize {
        self.snapshot.drafts.len()
    }
    pub fn local_documents(&self) -> impl Iterator<Item = &Document> {
        self.resources().filter_map(|entry| match entry.handle() {
            ResourceHandle::Local(local) => Some(local.document()),
            ResourceHandle::External(_) => None,
        })
    }
    pub fn validate_local_links(&self) -> Result<()> {
        self.snapshot
            .link_validation
            .get_or_init(|| {
                for document in self.local_documents() {
                    for reference in document.references() {
                        let target = ResourceId::local(&reference.link.target.0)?;
                        if !self.snapshot.entries.contains_key(&target) {
                            return Err(Diagnostic::new(
                                Code::UnresolvedReference,
                                Stage::Resolution,
                                "local link target is absent from the eligible catalog",
                            )
                            .source(&document.id.0, Some(&reference.link.anchor.0)));
                        }
                    }
                }
                Ok(())
            })
            .clone()
    }
    pub fn new(documents: DocumentCollection, options: PublicationOptions) -> Result<Self> {
        Self::with_resources(documents, ExternalResourceCollection::default(), options)
    }

    pub fn with_resources(
        documents: DocumentCollection,
        external: ExternalResourceCollection,
        options: PublicationOptions,
    ) -> Result<Self> {
        Self::with_resource_urls(documents, external, options, BTreeMap::new())
    }

    pub fn with_original_urls(
        documents: DocumentCollection,
        options: PublicationOptions,
        mappings: BTreeMap<String, Url>,
    ) -> Result<Self> {
        Self::with_resource_urls(
            documents,
            ExternalResourceCollection::default(),
            options,
            mappings,
        )
    }

    pub fn with_resource_urls(
        documents: DocumentCollection,
        external: ExternalResourceCollection,
        options: PublicationOptions,
        mappings: BTreeMap<String, Url>,
    ) -> Result<Self> {
        Self::with_document_batches(vec![Arc::new(documents)], external, options, mappings)
    }
    /// Runtime batches retain their captured semantic document storage. Catalog
    /// construction and repeated composition never clone authored bodies.
    pub fn with_document_batches(
        batches: Vec<Arc<DocumentCollection>>,
        external: ExternalResourceCollection,
        options: PublicationOptions,
        mappings: BTreeMap<String, Url>,
    ) -> Result<Self> {
        options.limits.validate()?;
        options.resource_limits.validate()?;
        if external.len() > options.resource_limits.catalog_items
            || batches
                .iter()
                .try_fold(0usize, |total, batch| total.checked_add(batch.len()))
                .unwrap_or(usize::MAX)
                > options
                    .resource_limits
                    .catalog_items
                    .saturating_sub(external.len())
        {
            return Err(Diagnostic::new(
                Code::ResourceLimit,
                Stage::Resolution,
                "catalog source count exceeded",
            ));
        }
        let base = options
            .source_site_url
            .as_ref()
            .map(mount_url)
            .transpose()?;
        if base
            .as_ref()
            .is_some_and(|url| url.as_str().len() > options.resource_limits.url_bytes)
        {
            return Err(Diagnostic::new(
                Code::ResourceLimit,
                Stage::Resolution,
                "source URL byte limit exceeded",
            ));
        }
        let mut entries = BTreeMap::new();
        let mut drafts = BTreeSet::new();
        let mut seen = BTreeSet::new();
        let mut descriptor_bytes = 0usize;
        let mut input_sources = BTreeMap::new();
        for input in external.input_sources() {
            if input_sources.len() >= options.resource_limits.catalog_items {
                return Err(Diagnostic::new(
                    Code::ResourceLimit,
                    Stage::Resolution,
                    "catalog input source count exceeded",
                ));
            }
            input.validate()?;
            charge_catalog(input, &options, &mut descriptor_bytes)?;
            input_sources.insert(
                (input.namespace.clone(), input.logical_name.clone()),
                input.clone(),
            );
        }
        for documents in batches {
            for (position, document) in documents.as_slice().iter().enumerate() {
                if document.id.0.len() > options.resource_limits.id_bytes {
                    return Err(Diagnostic::new(
                        Code::ResourceLimit,
                        Stage::Resolution,
                        "local resource identity byte limit exceeded",
                    ));
                }
                if !seen.insert(document.id.0.clone()) {
                    return Err(Diagnostic::new(
                        Code::DuplicateResource,
                        Stage::Resolution,
                        "duplicate local source identity",
                    ));
                }
                if document.metadata.draft {
                    drafts.insert(document.id.0.clone());
                    continue;
                }
                let kind = ResourceKind::from_document(&document.kind)?;
                if document.metadata.tags.len() > options.resource_limits.topics_per_resource
                    || document.metadata.authors.len()
                        > options.resource_limits.authors_per_resource
                {
                    return Err(Diagnostic::new(
                        Code::ResourceLimit,
                        Stage::Resolution,
                        "local resource topic/author count exceeded",
                    ));
                }
                document.validate().map_err(|_| {
                    Diagnostic::new(
                        Code::InvalidSource,
                        Stage::Resolution,
                        "local source failed semantic validation",
                    )
                })?;
                for text in document
                    .metadata
                    .title
                    .iter()
                    .chain(document.metadata.description.iter())
                    .chain(document.metadata.authors.iter())
                    .chain(document.metadata.tags.iter())
                {
                    if text.len() > options.limits.string_bytes {
                        return Err(Diagnostic::new(
                            Code::ResourceLimit,
                            Stage::Resolution,
                            "local descriptor string budget exceeded",
                        ));
                    }
                }
                let resource_id = ResourceId::local(&document.id.0)?;
                if entries.contains_key(&resource_id) {
                    return Err(Diagnostic::new(
                        Code::DuplicateResource,
                        Stage::Resolution,
                        "duplicate local resource identity",
                    ));
                }
                let original_url = if let Some(url) = mappings.get(&document.id.0) {
                    if url.as_str().len() > options.resource_limits.url_bytes {
                        return Err(Diagnostic::new(
                            Code::ResourceLimit,
                            Stage::Resolution,
                            "original URL byte limit exceeded",
                        ));
                    }
                    validate_public_url(url)?;
                    Some(url.clone())
                } else if let Some(base) = &base {
                    let route = berlin_content::document_route(document).map_err(|e| {
                        Diagnostic::new(Code::InvalidSource, Stage::Resolution, e.to_string())
                            .source(&document.id.0, None)
                    })?;
                    Some(base.join(&route).map_err(|e| {
                        Diagnostic::new(Code::UnsafeUrl, Stage::Resolution, e.to_string())
                    })?)
                } else {
                    None
                };
                if let Some(url) = &original_url {
                    if url.as_str().len() > options.resource_limits.url_bytes {
                        return Err(Diagnostic::new(
                            Code::ResourceLimit,
                            Stage::Resolution,
                            "original URL byte limit exceeded",
                        ));
                    }
                    charge_catalog(url.as_str(), &options, &mut descriptor_bytes)?;
                }
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
                let local = Arc::new(CatalogEntry {
                    source_sha256: document.provenance.source_hash.clone(),
                    document_digest: document_digest.clone(),
                    original_url,
                    anchors,
                    documents: documents.clone(),
                    position,
                });
                let document = local.document();
                if document.metadata.tags.len() > options.resource_limits.topics_per_resource
                    || document.metadata.authors.len()
                        > options.resource_limits.authors_per_resource
                {
                    return Err(Diagnostic::new(
                        Code::ResourceLimit,
                        Stage::Resolution,
                        "local resource topic/author count exceeded",
                    ));
                }
                let mut topics = BTreeMap::new();
                let mut display = document.metadata.tags.clone();
                display.sort();
                for tag in display {
                    topics
                        .entry(topic_key(&tag)?)
                        .or_insert_with(|| tag.trim().to_owned());
                }
                let mut capabilities = BTreeSet::from([
                    ResourceCapability::Excerptable,
                    ResourceCapability::HasSemanticBody,
                ]);
                if local.original_url.is_some() {
                    capabilities.insert(ResourceCapability::Linkable);
                }
                if document.metadata.preview.is_some() {
                    capabilities.insert(ResourceCapability::HasPreview);
                }
                if document.metadata.published.is_some() {
                    capabilities.insert(ResourceCapability::HasDate);
                }
                let descriptor = ResourceDescriptor {
                    schema_version: 1,
                    id: resource_id.clone(),
                    origin: ResourceOrigin::Local,
                    kind,
                    title: document.metadata.title.clone().unwrap_or_default(),
                    description: document.metadata.description.clone(),
                    authors: document.metadata.authors.clone(),
                    topics: topics.into_values().collect(),
                    published: document.metadata.published.clone(),
                    added: None,
                    capabilities,
                    revision: document_digest,
                };
                let source = ResourceSource {
                    namespace: "documents".into(),
                    logical_name: format!(
                        "documents/{}",
                        crate::input_digest(document.id.0.as_bytes())
                    ),
                    sha256: local.source_sha256.clone(),
                    record: 1,
                };
                charge_descriptor(&descriptor, &source, &options, &mut descriptor_bytes)?;
                let input = source.input();
                if !input_sources
                    .contains_key(&(input.namespace.clone(), input.logical_name.clone()))
                {
                    if input_sources.len() >= options.resource_limits.catalog_items {
                        return Err(Diagnostic::new(
                            Code::ResourceLimit,
                            Stage::Resolution,
                            "catalog input source count exceeded",
                        ));
                    }
                    charge_catalog(&input, &options, &mut descriptor_bytes)?;
                    input_sources
                        .insert((input.namespace.clone(), input.logical_name.clone()), input);
                }
                entries.insert(
                    resource_id,
                    ResourceEntry {
                        descriptor,
                        source,
                        handle: ResourceHandle::Local(local),
                    },
                );
            }
        }
        if let Some(id) = mappings.keys().find(|id| {
            ResourceId::local(id).is_err() || !entries.contains_key(&ResourceId::local(id).unwrap())
        }) {
            return Err(Diagnostic::new(
                Code::InvalidSource,
                Stage::Resolution,
                "original URL mapping must name an eligible source",
            )
            .source(id, None));
        }
        let duplicate_targets = external.duplicate_targets().to_vec();
        for record in external.records() {
            let resource = &record.resource;
            resource
                .as_ref()
                .clone()
                .normalized(&options.resource_limits, &options.limits)?;
            let id = resource.resource_id()?;
            let descriptor = ResourceDescriptor {
                schema_version: 1,
                id: id.clone(),
                origin: ResourceOrigin::External,
                kind: resource.kind.clone(),
                title: resource.title.clone(),
                description: resource.description.clone(),
                authors: resource.authors.clone(),
                topics: resource.topics.clone(),
                published: resource.published.clone(),
                added: resource.added.clone(),
                capabilities: resource.capabilities(),
                revision: record.revision.clone(),
            };
            charge_descriptor(&descriptor, &record.source, &options, &mut descriptor_bytes)?;
            charge_catalog(resource.as_ref(), &options, &mut descriptor_bytes)?;
            entries.insert(
                id,
                ResourceEntry {
                    descriptor,
                    source: record.source.clone(),
                    handle: ResourceHandle::External(Arc::new(record.clone())),
                },
            );
        }
        let mut kinds = BTreeMap::<ResourceKind, BTreeSet<ResourceId>>::new();
        let mut origins = BTreeMap::<ResourceOrigin, BTreeSet<ResourceId>>::new();
        let mut topics = BTreeMap::<String, BTreeSet<ResourceId>>::new();
        for (id, entry) in &entries {
            kinds
                .entry(entry.descriptor.kind.clone())
                .or_default()
                .insert(id.clone());
            origins
                .entry(entry.descriptor.origin)
                .or_default()
                .insert(id.clone());
            for tag in &entry.descriptor.topics {
                topics
                    .entry(topic_key(tag)?)
                    .or_default()
                    .insert(id.clone());
            }
        }
        let input_sources = input_sources.into_values().collect::<Vec<_>>();
        let input_digest = fingerprint(
            "resource-catalog/v1",
            &(
                entries
                    .values()
                    .map(|entry| (&entry.descriptor, &entry.source))
                    .collect::<Vec<_>>(),
                &input_sources,
            ),
        )?;
        let original_urls_digest = fingerprint(
            "resource-original-urls/v1",
            &entries
                .iter()
                .map(|(id, entry)| (id, entry.original_url()))
                .collect::<BTreeMap<_, _>>(),
        )?;
        Ok(Self {
            snapshot: Arc::new(CatalogSnapshot {
                link_validation: std::sync::OnceLock::new(),
                entries,
                drafts,
                options,
                input_digest,
                original_urls_digest,
                kinds,
                origins,
                topics,
                duplicate_targets,
                input_sources,
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
    pub fn original_urls_digest(&self) -> &str {
        &self.snapshot.original_urls_digest
    }
    /// One immutable index is constructed by each catalog constructor. Cloning
    /// a catalog shares this snapshot rather than indexing the sources again.
    pub fn index_builds(&self) -> usize {
        1
    }
    pub fn len(&self) -> usize {
        self.snapshot.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.snapshot.entries.is_empty()
    }

    pub fn entry(&self, id: &str) -> Result<&CatalogEntry> {
        self.snapshot
            .entries
            .get(&ResourceId::local(id)?)
            .and_then(|entry| match &entry.handle {
                ResourceHandle::Local(local) => Some(local.as_ref()),
                _ => None,
            })
            .ok_or_else(|| {
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
    pub fn get(&self, id: &ResourceId) -> Result<&ResourceEntry> {
        self.snapshot.entries.get(id).ok_or_else(|| {
            Diagnostic::new(
                if id.origin() == ResourceOrigin::Local && self.snapshot.drafts.contains(id.leaf())
                {
                    Code::DraftSelection
                } else {
                    Code::InvalidSource
                },
                Stage::Resolution,
                "resource does not exist or is not publication-eligible",
            )
        })
    }
    pub fn resolve_resource(&self, reference: &ResourceRef) -> Result<&ResourceEntry> {
        let entry = self.get(&reference.id)?;
        if reference
            .expected_revision
            .as_ref()
            .is_some_and(|revision| revision != &entry.descriptor.revision)
        {
            return Err(Diagnostic::new(
                Code::SourceDigestMismatch,
                Stage::Resolution,
                "resource revision differs from its pinned reference",
            ));
        }
        if reference
            .expected_source_hash
            .as_ref()
            .is_some_and(|sha| sha != &entry.source.sha256)
        {
            return Err(Diagnostic::new(
                Code::SourceDigestMismatch,
                Stage::Resolution,
                "resource input hash differs from its pinned reference",
            ));
        }
        Ok(entry)
    }
    pub fn resources(&self) -> impl Iterator<Item = &ResourceEntry> {
        self.snapshot.entries.values()
    }
    pub fn by_kind(&self, kind: &ResourceKind) -> impl Iterator<Item = &ResourceId> {
        self.snapshot.kinds.get(kind).into_iter().flatten()
    }
    pub(crate) fn kind_index(&self, kind: &ResourceKind) -> Option<&BTreeSet<ResourceId>> {
        self.snapshot.kinds.get(kind)
    }
    pub(crate) fn origin_index(&self, origin: ResourceOrigin) -> Option<&BTreeSet<ResourceId>> {
        self.snapshot.origins.get(&origin)
    }
    pub(crate) fn topic_index(&self, topic: &str) -> Option<&BTreeSet<ResourceId>> {
        self.snapshot.topics.get(topic)
    }
    pub fn by_origin(&self, origin: ResourceOrigin) -> impl Iterator<Item = &ResourceId> {
        self.snapshot.origins.get(&origin).into_iter().flatten()
    }
    pub fn by_topic(&self, topic: &str) -> Result<impl Iterator<Item = &ResourceId>> {
        Ok(self
            .snapshot
            .topics
            .get(&topic_key(topic)?)
            .into_iter()
            .flatten())
    }
    pub fn duplicate_targets(&self) -> &[DuplicateResourceTarget] {
        &self.snapshot.duplicate_targets
    }
    pub fn input_sources(&self) -> &[ResourceInputEvidence] {
        &self.snapshot.input_sources
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
            section: Some(section.into()),
        })
    }
    pub fn document_excerpt(&self, id: &str) -> Result<ExcerptRef> {
        Ok(ExcerptRef {
            source: self.work_ref(id)?,
            section: None,
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
}

fn charge_descriptor(
    descriptor: &ResourceDescriptor,
    source: &ResourceSource,
    options: &PublicationOptions,
    total: &mut usize,
) -> Result<()> {
    source.validate()?;
    charge_catalog(&(descriptor, source), options, total)
}
fn charge_catalog(
    value: &(impl Serialize + ?Sized),
    options: &PublicationOptions,
    total: &mut usize,
) -> Result<()> {
    let bytes = crate::canonical_bytes(&value)?.len();
    *total = total.checked_add(bytes).ok_or_else(|| {
        Diagnostic::new(
            Code::ResourceLimit,
            Stage::Resolution,
            "catalog descriptor size overflow",
        )
    })?;
    if *total > options.resource_limits.descriptor_bytes {
        return Err(Diagnostic::new(
            Code::ResourceLimit,
            Stage::Resolution,
            "catalog descriptor byte limit exceeded",
        ));
    }
    Ok(())
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
