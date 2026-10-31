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
    limits: PublicationLimits,
    nodes: usize,
    bytes: usize,
    edges: usize,
    registry: Arc<ComponentRegistry>,
}
impl ComposerState {
    pub fn new(limits: PublicationLimits) -> Self {
        Self(Rc::new(RefCell::new(Budget {
            limits,
            nodes: 0,
            bytes: 0,
            edges: 0,
            registry: Arc::new(ComponentRegistry::builtins()),
        })))
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
    engine.register_type_with_name::<PublicationCatalog>("PublicationCatalog");
    engine.register_type_with_name::<ContentRef>("ContentRef");
    engine.register_type_with_name::<ExcerptRef>("ExcerptRef");
    engine.register_type_with_name::<ScriptFragment>("ViewFragment");
    engine.register_type_with_name::<PublicationSpec>("PublicationSpec");
    engine.register_fn(
        "select_work",
        |catalog: PublicationCatalog, query: rhai::Map| -> ScriptResult<rhai::Array> {
            let query: WorkQuery = rhai::serde::from_dynamic(&Dynamic::from(query))?;
            Ok(catalog
                .select(&query)
                .map_err(failure)?
                .into_iter()
                .map(Dynamic::from)
                .collect())
        },
    );
    engine.register_fn(
        "work_ref",
        |catalog: PublicationCatalog, id: ImmutableString| -> ScriptResult<ContentRef> {
            catalog.work_ref(&id).map_err(failure)
        },
    );
    engine.register_fn(
        "pinned_work_ref",
        |catalog: PublicationCatalog,
         id: ImmutableString,
         pins: rhai::Map|
         -> ScriptResult<ContentRef> {
            let mut reference = catalog.work_ref(&id).map_err(failure)?;
            for (name, value) in pins {
                let value = value
                    .try_cast::<ImmutableString>()
                    .ok_or_else(|| failure("pin values must be SHA-256 strings"))?;
                match name.as_str() {
                    "source_sha256" => reference.expected_source_hash = Some(value.into()),
                    "document_digest" => reference.expected_document_digest = Some(value.into()),
                    _ => return Err(failure(format!("unknown pin field '{name}'"))),
                }
            }
            catalog.pinned_work_ref(reference).map_err(failure)
        },
    );
    engine.register_fn(
        "section_ref",
        |catalog: PublicationCatalog,
         id: ImmutableString,
         section: ImmutableString|
         -> ScriptResult<ExcerptRef> {
            catalog.section_ref(&id, &section).map_err(failure)
        },
    );
    engine.register_fn("view_component", view_component);
    engine.register_fn("publication", publication);
}

fn decode(value: Dynamic, depth: usize) -> ScriptResult<Property> {
    if depth > 32 {
        return Err(failure("ResourceLimit: native property nesting exceeded"));
    }
    if value.is::<ContentRef>() {
        return Ok(Property::Source(value.cast()));
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
    let state = state(&context)?;
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
) -> ScriptResult<PublicationSpec> {
    let state = state(&context)?;
    let mut metadata = metadata;
    metadata
        .entry("description".into())
        .or_insert(Dynamic::UNIT);
    let metadata: PublicationMetadata = rhai::serde::from_dynamic(&Dynamic::from(metadata))?;
    let spec = PublicationSpec {
        schema_version: 1,
        metadata,
        root: root.lower(),
    };
    let limits = state.0.borrow().limits.clone();
    state
        .0
        .borrow()
        .registry
        .normalize(&spec, &limits)
        .map_err(failure)
}
