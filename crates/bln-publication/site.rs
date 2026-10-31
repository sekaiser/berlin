//! Multipage intentions over one catalog. No filesystem or presentation engine.
use crate::*;
use serde::{Deserialize, Serialize};
use slugify::slugify;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteSpec {
    pub id: PublicationId,
    pub title: String,
    #[serde(default = "local_scope")]
    pub source_scope: ResourceQuery,
    pub entries: Vec<SiteEntrySpec>,
}
fn local_scope() -> ResourceQuery {
    ResourceQuery {
        origins_any: Some(vec![ResourceOrigin::Local]),
        ..Default::default()
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SiteEntrySpec {
    Composed {
        key: String,
        route: String,
        view: PublicationSpec,
    },
    LocalDocuments {
        selection: ResourceQuery,
        prefix: String,
    },
    TagPages {
        selection: ResourceQuery,
    },
    Search {
        selection: ResourceQuery,
    },
    Redirects {
        selection: ResourceQuery,
    },
}
impl SiteSpec {
    pub fn new(id: PublicationId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            source_scope: local_scope(),
            entries: vec![],
        }
    }
    pub fn add_page(
        &mut self,
        key: impl Into<String>,
        route: impl Into<String>,
        view: PublicationSpec,
    ) {
        self.entries.push(SiteEntrySpec::Composed {
            key: key.into(),
            route: route.into(),
            view,
        });
    }
    pub fn add_document_pages(&mut self, selection: ResourceQuery, prefix: impl Into<String>) {
        self.entries.push(SiteEntrySpec::LocalDocuments {
            selection,
            prefix: prefix.into(),
        });
    }
    pub fn add_tag_pages(&mut self, selection: ResourceQuery) {
        self.entries.push(SiteEntrySpec::TagPages { selection });
    }
    pub fn add_search(&mut self, selection: ResourceQuery) {
        self.entries.push(SiteEntrySpec::Search { selection });
    }
    pub fn add_redirects(&mut self, selection: ResourceQuery) {
        self.entries.push(SiteEntrySpec::Redirects { selection });
    }
}
#[derive(Clone, Debug)]
pub enum SiteRoute {
    Composed {
        key: String,
        assembly: Arc<PublicationAssembly>,
    },
    Document {
        source: ResourceRef,
    },
    Tag {
        label: String,
        resources: Vec<ResourceRef>,
    },
    SearchPage,
    SearchIndex {
        resources: Vec<ResourceRef>,
    },
    SearchClient,
    Redirect {
        target: String,
        source: ResourceRef,
    },
}
#[derive(Clone, Debug)]
pub struct SiteAssembly {
    spec: SiteSpec,
    catalog: PublicationCatalog,
    routes: BTreeMap<String, SiteRoute>,
    source_ids: BTreeSet<ResourceId>,
    document_routes: BTreeMap<ResourceId, String>,
    composition_fingerprint: String,
}
fn fail(code: Code, detail: impl Into<String>) -> Diagnostic {
    Diagnostic::new(code, Stage::Resolution, detail)
}
pub fn validate_site_route(route: &str) -> Result<()> {
    if route.is_empty()
        || route.starts_with("static/")
        || route.starts_with("assets/")
        || route.len() > 1024
        || route.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
    {
        return Err(fail(
            Code::UnsafeUrl,
            "site route must be a bounded relative path of ASCII filename segments",
        ));
    }
    Ok(())
}
fn insert(
    routes: &mut BTreeMap<String, SiteRoute>,
    folded: &mut BTreeSet<String>,
    route: String,
    value: SiteRoute,
    limit: usize,
) -> Result<()> {
    validate_site_route(&route)?;
    if routes.len() >= limit {
        return Err(fail(Code::ResourceLimit, "site route count exceeded"));
    }
    if !folded.insert(route.to_ascii_lowercase()) {
        return Err(fail(
            Code::OutputConflict,
            format!("duplicate site route '{route}'"),
        ));
    }
    routes.insert(route, value);
    Ok(())
}
fn document_route(catalog: &PublicationCatalog, id: &ResourceId, prefix: &str) -> Result<String> {
    validate_site_route(prefix)?;
    let entry = catalog.get(id)?;
    let document = entry.handle().try_local_document()?;
    let route = berlin_content::document_route(document)
        .map_err(|e| fail(Code::InvalidSource, e.to_string()))?;
    Ok(format!(
        "{prefix}/{}",
        route
            .strip_prefix("notes/")
            .expect("canonical document route")
    ))
}
fn previous_routes(
    catalog: &PublicationCatalog,
    id: &ResourceId,
    prefix: &str,
) -> Result<Vec<String>> {
    let entry = catalog.get(id)?;
    let doc = entry.handle().try_local_document()?;
    if !doc.metadata.previous_slugs.is_empty() && doc.metadata.slug.is_none() {
        return Err(fail(
            Code::InvalidSource,
            "previous slugs require an explicit current slug",
        ));
    }
    doc.metadata
        .previous_slugs
        .iter()
        .map(|slug| {
            berlin_content::validated_document_slug(doc, slug)
                .map_err(|e| fail(Code::InvalidSource, e.to_string()))?;
            Ok(format!("{prefix}/{slug}.html"))
        })
        .collect()
}
impl SiteAssembly {
    pub fn resolve(
        spec: &SiteSpec,
        catalog: PublicationCatalog,
        registry: &ComponentRegistry,
    ) -> Result<Self> {
        let limits = catalog.limits();
        if spec.id.0.trim().is_empty() || spec.id.0.chars().any(char::is_control) {
            return Err(fail(Code::InvalidIdentifier, "site requires a stable ID"));
        }
        if spec.title.trim().is_empty()
            || spec.title.len() > limits.string_bytes
            || spec.id.0.len() > limits.string_bytes
            || spec.entries.len() > limits.occurrences
        {
            return Err(fail(
                Code::ResourceLimit,
                "site title, identity or entry count exceeded",
            ));
        }
        let query_count = 1usize
            + spec
                .entries
                .iter()
                .filter(|e| !matches!(e, SiteEntrySpec::Composed { .. }))
                .count();
        if query_count
            .checked_mul(catalog.len().max(1))
            .is_none_or(|n| n > catalog.options().resource_limits.query_work)
        {
            return Err(fail(
                Code::ResourceLimit,
                "cumulative site query work exceeded",
            ));
        }
        let scope = catalog.select(&spec.source_scope)?;
        scope.local_documents()?;
        let source_ids = scope.ids().iter().cloned().collect::<BTreeSet<_>>();
        let mut property_bytes = spec.id.0.len().saturating_add(spec.title.len());
        for entry in &spec.entries {
            let bytes = if let SiteEntrySpec::Composed { key, route, view } = entry {
                if key.len() > limits.string_bytes {
                    return Err(fail(
                        Code::ResourceLimit,
                        "site page key byte limit exceeded",
                    ));
                }
                validate_site_route(route)?;
                view.occurrences(limits)?;
                view.root
                    .measure(limits)?
                    .1
                    .saturating_add(key.len())
                    .saturating_add(route.len())
                    .saturating_add(view.metadata.id.0.len())
                    .saturating_add(view.metadata.title.len())
                    .saturating_add(view.metadata.description.as_ref().map_or(0, String::len))
            } else {
                let selection = match entry {
                    SiteEntrySpec::LocalDocuments { selection, prefix } => {
                        validate_site_route(prefix)?;
                        selection
                    }
                    SiteEntrySpec::TagPages { selection }
                    | SiteEntrySpec::Search { selection }
                    | SiteEntrySpec::Redirects { selection } => selection,
                    _ => unreachable!(),
                };
                crate::canonical_bytes(&selection.normalized(&catalog)?)?.len()
            };
            property_bytes = property_bytes
                .checked_add(bytes)
                .ok_or_else(|| fail(Code::ResourceLimit, "site property size overflow"))?;
            if property_bytes > limits.property_bytes {
                return Err(fail(
                    Code::ResourceLimit,
                    "cumulative site property byte limit exceeded",
                ));
            }
        }
        let mut spec = spec.clone();
        spec.source_scope = scope.query().clone();
        let mut routes = BTreeMap::new();
        let mut folded = BTreeSet::new();
        let mut keys = BTreeSet::new();
        let mut document_routes = BTreeMap::new();
        let mut retained = 0usize;
        let mut selected = Vec::new();
        // Resolve all source-page routes first, independent of declaration order.
        let mut generated_references = 0usize;
        for entry in &mut spec.entries {
            match entry {
                SiteEntrySpec::Composed { key, route, view } => {
                    if key.is_empty()
                        || key.len() > limits.string_bytes
                        || !keys.insert(key.clone())
                    {
                        return Err(fail(
                            Code::DuplicateOccurrenceKey,
                            "site page keys must be bounded and unique",
                        ));
                    }
                    if !route.ends_with(".html") {
                        return Err(fail(
                            Code::UnsafeUrl,
                            "composed page route must end in .html",
                        ));
                    }
                    let assembly = PublicationAssembly::resolve(view, catalog.clone(), registry)?;
                    retained = retained
                        .checked_add(assembly.placements().len())
                        .ok_or_else(|| fail(Code::ResourceLimit, "site occurrence overflow"))?;
                    if retained > limits.occurrences {
                        return Err(fail(
                            Code::ResourceLimit,
                            "cumulative site occurrences exceeded",
                        ));
                    }
                    *view = assembly.spec().clone();
                    selected.push((key.clone(), assembly.composition_fingerprint().to_owned()));
                    insert(
                        &mut routes,
                        &mut folded,
                        route.clone(),
                        SiteRoute::Composed {
                            key: key.clone(),
                            assembly: Arc::new(assembly),
                        },
                        limits.occurrences,
                    )?;
                }
                SiteEntrySpec::LocalDocuments { selection, prefix } => {
                    let result = catalog.select(selection)?;
                    result.local_documents()?;
                    *selection = result.query().clone();
                    for id in result.ids() {
                        if !source_ids.contains(id) {
                            return Err(fail(
                                Code::InvalidSource,
                                "document page is outside declared source scope",
                            ));
                        }
                        let route = document_route(&catalog, id, prefix)?;
                        insert(
                            &mut routes,
                            &mut folded,
                            route.clone(),
                            SiteRoute::Document {
                                source: ResourceRef::new(id.clone()),
                            },
                            limits.occurrences,
                        )?;
                        if document_routes.insert(id.clone(), route).is_some() {
                            return Err(fail(
                                Code::OutputConflict,
                                "local document has multiple source page routes",
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        // Links may target any eligible source, including originals not rendered here.
        for id in &source_ids {
            let document = catalog.get(id)?.handle().try_local_document()?;
            for reference in document.references() {
                let target = ResourceId::local(&reference.link.target.0)?;
                let entry = catalog.get(&target).map_err(|_| {
                    fail(
                        Code::UnresolvedReference,
                        "local link target is not an eligible source",
                    )
                })?;
                if !document_routes.contains_key(&target) && entry.original_url().is_none() {
                    return Err(fail(
                        Code::UnresolvedReference,
                        "unrendered local link requires an original URL mapping",
                    ));
                }
            }
        }
        for entry in &mut spec.entries {
            match entry {
                SiteEntrySpec::TagPages { selection } => {
                    let result = catalog.select(selection)?;
                    *selection = result.query().clone();
                    let mut tags = BTreeMap::<String, (String, Vec<ResourceRef>)>::new();
                    for source in result.iter() {
                        for tag in &source.descriptor().topics {
                            generated_references =
                                generated_references.checked_add(1).ok_or_else(|| {
                                    fail(Code::ResourceLimit, "site generated reference overflow")
                                })?;
                            if generated_references > limits.occurrences {
                                return Err(fail(
                                    Code::ResourceLimit,
                                    "site generated reference limit exceeded",
                                ));
                            }
                            let group = tags
                                .entry(topic_key(tag)?)
                                .or_insert_with(|| (tag.clone(), vec![]));
                            group.1.push(source.reference());
                        }
                    }
                    for (_, (label, resources)) in tags {
                        let slug = slugify!(&label);
                        if slug.is_empty() {
                            return Err(fail(Code::UnsafeUrl, "tag cannot produce a route"));
                        }
                        insert(
                            &mut routes,
                            &mut folded,
                            format!("tags/{slug}.html"),
                            SiteRoute::Tag { label, resources },
                            limits.occurrences,
                        )?;
                    }
                }
                SiteEntrySpec::Search { selection } => {
                    let result = catalog.select(selection)?;
                    generated_references = generated_references
                        .checked_add(result.len())
                        .ok_or_else(|| {
                            fail(Code::ResourceLimit, "site generated reference overflow")
                        })?;
                    if generated_references > limits.occurrences {
                        return Err(fail(
                            Code::ResourceLimit,
                            "site generated reference limit exceeded",
                        ));
                    }
                    *selection = result.query().clone();
                    insert(
                        &mut routes,
                        &mut folded,
                        "search.html".into(),
                        SiteRoute::SearchPage,
                        limits.occurrences,
                    )?;
                    insert(
                        &mut routes,
                        &mut folded,
                        "search/index.json".into(),
                        SiteRoute::SearchIndex {
                            resources: result.ids().iter().cloned().map(ResourceRef::new).collect(),
                        },
                        limits.occurrences,
                    )?;
                    insert(
                        &mut routes,
                        &mut folded,
                        "search/client.js".into(),
                        SiteRoute::SearchClient,
                        limits.occurrences,
                    )?;
                }
                SiteEntrySpec::Redirects { selection } => {
                    let result = catalog.select(selection)?;
                    result.local_documents()?;
                    *selection = result.query().clone();
                    for id in result.ids() {
                        let target = document_routes.get(id).ok_or_else(|| {
                            fail(
                                Code::InvalidSource,
                                "redirect target requires a generated local document page",
                            )
                        })?;
                        let (prefix, _) = target.rsplit_once('/').expect("document page prefix");
                        for route in previous_routes(&catalog, id, prefix)? {
                            insert(
                                &mut routes,
                                &mut folded,
                                route,
                                SiteRoute::Redirect {
                                    target: target.clone(),
                                    source: ResourceRef::new(id.clone()),
                                },
                                limits.occurrences,
                            )?;
                        }
                    }
                }
                _ => {}
            }
        }
        if !routes.contains_key("index.html") {
            return Err(fail(
                Code::InvalidProperty,
                "site requires an explicit index.html page",
            ));
        }
        // Bounds apply across repeated generation intentions, not just each query.
        let generated = routes.values().try_fold(0usize, |sum, route| {
            let n = match route {
                SiteRoute::Tag { resources, .. } | SiteRoute::SearchIndex { resources } => {
                    resources.len()
                }
                _ => 1,
            };
            sum.checked_add(n)
                .ok_or_else(|| fail(Code::ResourceLimit, "site retained selection overflow"))
        })?;
        if generated > limits.occurrences {
            return Err(fail(
                Code::ResourceLimit,
                "site retained selection limit exceeded",
            ));
        }
        let mut consumed_ids = BTreeSet::new();
        for route in routes.values() {
            match route {
                SiteRoute::Tag { resources, .. } | SiteRoute::SearchIndex { resources } => {
                    consumed_ids.extend(resources.iter().map(|r| r.id.clone()))
                }
                SiteRoute::Document { source } | SiteRoute::Redirect { source, .. } => {
                    consumed_ids.insert(source.id.clone());
                }
                SiteRoute::Composed { assembly, .. } => {
                    consumed_ids.extend(assembly.sources().iter().map(|r| r.id.clone()))
                }
                _ => {}
            }
        }
        let revisions = consumed_ids
            .iter()
            .map(|id| catalog.get(id).map(|e| (id, &e.descriptor().revision)))
            .collect::<Result<Vec<_>>>()?;
        let composition_fingerprint = fingerprint(
            "resource-site-composition/v1",
            &(&spec, &selected, revisions),
        )?;
        Ok(Self {
            spec,
            catalog,
            routes,
            source_ids,
            document_routes,
            composition_fingerprint,
        })
    }
    pub fn spec(&self) -> &SiteSpec {
        &self.spec
    }
    pub fn catalog(&self) -> &PublicationCatalog {
        &self.catalog
    }
    pub fn routes(&self) -> &BTreeMap<String, SiteRoute> {
        &self.routes
    }
    pub fn source_ids(&self) -> &BTreeSet<ResourceId> {
        &self.source_ids
    }
    pub fn document_routes(&self) -> &BTreeMap<ResourceId, String> {
        &self.document_routes
    }
    pub fn composition_fingerprint(&self) -> &str {
        &self.composition_fingerprint
    }
}

#[derive(Clone, Debug)]
pub enum ComposedArtifact {
    Single(Arc<PublicationAssembly>),
    Site(Arc<SiteAssembly>),
}
impl ComposedArtifact {
    pub fn single(&self) -> Result<&PublicationAssembly> {
        match self {
            Self::Single(value) => Ok(value),
            Self::Site(_) => Err(fail(
                Code::UnsupportedChannelFeature,
                "text output requires one explicit composition, not a multipage site",
            )),
        }
    }
    pub fn catalog(&self) -> &PublicationCatalog {
        match self {
            Self::Single(value) => value.catalog(),
            Self::Site(value) => value.catalog(),
        }
    }
}
