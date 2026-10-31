//! Pure execution-time composition. Custom reference values are decoded
//! explicitly rather than relying on serialization of Rhai native types.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use berlin_publication::*;
use rhai::{Dynamic, Engine, EvalAltResult, ImmutableString, NativeCallContext};
type ScriptResult<T> = std::result::Result<T, Box<EvalAltResult>>;

#[derive(Clone)]
pub(super) struct ComposerState(Rc<RefCell<Budget>>);
#[derive(Clone)]
pub(super) struct ScriptPublication(pub Arc<PublicationSpec>);
#[derive(Clone)]
pub(super) struct ScriptSite(pub Arc<SiteSpec>);
#[derive(Clone)]
struct ScriptFragment {
    view: Arc<ScriptNode>,
    nodes: usize,
    bytes: usize,
    depth: usize,
}
struct ScriptNode {
    component: ComponentKey,
    key: Option<String>,
    properties: BTreeMap<String, Property>,
    slots: BTreeMap<String, Vec<ScriptFragment>>,
}
impl ScriptFragment {
    // Called only after the expanded logical tree has passed its budgets.
    fn lower(&self) -> ComponentCall {
        ComponentCall {
            component: self.view.component.clone(),
            key: self.view.key.clone(),
            properties: self.view.properties.clone(),
            slots: self
                .view
                .slots
                .iter()
                .map(|(name, children)| {
                    (
                        name.clone(),
                        ViewFragment(children.iter().map(Self::lower).collect()),
                    )
                })
                .collect(),
        }
    }
}
struct Budget {
    queries: QueryEvidenceTable,
    limits: PublicationLimits,
    nodes: usize,
    bytes: usize,
    edges: usize,
    query_work: usize,
    query_results: usize,
    lowered_bytes: usize,
    lowered_nodes: usize,
    resource_limits: ResourceLimits,
    registry: Arc<ComponentRegistry>,
}
impl ComposerState {
    pub fn new(
        limits: PublicationLimits,
        resource_limits: ResourceLimits,
        registry: Arc<ComponentRegistry>,
    ) -> Self {
        Self(Rc::new(RefCell::new(Budget {
            queries: BTreeMap::new(),
            limits,
            nodes: 0,
            bytes: 0,
            edges: 0,
            query_work: 0,
            query_results: 0,
            lowered_bytes: 0,
            lowered_nodes: 0,
            resource_limits,
            registry,
        })))
    }
    pub fn queries(&self) -> QueryEvidenceTable {
        self.0.borrow().queries.clone()
    }
}
fn failure(error: impl std::fmt::Display) -> Box<EvalAltResult> {
    error.to_string().into()
}
fn state(context: &NativeCallContext<'_>) -> ScriptResult<ComposerState> {
    context
        .tag()
        .and_then(|tag| tag.clone().try_cast::<ComposerState>())
        .ok_or_else(|| {
            "composition values are available only during execution of a publication composer"
                .into()
        })
}

