//! One deterministic query evaluator shared by Rust and Rhai.
use crate::{
    Code, ContentRef, Diagnostic, PublicationCatalog, ResourceCapability, ResourceEntry,
    ResourceId, ResourceKind, ResourceOrigin, ResourceRef, Result, Stage, fingerprint, topic_key,
};
use berlin_document::PublicationDate;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceOrder {
    Curated,
    #[default]
    PublishedDesc,
    AddedDesc,
    TitleAsc,
    IdAsc,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResourceQuery {
    pub ids: Option<Vec<ResourceId>>,
    pub origins_any: Option<Vec<ResourceOrigin>>,
    pub kinds_any: Option<Vec<ResourceKind>>,
    pub topics_all: Vec<String>,
    pub topics_any: Option<Vec<String>>,
    pub topics_none: Vec<String>,
    pub author_text: Option<String>,
    pub source_namespace: Option<String>,
    pub capabilities_all: Vec<ResourceCapability>,
    pub published_from: Option<PublicationDate>,
    pub published_to: Option<PublicationDate>,
    pub added_from: Option<PublicationDate>,
    pub added_to: Option<PublicationDate>,
    pub order: ResourceOrder,
    pub limit: Option<usize>,
}
fn invalid(detail: &str) -> Diagnostic {
    Diagnostic::new(Code::InvalidQuery, Stage::Resolution, detail)
}
fn budget(detail: &str) -> Diagnostic {
    Diagnostic::new(Code::ResourceLimit, Stage::Resolution, detail)
}
fn unique<T: Ord + Clone>(values: &[T]) -> Result<()> {
    let mut seen = BTreeSet::new();
    if values.iter().any(|v| !seen.insert(v.clone())) {
        return Err(invalid("duplicate query term"));
    }
    Ok(())
}
impl ResourceQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn normalized(&self, catalog: &PublicationCatalog) -> Result<Self> {
        let limits = &catalog.options().resource_limits;
        if self.limit == Some(0)
            || self
                .limit
                .is_some_and(|n| n > catalog.limits().query_sources)
        {
            return Err(budget(
                "selection limit must be positive and within query_sources",
            ));
        }
        if self
            .ids
            .as_ref()
            .is_some_and(|ids| ids.len() > catalog.limits().query_sources)
        {
            return Err(budget("curated identity count exceeds query_sources"));
        }
        let count = [
            self.origins_any.as_ref().map_or(0, Vec::len),
            self.kinds_any.as_ref().map_or(0, Vec::len),
            self.topics_all.len(),
            self.topics_any.as_ref().map_or(0, Vec::len),
            self.topics_none.len(),
            self.capabilities_all.len(),
        ]
        .into_iter()
        .try_fold(0usize, |sum, n| {
            sum.checked_add(n)
                .ok_or_else(|| budget("query term count overflow"))
        })?;
        if count > limits.query_terms {
            return Err(budget("query term count exceeded"));
        }
        if self.topics_any.as_ref().is_some_and(Vec::is_empty)
            || self.origins_any.as_ref().is_some_and(Vec::is_empty)
            || self.kinds_any.as_ref().is_some_and(Vec::is_empty)
        {
            return Err(invalid("an explicit any filter must not be empty"));
        }
        if self.order == ResourceOrder::Curated && self.ids.is_none() {
            return Err(invalid("curated order requires explicit IDs"));
        }
        for (from, to) in [
            (&self.published_from, &self.published_to),
            (&self.added_from, &self.added_to),
        ] {
            if from.as_ref().zip(to.as_ref()).is_some_and(|(a, b)| a > b) {
                return Err(invalid("query date range is reversed"));
            }
        }
        if let Some(ids) = &self.ids {
            if ids.iter().any(|id| id.leaf().len() > limits.id_bytes) {
                return Err(budget("query resource identity byte limit exceeded"));
            }
            unique(ids)?;
        }
        if let Some(kinds) = &self.kinds_any {
            unique(kinds)?;
        }
        if let Some(origins) = &self.origins_any {
            unique(origins)?;
        }
        unique(&self.capabilities_all)?;
        let mut query = self.clone();
        let normalize = |values: &[String]| -> Result<Vec<String>> {
            let mut keys = values
                .iter()
                .map(|v| topic_key(v))
                .collect::<Result<Vec<_>>>()?;
            unique(&keys)?;
            keys.sort();
            Ok(keys)
        };
        query.topics_all = normalize(&query.topics_all)?;
        query.topics_none = normalize(&query.topics_none)?;
        query.topics_any = query
            .topics_any
            .as_ref()
            .map(|values| normalize(values))
            .transpose()?;
        if let Some(kinds) = &mut query.kinds_any {
            kinds.sort();
        }
        if let Some(origins) = &mut query.origins_any {
            origins.sort();
        }
        query.capabilities_all.sort();
        if let Some(author) = &mut query.author_text {
            if author.trim().is_empty()
                || author.len() > 256
                || author.chars().any(char::is_control)
            {
                return Err(invalid(
                    "author_text must be a bounded nonempty search string",
                ));
            }
            *author = author.trim().to_lowercase();
            if author.len() > 256 {
                return Err(invalid("normalized author search string is too long"));
            }
        }
        if let Some(namespace) = &query.source_namespace {
            if namespace.is_empty()
                || namespace.len() > 128
                || !namespace
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            {
                return Err(invalid("source_namespace must be a declared source name"));
            }
            if !catalog
                .input_sources()
                .iter()
                .any(|source| source.namespace == *namespace)
            {
                return Err(invalid("unknown source namespace"));
            }
        }
        let encoded = crate::canonical_bytes(&query)?;
        if encoded.len() > catalog.limits().property_bytes {
            return Err(budget("query property byte limit exceeded"));
        }
        for text in query
            .topics_all
            .iter()
            .chain(query.topics_any.iter().flatten())
            .chain(query.topics_none.iter())
            .chain(query.author_text.iter())
            .chain(query.source_namespace.iter())
        {
            if text.len() > catalog.limits().string_bytes {
                return Err(budget("query string byte limit exceeded"));
            }
        }
        Ok(query)
    }
}

