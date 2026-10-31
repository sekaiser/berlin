//! Format-neutral resource metadata. No filesystem, remote I/O or presentation.
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    net::IpAddr,
    str::FromStr,
    sync::Arc,
};

use berlin_document::{DocumentKind, PublicationDate};
use serde::{Deserialize, Serialize};
use url::{Host, Url};

use crate::{Code, Diagnostic, PublicationLimits, Result, Stage, fingerprint};

pub const RESOURCE_SCHEMA_VERSION: u32 = 1;

fn invalid(detail: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Code::InvalidResource, Stage::Resolution, detail)
}
fn limit(detail: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Code::ResourceLimit, Stage::Resolution, detail)
}
fn version_one() -> u32 {
    RESOURCE_SCHEMA_VERSION
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceOrigin {
    Local,
    External,
}

/// Stable identity; the namespace is part of its canonical serialized value.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceId(String);

impl ResourceId {
    pub fn local(value: &str) -> Result<Self> {
        format!("local:{value}").parse()
    }
    pub fn external(value: &str) -> Result<Self> {
        format!("external:{value}").parse()
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn leaf(&self) -> &str {
        self.0.split_once(':').expect("validated ID").1
    }
    pub fn origin(&self) -> ResourceOrigin {
        if self.0.starts_with("local:") {
            ResourceOrigin::Local
        } else {
            ResourceOrigin::External
        }
    }
}
impl FromStr for ResourceId {
    type Err = Diagnostic;
    fn from_str(value: &str) -> Result<Self> {
        let bad = || {
            Diagnostic::new(
                Code::InvalidIdentifier,
                Stage::Resolution,
                "resource ID requires local: or external: namespace and a bounded nonempty identity",
            )
        };
        let (namespace, leaf) = value.split_once(':').ok_or_else(bad)?;
        if !matches!(namespace, "local" | "external")
            || leaf.is_empty()
            || leaf.len() > 256
            || leaf.trim() != leaf
            || leaf.chars().any(char::is_control)
            || (namespace == "external"
                && !leaf
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b':')))
        {
            return Err(bad());
        }
        Ok(Self(value.into()))
    }
}
impl TryFrom<String> for ResourceId {
    type Error = Diagnostic;
    fn try_from(value: String) -> Result<Self> {
        value.parse()
    }
}
impl From<ResourceId> for String {
    fn from(value: ResourceId) -> Self {
        value.0
    }
}
impl fmt::Display for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceKind(String);
impl ResourceKind {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn from_document(kind: &DocumentKind) -> Result<Self> {
        match kind {
            DocumentKind::Article => "article".parse(),
            DocumentKind::Guide => "guide".parse(),
            DocumentKind::Note => "note".parse(),
            DocumentKind::SocialPost => "social_post".parse(),
            DocumentKind::Custom(name) if name.contains(':') => name.parse(),
            DocumentKind::Custom(name) => format!("custom:{name}").parse(),
        }
    }
}
impl FromStr for ResourceKind {
    type Err = Diagnostic;
    fn from_str(value: &str) -> Result<Self> {
        let name = |s: &str| {
            !s.is_empty()
                && s.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-')
                })
        };
        let custom = value
            .split_once(':')
            .is_some_and(|(ns, kind)| name(ns) && name(kind));
        if value.len() > 128
            || !(custom
                || matches!(
                    value,
                    "article"
                        | "guide"
                        | "note"
                        | "repository"
                        | "paper"
                        | "documentation"
                        | "video"
                        | "website"
                        | "social_post"
                ))
        {
            return Err(Diagnostic::new(
                Code::UnknownResourceKind,
                Stage::Resolution,
                "kind must be a registered builtin or a namespace:name extension",
            ));
        }
        Ok(Self(value.into()))
    }
}
impl TryFrom<String> for ResourceKind {
    type Error = Diagnostic;
    fn try_from(value: String) -> Result<Self> {
        value.parse()
    }
}
impl From<ResourceKind> for String {
    fn from(value: ResourceKind) -> Self {
        value.0
    }
}