pub(super) fn register(engine: &mut Engine) {
    register_sites(engine);
    engine.register_type_with_name::<PublicationCatalog>("PublicationCatalog");
    engine.register_type_with_name::<ContentRef>("ContentRef");
    engine.register_type_with_name::<ExcerptRef>("ExcerptRef");
    engine.register_type_with_name::<ScriptFragment>("ViewFragment");
    engine.register_type_with_name::<ScriptPublication>("PublicationSpec");
    engine.register_type_with_name::<ResourceRef>("ResourceRef");
    engine.register_type_with_name::<ResourceSelection>("ResourceSelection");
    engine.register_iterator::<ResourceSelection>();
    engine.register_get("id", |reference: &mut ResourceRef| -> ImmutableString {
        reference.id.as_str().into()
    });
    engine.register_get(
        "query_digest",
        |selection: &mut ResourceSelection| -> ImmutableString { selection.query_digest().into() },
    );
    engine.register_get(
        "selection_digest",
        |selection: &mut ResourceSelection| -> ImmutableString {
            selection.selection_digest().into()
        },
    );
    engine.register_fn("len", |selection: &mut ResourceSelection| -> rhai::INT {
        selection.len() as rhai::INT
    });
    engine.register_fn(
        "describe",
        |catalog: PublicationCatalog, reference: ResourceRef| -> ScriptResult<Dynamic> {
            let entry = catalog.resolve_resource(&reference).map_err(failure)?;
            rhai::serde::to_dynamic(entry.descriptor()).map_err(failure)
        },
    );
    engine.register_fn(
        "select",
        |context: NativeCallContext<'_>,
         catalog: PublicationCatalog,
         query: rhai::Map|
         -> ScriptResult<ResourceSelection> {
            preflight_query(&query, &catalog)?;
            let query: ResourceQuery = rhai::serde::from_dynamic(&Dynamic::from(query))?;
            let state = state(&context)?;
            let reservation = query
                .limit
                .unwrap_or(catalog.limits().query_sources)
                .min(catalog.len());
            {
                let mut budget = state.0.borrow_mut();
                let work = budget
                    .query_work
                    .checked_add(catalog.len().max(1))
                    .ok_or_else(|| failure("ResourceLimit: query work overflow"))?;
                let results = budget
                    .query_results
                    .checked_add(reservation)
                    .ok_or_else(|| failure("ResourceLimit: query result count overflow"))?;
                if work > budget.resource_limits.query_work || results > budget.limits.occurrences {
                    return Err(failure("ResourceLimit: cumulative query budget exceeded"));
                }
                budget.query_work = work;
                budget.query_results = results;
            }
            let result = catalog.select(&query).map_err(failure)?;
            let mut budget = state.0.borrow_mut();
            budget.query_results -= reservation - result.len();
            let limits = budget.limits.clone();
            record_selection(&mut budget.queries, &result, &limits).map_err(failure)?;
            Ok(result)
        },
    );
    engine.register_fn(
        "resource_ref",
        |catalog: PublicationCatalog, id: ImmutableString| -> ScriptResult<ResourceRef> {
            let id = id.as_str().parse::<ResourceId>().map_err(failure)?;
            Ok(catalog.get(&id).map_err(failure)?.reference())
        },
    );
    engine.register_fn(
        "pinned_resource_ref",
        |catalog: PublicationCatalog,
         id: ImmutableString,
         pins: rhai::Map|
         -> ScriptResult<ResourceRef> {
            let id = id.as_str().parse::<ResourceId>().map_err(failure)?;
            let mut reference = catalog.get(&id).map_err(failure)?.reference();
            for (name, value) in pins {
                let value = value
                    .try_cast::<ImmutableString>()
                    .ok_or_else(|| failure("pin values must be SHA-256 strings"))?;
                match name.as_str() {
                    "record_revision" => reference.expected_revision = Some(value.into()),
                    "source_sha256" => reference.expected_source_hash = Some(value.into()),
                    _ => return Err(failure(format!("unknown pin field '{name}'"))),
                }
            }
            catalog.resolve_resource(&reference).map_err(failure)?;
            Ok(reference)
        },
    );
    engine.register_fn(
        "section_ref",
        |catalog: PublicationCatalog,
         id: ImmutableString,
         section: ImmutableString|
         -> ScriptResult<ExcerptRef> {
            let id = id.as_str().parse::<ResourceId>().map_err(failure)?;
            catalog
                .get(&id)
                .map_err(failure)?
                .handle()
                .try_local_document()
                .map_err(failure)?;
            catalog.section_ref(id.leaf(), &section).map_err(failure)
        },
    );
    engine.register_fn("view_component", view_component);
    engine.register_fn("publication", publication);
    register_helpers(engine);
}