/// Read-only ordered selection. Source records are shared with the catalog.
#[derive(Clone, Debug)]
pub struct ResourceSelection {
    catalog: PublicationCatalog,
    ids: Arc<[ResourceId]>,
    query: ResourceQuery,
    query_digest: String,
    selection_digest: String,
    examined: usize,
}
impl ResourceSelection {
    pub fn len(&self) -> usize {
        self.ids.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
    pub fn ids(&self) -> &[ResourceId] {
        &self.ids
    }
    pub fn query(&self) -> &ResourceQuery {
        &self.query
    }
    pub fn query_digest(&self) -> &str {
        &self.query_digest
    }
    pub fn selection_digest(&self) -> &str {
        &self.selection_digest
    }
    pub fn examined_candidates(&self) -> usize {
        self.examined
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ResourceEntry> {
        self.ids.iter().map(|id| {
            self.catalog
                .get(id)
                .expect("selection IDs belong to an immutable catalog")
        })
    }
    pub fn get(&self, id: &ResourceId) -> Option<&ResourceEntry> {
        self.ids
            .contains(id)
            .then(|| self.catalog.get(id).expect("immutable selection"))
    }
    pub fn local_documents(&self) -> Result<Vec<ContentRef>> {
        self.iter()
            .map(|entry| {
                entry.handle().try_local_document()?;
                Ok(ContentRef {
                    id: berlin_document::ContentId(entry.descriptor().id.leaf().into()),
                    expected_source_hash: Some(entry.source().sha256.clone()),
                    expected_document_digest: Some(entry.descriptor().revision.clone()),
                })
            })
            .collect()
    }
}
impl IntoIterator for ResourceSelection {
    type Item = ResourceRef;
    type IntoIter = std::vec::IntoIter<ResourceRef>;
    fn into_iter(self) -> Self::IntoIter {
        self.ids
            .iter()
            .cloned()
            .map(ResourceRef::new)
            .collect::<Vec<_>>()
            .into_iter()
    }
}

enum Seed<'a> {
    All,
    Ids(&'a [ResourceId]),
    Sets(Vec<&'a BTreeSet<ResourceId>>),
}
fn consider<'a>(seed: &mut Seed<'a>, size: &mut usize, sets: Vec<&'a BTreeSet<ResourceId>>) {
    let estimate = sets.iter().map(|set| set.len()).sum::<usize>();
    if estimate < *size {
        *size = estimate;
        *seed = Seed::Sets(sets);
    }
}

impl PublicationCatalog {
    pub fn select(&self, query: &ResourceQuery) -> Result<ResourceSelection> {
        let query = query.normalized(self)?;
        // Estimate cardinality using index lengths, not scans. Materialize only
        // the smallest indexed seed; remaining predicates check existing sets.
        let mut size = self.len();
        let mut seed = Seed::All;
        if let Some(ids) = &query.ids {
            for id in ids {
                self.get(id)?;
            }
            seed = Seed::Ids(ids);
        } else {
            if let Some(kinds) = &query.kinds_any {
                consider(
                    &mut seed,
                    &mut size,
                    kinds
                        .iter()
                        .filter_map(|kind| self.kind_index(kind))
                        .collect(),
                );
            }
            if let Some(origins) = &query.origins_any {
                consider(
                    &mut seed,
                    &mut size,
                    origins
                        .iter()
                        .filter_map(|origin| self.origin_index(*origin))
                        .collect(),
                );
            }
            for tag in &query.topics_all {
                consider(
                    &mut seed,
                    &mut size,
                    self.topic_index(tag).into_iter().collect(),
                );
            }
            if let Some(tags) = &query.topics_any {
                consider(
                    &mut seed,
                    &mut size,
                    tags.iter()
                        .filter_map(|tag| self.topic_index(tag))
                        .collect(),
                );
            }
        }
        let candidate_ids: Vec<&ResourceId> = match seed {
            Seed::All => self
                .resources()
                .map(|entry| &entry.descriptor().id)
                .collect(),
            Seed::Ids(ids) => ids.iter().collect(),
            Seed::Sets(sets) => sets
                .into_iter()
                .flatten()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        };
        let mut entries = Vec::new();
        let mut examined = 0usize;
        for id in candidate_ids {
            examined += 1;
            let entry = self.get(id)?;
            let descriptor = entry.descriptor();
            let topic = |key: &String| self.topic_index(key).is_some_and(|set| set.contains(id));
            let dates = |date: Option<&PublicationDate>,
                         from: Option<&PublicationDate>,
                         to: Option<&PublicationDate>| {
                (from.is_none() && to.is_none())
                    || date
                        .is_some_and(|d| from.is_none_or(|f| d >= f) && to.is_none_or(|t| d <= t))
            };
            if query
                .origins_any
                .as_ref()
                .is_none_or(|origins| origins.contains(&descriptor.origin))
                && query
                    .kinds_any
                    .as_ref()
                    .is_none_or(|kinds| kinds.contains(&descriptor.kind))
                && query.topics_all.iter().all(topic)
                && query
                    .topics_any
                    .as_ref()
                    .is_none_or(|tags| tags.iter().any(topic))
                && !query.topics_none.iter().any(topic)
                && query
                    .capabilities_all
                    .iter()
                    .all(|cap| descriptor.capabilities.contains(cap))
                && query
                    .source_namespace
                    .as_ref()
                    .is_none_or(|name| name == &entry.source().namespace)
                && query.author_text.as_ref().is_none_or(|text| {
                    descriptor
                        .authors
                        .iter()
                        .any(|author| author.to_lowercase().contains(text))
                })
                && dates(
                    descriptor.published.as_ref(),
                    query.published_from.as_ref(),
                    query.published_to.as_ref(),
                )
                && dates(
                    descriptor.added.as_ref(),
                    query.added_from.as_ref(),
                    query.added_to.as_ref(),
                )
            {
                entries.push(entry);
                if query.limit.is_none() && entries.len() > self.limits().query_sources {
                    return Err(budget(
                        "query selection exceeds query_sources; supply an explicit limit",
                    ));
                }
            }
        }
        if query.order != ResourceOrder::Curated {
            entries.sort_by(|a, b| {
                let a = a.descriptor();
                let b = b.descriptor();
                let dates = |a: &Option<PublicationDate>, b: &Option<PublicationDate>| match (a, b)
                {
                    (Some(a), Some(b)) => b.cmp(a),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                };
                let order = match query.order {
                    ResourceOrder::PublishedDesc => dates(&a.published, &b.published),
                    ResourceOrder::AddedDesc => dates(&a.added, &b.added),
                    ResourceOrder::TitleAsc => a.title.cmp(&b.title),
                    _ => std::cmp::Ordering::Equal,
                };
                order.then_with(|| a.id.cmp(&b.id))
            });
        }
        if let Some(limit) = query.limit {
            entries.truncate(limit);
        }
        let query_digest = fingerprint("resource-query/v1", &query)?;
        let selection_digest = fingerprint(
            "resource-selection/v1",
            &(
                &query_digest,
                entries
                    .iter()
                    .map(|e| (&e.descriptor().id, &e.descriptor().revision))
                    .collect::<Vec<_>>(),
            ),
        )?;
        let ids = entries
            .iter()
            .map(|e| e.descriptor().id.clone())
            .collect::<Vec<_>>()
            .into();
        Ok(ResourceSelection {
            catalog: self.clone(),
            ids,
            query,
            query_digest,
            selection_digest,
            examined,
        })
    }
}