/// Additional downward-only limits. Existing composition ceilings are unchanged.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResourceLimits {
    pub catalog_items: usize,
    pub descriptor_bytes: usize,
    pub file_bytes: usize,
    pub id_bytes: usize,
    pub url_bytes: usize,
    pub topics_per_resource: usize,
    pub authors_per_resource: usize,
    pub query_terms: usize,
}
impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            catalog_items: 10_000,
            descriptor_bytes: 8 * 1024 * 1024,
            file_bytes: 8 * 1024 * 1024,
            id_bytes: 256,
            url_bytes: 8192,
            topics_per_resource: 64,
            authors_per_resource: 64,
            query_terms: 64,
        }
    }
}
impl ResourceLimits {
    pub fn validate(&self) -> Result<()> {
        let max = Self::default();
        for (actual, ceiling) in [
            (self.catalog_items, max.catalog_items),
            (self.descriptor_bytes, max.descriptor_bytes),
            (self.file_bytes, max.file_bytes),
            (self.id_bytes, max.id_bytes),
            (self.url_bytes, max.url_bytes),
            (self.topics_per_resource, max.topics_per_resource),
            (self.authors_per_resource, max.authors_per_resource),
            (self.query_terms, max.query_terms),
        ] {
            if actual == 0 || actual > ceiling {
                return Err(limit(
                    "resource limits must be positive and cannot exceed supported ceilings",
                ));
            }
        }
        Ok(())
    }
}

/// Validate without DNS or network I/O. Return the parsed URL only for comparison;
/// the authored string remains the navigation target, including query/fragment.
pub fn validate_resource_url(value: &str, maximum_bytes: usize) -> Result<Url> {
    let bad = || {
        Diagnostic::new(
            Code::UnsafeUrl,
            Stage::Resolution,
            "resource URL must be an absolute public HTTP(S) address without credentials or unsafe bytes",
        )
    };
    if value.len() > maximum_bytes {
        return Err(limit("resource URL byte limit exceeded"));
    }
    if value
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '\\' | '<' | '>' | '"'))
    {
        return Err(bad());
    }
    let url = Url::parse(value).map_err(|_| bad())?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || value
            .split_once("://")
            .and_then(|(_, s)| s.split(['/', '?', '#']).next())
            .is_some_and(|s| s.contains('@'))
    {
        return Err(bad());
    }
    fn public(ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(ip) => {
                !(ip.is_private()
                    || ip.is_loopback()
                    || ip.is_link_local()
                    || ip.is_unspecified()
                    || ip.is_broadcast()
                    || ip.is_multicast()
                    || ip.octets()[0] == 0
                    || ip.octets()[0] >= 240
                    || (ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1])))
            }
            IpAddr::V6(ip) => {
                if let Some(v4) = ip.to_ipv4_mapped() {
                    public(IpAddr::V4(v4))
                } else {
                    !(ip.is_loopback()
                        || ip.is_unspecified()
                        || ip.is_unique_local()
                        || ip.is_unicast_link_local()
                        || ip.is_multicast())
                }
            }
        }
    }
    match url.host().ok_or_else(bad)? {
        Host::Ipv4(ip) if !public(IpAddr::V4(ip)) => return Err(bad()),
        Host::Ipv6(ip) if !public(IpAddr::V6(ip)) => return Err(bad()),
        Host::Domain(name) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            if !name.contains('.')
                || name == "localhost"
                || name.ends_with(".localhost")
                || name.ends_with(".local")
                || name.ends_with(".internal")
            {
                return Err(bad());
            }
        }
        _ => {}
    }
    Ok(url)
}