fn register_helpers(engine: &mut Engine) {
    engine.register_fn(
        "map_cards",
        |context: NativeCallContext<'_>, selection: ResourceSelection| {
            map_selection(&context, selection, "berlin.resource_card@1", "resource")
        },
    );
    engine.register_fn(
        "map_featured",
        |context: NativeCallContext<'_>, selection: ResourceSelection| {
            map_selection(&context, selection, "berlin.featured_work@1", "source")
        },
    );
    engine.register_fn(
        "section",
        |context: NativeCallContext<'_>, title: ImmutableString, children: rhai::Array| {
            make_view(
                &context,
                "berlin.section@1".into(),
                rhai::Map::from_iter([("title".into(), Dynamic::from(title))]),
                rhai::Map::from_iter([("body".into(), Dynamic::from(children))]),
            )
        },
    );
    engine.register_fn(
        "reading_list",
        |context: NativeCallContext<'_>, title: ImmutableString, children: rhai::Array| {
            make_view(
                &context,
                "berlin.reading_list@1".into(),
                rhai::Map::from_iter([("title".into(), Dynamic::from(title))]),
                rhai::Map::from_iter([("items".into(), Dynamic::from(children))]),
            )
        },
    );
    engine.register_fn(
        "publication",
        |context: NativeCallContext<'_>,
         id: ImmutableString,
         mut settings: rhai::Map,
         children: rhai::Array| {
            if settings.contains_key("id") {
                return Err(failure("publication ID must be supplied only once"));
            }
            settings.insert("id".into(), Dynamic::from(id));
            let root = make_view(
                &context,
                "berlin.portfolio@1".into(),
                rhai::Map::new(),
                rhai::Map::from_iter([("body".into(), Dynamic::from(children))]),
            )?;
            make_publication(&context, settings, root)
        },
    );
    engine.register_fn(
        "home_page",
        |context: NativeCallContext<'_>,
         authored: ResourceSelection,
         reading: ResourceSelection|
         -> ScriptResult<ScriptPublication> {
            let mut children = rhai::Array::new();
            if !authored.is_empty() {
                let cards = map_selection(&context, authored, "berlin.featured_work@1", "source")?;
                children.push(Dynamic::from(make_view(
                    &context,
                    "berlin.section@1".into(),
                    rhai::Map::from_iter([("title".into(), Dynamic::from("Notes"))]),
                    rhai::Map::from_iter([("body".into(), Dynamic::from(cards))]),
                )?));
            }
            if !reading.is_empty() {
                let cards = map_selection(&context, reading, "berlin.resource_card@1", "resource")?;
                children.push(Dynamic::from(make_view(
                    &context,
                    "berlin.reading_list@1".into(),
                    rhai::Map::from_iter([("title".into(), Dynamic::from("Reading"))]),
                    rhai::Map::from_iter([("items".into(), Dynamic::from(cards))]),
                )?));
            }
            let root = make_view(
                &context,
                "berlin.portfolio@1".into(),
                rhai::Map::new(),
                rhai::Map::from_iter([("body".into(), Dynamic::from(children))]),
            )?;
            make_publication(
                &context,
                rhai::Map::from_iter([
                    ("id".into(), Dynamic::from("home")),
                    ("title".into(), Dynamic::from("Notebook")),
                ]),
                root,
            )
        },
    );
}
fn map_selection(
    context: &NativeCallContext<'_>,
    selection: ResourceSelection,
    key: &str,
    property: &str,
) -> ScriptResult<rhai::Array> {
    let mut result = Vec::with_capacity(selection.len());
    for reference in selection {
        result.push(Dynamic::from(make_view(
            context,
            key.into(),
            rhai::Map::from_iter([(property.into(), Dynamic::from(reference))]),
            rhai::Map::new(),
        )?));
    }
    Ok(result)
}

fn register_sites(engine: &mut Engine) {
    engine.register_type_with_name::<ScriptSite>("SiteSpec");
    engine.register_fn(
        "site",
        |context: NativeCallContext<'_>,
         id: ImmutableString,
         settings: rhai::Map|
         -> ScriptResult<ScriptSite> {
            let state = state(&context)?;
            if settings.keys().any(|key| key != "title") {
                return Err(failure("unknown site setting"));
            }
            let title = settings
                .get("title")
                .and_then(|v| v.clone().try_cast::<ImmutableString>())
                .ok_or_else(|| failure("site requires a title"))?;
            let limits = state.0.borrow().limits.clone();
            if id.trim().is_empty()
                || id.chars().any(char::is_control)
                || title.trim().is_empty()
                || id.len() > limits.string_bytes
                || title.len() > limits.string_bytes
            {
                return Err(failure("invalid or oversized site identity/title"));
            }
            {
                let mut budget = state.0.borrow_mut();
                if id.len().saturating_add(title.len())
                    > budget.limits.property_bytes.saturating_sub(budget.bytes)
                    || budget.edges >= budget.limits.occurrences
                {
                    return Err(failure(
                        "ResourceLimit: cumulative site value budget exceeded",
                    ));
                }
                budget.bytes += id.len() + title.len();
                budget.edges += 1;
            }
            Ok(ScriptSite(Arc::new(SiteSpec::new(
                PublicationId(id.into()),
                title.to_string(),
            ))))
        },
    );
    engine.register_fn(
        "add_page",
        |context: NativeCallContext<'_>,
         site: &mut ScriptSite,
         key: ImmutableString,
         route: ImmutableString,
         view: ScriptPublication|
         -> ScriptResult<()> {
            validate_site_route(&route).map_err(failure)?;
            let state = state(&context)?;
            let limits = state.0.borrow().limits.clone();
            let (nodes, bytes) = view.0.root.measure(&limits).map_err(failure)?;
            charge_site(
                &state,
                &site.0,
                nodes,
                bytes
                    .checked_add(key.len())
                    .and_then(|v| v.checked_add(route.len()))
                    .ok_or_else(|| failure("ResourceLimit: site size overflow"))?,
            )?;
            let site = site_mut(site, &state)?;
            site.add_page(key.to_string(), route.to_string(), view.0.as_ref().clone());
            Ok(())
        },
    );
    engine.register_fn(
        "add_document_pages",
        |context: NativeCallContext<'_>,
         site: &mut ScriptSite,
         selection: ResourceSelection,
         prefix: ImmutableString|
         -> ScriptResult<()> {
            selection.local_documents().map_err(failure)?;
            validate_site_route(&prefix).map_err(failure)?;
            let state = state(&context)?;
            charge_site(&state, &site.0, selection.len(), prefix.len())?;
            site_mut(site, &state)?
                .add_document_pages(selection.query().clone(), prefix.to_string());
            Ok(())
        },
    );
    engine.register_fn(
        "add_tag_pages",
        |context: NativeCallContext<'_>,
         site: &mut ScriptSite,
         selection: ResourceSelection|
         -> ScriptResult<()> {
            let state = state(&context)?;
            charge_site(&state, &site.0, selection.len(), 0)?;
            site_mut(site, &state)?.add_tag_pages(selection.query().clone());
            Ok(())
        },
    );
    engine.register_fn(
        "add_search",
        |context: NativeCallContext<'_>,
         site: &mut ScriptSite,
         selection: ResourceSelection|
         -> ScriptResult<()> {
            let state = state(&context)?;
            charge_site(&state, &site.0, selection.len(), 0)?;
            site_mut(site, &state)?.add_search(selection.query().clone());
            Ok(())
        },
    );
    engine.register_fn(
        "add_redirects",
        |context: NativeCallContext<'_>,
         site: &mut ScriptSite,
         selection: ResourceSelection|
         -> ScriptResult<()> {
            selection.local_documents().map_err(failure)?;
            let state = state(&context)?;
            charge_site(&state, &site.0, selection.len(), 0)?;
            site_mut(site, &state)?.add_redirects(selection.query().clone());
            Ok(())
        },
    );
}
fn site_mut<'a>(site: &'a mut ScriptSite, state: &ComposerState) -> ScriptResult<&'a mut SiteSpec> {
    if Arc::strong_count(&site.0) > 1 {
        let bytes = canonical_bytes(site.0.as_ref()).map_err(failure)?.len();
        charge_site(state, &site.0, site.0.entries.len(), bytes)?;
    }
    Ok(Arc::make_mut(&mut site.0))
}
fn charge_site(
    state: &ComposerState,
    site: &SiteSpec,
    nodes: usize,
    bytes: usize,
) -> ScriptResult<()> {
    let mut budget = state.0.borrow_mut();
    if site.entries.len() >= budget.limits.occurrences
        || nodes > budget.limits.occurrences.saturating_sub(budget.edges)
        || bytes > budget.limits.property_bytes.saturating_sub(budget.bytes)
    {
        return Err(failure(
            "ResourceLimit: cumulative site composition limit exceeded",
        ));
    }
    budget.edges += nodes;
    budget.bytes += bytes;
    Ok(())
}