pub fn topic_key(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(invalid(
            "topics must be nonempty, at most 256 bytes and contain no control characters",
        ));
    }
    Ok(if value.is_ascii() {
        value.to_ascii_lowercase()
    } else {
        value.into()
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceCapability {
    Linkable,
    Excerptable,
    HasPreview,
    HasDate,
    HasSemanticBody,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourcePreview {
    pub url: String,
    pub alt: String,
}

/// An external reference is metadata, never a semantic document or remote snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalResource {
    #[serde(default = "version_one")]
    pub schema_version: u32,
    pub id: String,
    pub kind: ResourceKind,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub published: Option<PublicationDate>,
    #[serde(default)]
    pub added: Option<PublicationDate>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub preview: Option<ResourcePreview>,
    #[serde(default)]
    pub license_note: Option<String>,
    #[serde(default)]
    pub attribution: Option<String>,
    #[serde(default)]
    pub metadata_note: Option<String>,
}
impl ExternalResource {
    pub fn normalized(
        mut self,
        limits: &ResourceLimits,
        publication: &PublicationLimits,
    ) -> Result<Self> {
        limits.validate()?;
        publication.validate()?;
        if self.schema_version != RESOURCE_SCHEMA_VERSION {
            return Err(Diagnostic::new(
                Code::UnknownVersion,
                Stage::Resolution,
                "unsupported external resource schema version",
            ));
        }
        ResourceId::external(&self.id)?;
        if self.id.len() > limits.id_bytes {
            return Err(limit("resource identity byte limit exceeded"));
        }
        validate_resource_url(&self.url, limits.url_bytes)?;
        if self.title.trim().is_empty() {
            return Err(invalid("resource title is required"));
        }
        if self.topics.len() > limits.topics_per_resource
            || self.authors.len() > limits.authors_per_resource
        {
            return Err(limit("resource topic or author count exceeded"));
        }
        let mut charged = 0usize;
        let mut charge = |text: &str| -> Result<()> {
            if text.len() > publication.string_bytes {
                return Err(limit("resource string byte limit exceeded"));
            }
            charged = charged
                .checked_add(text.len())
                .ok_or_else(|| limit("resource size overflow"))?;
            if charged > limits.descriptor_bytes {
                return Err(limit("resource descriptor byte limit exceeded"));
            }
            Ok(())
        };
        for text in [&self.id, &self.url, &self.title] {
            charge(text)?;
        }
        charge(self.kind.as_str())?;
        for text in [
            &self.description,
            &self.publisher,
            &self.language,
            &self.license_note,
            &self.attribution,
            &self.metadata_note,
        ]
        .into_iter()
        .flatten()
        {
            charge(text)?;
        }
        for author in &self.authors {
            if author.trim().is_empty() {
                return Err(invalid("resource author must not be empty"));
            }
            charge(author)?;
        }
        let mut topics = BTreeMap::<String, String>::new();
        // Stable spelling selection if several display spellings share one key.
        self.topics.sort();
        for topic in &self.topics {
            charge(topic)?;
            topics
                .entry(topic_key(topic)?)
                .or_insert_with(|| topic.trim().into());
        }
        self.topics = topics.into_values().collect();
        if let Some(preview) = &self.preview {
            validate_resource_url(&preview.url, limits.url_bytes)?;
            if preview.alt.trim().is_empty() {
                return Err(invalid("preview requires meaningful alternative text"));
            }
            charge(&preview.url)?;
            charge(&preview.alt)?;
        }
        Ok(self)
    }
    pub fn resource_id(&self) -> Result<ResourceId> {
        ResourceId::external(&self.id)
    }
    pub fn revision(&self) -> Result<String> {
        fingerprint("resource-record/v1", self)
    }
    pub fn capabilities(&self) -> BTreeSet<ResourceCapability> {
        let mut set = BTreeSet::from([ResourceCapability::Linkable]);
        if self.preview.is_some() {
            set.insert(ResourceCapability::HasPreview);
        }
        if self.published.is_some() {
            set.insert(ResourceCapability::HasDate);
        }
        set
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSource {
    pub namespace: String,
    pub logical_name: String,
    pub sha256: String,
    pub record: usize,
}
impl ResourceSource {
    pub fn validate(&self) -> Result<()> {
        if self.namespace.is_empty()
            || self.namespace.len() > 128
            || !self
                .namespace
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
            || self.logical_name.is_empty()
            || self.logical_name.len() > 1024
            || self.logical_name.starts_with('/')
            || self.logical_name.contains('\\')
            || self
                .logical_name
                .split('/')
                .any(|s| matches!(s, ".." | "." | ""))
            || self.logical_name.chars().any(char::is_control)
            || self.logical_name.contains(':')
            || self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || self.record == 0
        {
            return Err(invalid("invalid logical resource source evidence"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceRef {
    pub id: ResourceId,
    #[serde(default)]
    pub expected_revision: Option<String>,
}
impl ResourceRef {
    pub fn new(id: ResourceId) -> Self {
        Self {
            id,
            expected_revision: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceDescriptor {
    pub schema_version: u32,
    pub id: ResourceId,
    pub origin: ResourceOrigin,
    pub kind: ResourceKind,
    pub title: String,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub topics: Vec<String>,
    pub published: Option<PublicationDate>,
    pub added: Option<PublicationDate>,
    pub capabilities: BTreeSet<ResourceCapability>,
    pub revision: String,
}

#[derive(Clone, Debug)]
pub struct ExternalRecord {
    pub resource: Arc<ExternalResource>,
    pub source: ResourceSource,
    pub revision: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DuplicateResourceTarget {
    pub first: ResourceId,
    pub second: ResourceId,
}

#[derive(Clone, Debug, Default)]
pub struct ExternalResourceCollection {
    records: Vec<ExternalRecord>,
    duplicate_targets: Vec<DuplicateResourceTarget>,
}
impl ExternalResourceCollection {
    pub fn new(
        mut records: Vec<(ExternalResource, ResourceSource)>,
        limits: &ResourceLimits,
        publication: &PublicationLimits,
    ) -> Result<Self> {
        limits.validate()?;
        if records.len() > limits.catalog_items {
            return Err(limit("external resource count exceeded"));
        }
        records.sort_by(|a, b| {
            (&a.0.id, &a.1.logical_name, a.1.record).cmp(&(&b.0.id, &b.1.logical_name, b.1.record))
        });
        let mut result = Self::default();
        let mut ids = BTreeMap::<ResourceId, ResourceSource>::new();
        let mut targets = BTreeMap::<String, ResourceId>::new();
        let mut charged = 0usize;
        for (resource, source) in records {
            source.validate()?;
            let resource = resource.normalized(limits, publication)?;
            let id = resource.resource_id()?;
            if let Some(first) = ids.insert(id.clone(), source.clone()) {
                return Err(Diagnostic::new(
                    Code::DuplicateResource,
                    Stage::Resolution,
                    format!(
                        "duplicate resource in {} record {} and {} record {}",
                        first.logical_name, first.record, source.logical_name, source.record
                    ),
                )
                .source(id.as_str(), None));
            }
            // Charge canonical metadata and evidence before retaining/indexing it.
            let bytes = crate::canonical_bytes(&(&resource, &source))?.len();
            charged = charged
                .checked_add(bytes)
                .ok_or_else(|| limit("resource size overflow"))?;
            if charged > limits.descriptor_bytes {
                return Err(limit("external descriptor collection byte limit exceeded"));
            }
            let target = validate_resource_url(&resource.url, limits.url_bytes)?.to_string();
            if let Some(first) = targets.get(&target) {
                result.duplicate_targets.push(DuplicateResourceTarget {
                    first: first.clone(),
                    second: id.clone(),
                });
            } else {
                targets.insert(target, id);
            }
            let revision = resource.revision()?;
            result.records.push(ExternalRecord {
                resource: Arc::new(resource),
                source,
                revision,
            });
        }
        result
            .records
            .sort_by(|a, b| a.resource.id.cmp(&b.resource.id));
        result
            .duplicate_targets
            .sort_by(|a, b| (&a.first, &a.second).cmp(&(&b.first, &b.second)));
        Ok(result)
    }
    pub fn records(&self) -> &[ExternalRecord] {
        &self.records
    }
    pub fn duplicate_targets(&self) -> &[DuplicateResourceTarget] {
        &self.duplicate_targets
    }
    pub fn len(&self) -> usize {
        self.records.len()
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}