// Check borrowed Rhai arrays before serde copies their strings into Rust Vecs.
fn preflight_query(query: &rhai::Map, catalog: &PublicationCatalog) -> ScriptResult<()> {
    let mut terms = 0usize;
    let resources = &catalog.options().resource_limits;
    for (key, value) in query {
        let array_limit = match key.as_str() {
            "ids" => Some(catalog.limits().query_sources),
            "origins_any" | "kinds_any" | "topics_all" | "topics_any" | "topics_none"
            | "capabilities_all" => Some(resources.query_terms),
            "author_text" | "source_namespace" | "order" | "published_from" | "published_to"
            | "added_from" | "added_to" | "limit" => None,
            _ => return Err(failure("InvalidQuery: unknown query field")),
        };
        if value.is_unit() {
            continue;
        }
        if let Some(maximum) = array_limit {
            let values = value
                .read_lock::<rhai::Array>()
                .ok_or_else(|| failure("InvalidQuery: expected query array"))?;
            if values.len() > maximum {
                return Err(failure("ResourceLimit: query array count exceeded"));
            }
            if key != "ids" {
                terms = terms
                    .checked_add(values.len())
                    .ok_or_else(|| failure("ResourceLimit: query term overflow"))?;
            }
            if terms > resources.query_terms {
                return Err(failure("ResourceLimit: query term count exceeded"));
            }
            for item in values.iter() {
                let text = item
                    .read_lock::<ImmutableString>()
                    .ok_or_else(|| failure("InvalidQuery: query arrays contain strings only"))?;
                if text.len() > 267 {
                    return Err(failure("ResourceLimit: query term string exceeded"));
                }
            }
        } else if key != "limit" {
            let text = value
                .read_lock::<ImmutableString>()
                .ok_or_else(|| failure("InvalidQuery: expected query string"))?;
            if text.len() > 256 {
                return Err(failure("ResourceLimit: query string exceeded"));
            }
        }
    }
    Ok(())
}

fn decode(value: Dynamic, depth: usize) -> ScriptResult<Property> {
    if depth > 32 {
        return Err(failure("ResourceLimit: native property nesting exceeded"));
    }
    if value.is::<ContentRef>() {
        return Ok(Property::Source(value.cast()));
    }
    if value.is::<ResourceRef>() {
        return Ok(Property::Resource(value.cast()));
    }
    if value.is::<ExcerptRef>() {
        return Ok(Property::Excerpt(value.cast()));
    }
    if value.is::<ImmutableString>() {
        return Ok(Property::Text(value.cast::<ImmutableString>().into()));
    }
    if value.is::<bool>() {
        return Ok(Property::Boolean(value.cast()));
    }
    if value.is::<rhai::INT>() {
        return Ok(Property::Integer(value.cast()));
    }
    if value.is_unit() {
        return Ok(Property::None);
    }
    if value.is::<rhai::Array>() {
        return Ok(Property::List(
            value
                .cast::<rhai::Array>()
                .into_iter()
                .map(|value| decode(value, depth + 1))
                .collect::<ScriptResult<_>>()?,
        ));
    }
    Err(failure(
        "InvalidProperty: supported values are strings, booleans, integers, native references, arrays and ()",
    ))
}
fn view_component(
    context: NativeCallContext<'_>,
    key: ImmutableString,
    props: rhai::Map,
    slots: rhai::Map,
) -> ScriptResult<ScriptFragment> {
    make_view(&context, key, props, slots)
}
fn make_view(
    context: &NativeCallContext<'_>,
    key: ImmutableString,
    props: rhai::Map,
    slots: rhai::Map,
) -> ScriptResult<ScriptFragment> {
    let state = state(context)?;
    let mut call = ComponentCall::new(key.as_str().parse().map_err(failure)?);
    for (name, value) in props {
        if name == "key" {
            let key = value
                .try_cast::<ImmutableString>()
                .ok_or_else(|| failure("occurrence key must be a string"))?;
            call.key = Some(key.into());
        } else {
            call.properties.insert(name.into(), decode(value, 1)?);
        }
    }
    let limits = state.0.borrow().limits.clone();
    let mut expanded_nodes = 1usize;
    let mut expanded_bytes = 0usize;
    let mut depth = 1usize;
    let mut edges = 0usize;
    let mut shared_slots = BTreeMap::<String, Vec<ScriptFragment>>::new();
    for (name, value) in slots {
        let array = value
            .try_cast::<rhai::Array>()
            .ok_or_else(|| failure("slot values must be arrays of fragments"))?;
        let mut children = Vec::new();
        for value in array {
            let fragment = value
                .try_cast::<ScriptFragment>()
                .ok_or_else(|| failure("slots require ViewFragment values"))?;
            expanded_nodes = expanded_nodes.saturating_add(fragment.nodes);
            expanded_bytes = expanded_bytes.saturating_add(fragment.bytes);
            depth = depth.max(fragment.depth.saturating_add(1));
            if expanded_nodes > limits.occurrences
                || expanded_bytes > limits.property_bytes
                || depth > limits.depth
            {
                return Err(failure(
                    "ResourceLimit: expanded native slot budget exceeded",
                ));
            }
            edges = edges.saturating_add(1);
            children.push(fragment);
        }
        shared_slots.insert(name.into(), children);
    }
    let counts = shared_slots
        .iter()
        .map(|(name, children)| (name.clone(), children.len()))
        .collect();
    state
        .0
        .borrow()
        .registry
        .normalize_parts(&call.component, &mut call.properties, &counts)
        .map_err(failure)?;
    for name in state
        .0
        .borrow()
        .registry
        .schema(&call.component)
        .map_err(failure)?
        .slots
        .keys()
    {
        shared_slots.entry(name.clone()).or_default();
        call.slots.insert(name.clone(), ViewFragment::default());
    }
    let mut budget = state
        .0
        .try_borrow_mut()
        .map_err(|_| failure("composer budget already borrowed"))?;
    let own_bytes = call.own_property_bytes(&budget.limits).map_err(failure)?;
    let bytes = expanded_bytes.saturating_add(own_bytes);
    budget.nodes = budget.nodes.saturating_add(1);
    budget.bytes = budget.bytes.saturating_add(own_bytes);
    budget.edges = budget.edges.saturating_add(edges);
    if budget.nodes > budget.limits.occurrences
        || budget.bytes > budget.limits.property_bytes
        || bytes > budget.limits.property_bytes
        || budget.edges
            > budget
                .limits
                .occurrences
                .saturating_mul(budget.limits.depth)
    {
        return Err(failure(
            "ResourceLimit: cumulative native composition budget exceeded",
        ));
    }
    Ok(ScriptFragment {
        view: Arc::new(ScriptNode {
            component: call.component,
            key: call.key,
            properties: call.properties,
            slots: shared_slots,
        }),
        nodes: expanded_nodes,
        bytes,
        depth,
    })
}
fn publication(
    context: NativeCallContext<'_>,
    metadata: rhai::Map,
    root: ScriptFragment,
) -> ScriptResult<ScriptPublication> {
    make_publication(&context, metadata, root)
}
fn make_publication(
    context: &NativeCallContext<'_>,
    metadata: rhai::Map,
    root: ScriptFragment,
) -> ScriptResult<ScriptPublication> {
    let state = state(context)?;
    let mut metadata = metadata;
    metadata
        .entry("description".into())
        .or_insert(Dynamic::UNIT);
    let metadata: PublicationMetadata = rhai::serde::from_dynamic(&Dynamic::from(metadata))?;
    {
        let mut budget = state.0.borrow_mut();
        let bytes = root
            .bytes
            .saturating_add(metadata.id.0.len())
            .saturating_add(metadata.title.len())
            .saturating_add(metadata.description.as_ref().map_or(0, String::len));
        if bytes
            > budget
                .limits
                .property_bytes
                .saturating_sub(budget.lowered_bytes)
            || root.nodes
                > budget
                    .limits
                    .occurrences
                    .saturating_sub(budget.lowered_nodes)
        {
            return Err(failure(
                "ResourceLimit: cumulative publication lowering budget exceeded",
            ));
        }
        budget.lowered_bytes += bytes;
        budget.lowered_nodes += root.nodes;
    }
    let spec = PublicationSpec::new(metadata, root.lower()).map_err(failure)?;
    let limits = state.0.borrow().limits.clone();
    state
        .0
        .borrow()
        .registry
        .normalize(&spec, &limits)
        .map(|spec| ScriptPublication(Arc::new(spec)))
        .map_err(failure)
}
