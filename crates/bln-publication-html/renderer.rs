use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::sync::{Arc, Mutex};

use berlin_document::{ComponentId, PropertyValue};
use berlin_document_html::{Html, Renderer as SemanticRenderer, SemanticComponentPresenter};
use berlin_publication::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tera::{Context, Function, Tera};
#[path = "site.rs"]
mod site_renderer;
pub use site_renderer::*;

#[derive(Clone, Debug, Serialize)]
pub struct HtmlBinding {
    pub template: String,
    pub stylesheets: BTreeSet<String>,
}
#[derive(Clone, Default, Serialize)]
pub struct HtmlBindings(BTreeMap<ComponentKey, HtmlBinding>);
impl HtmlBindings {
    pub fn builtins() -> Self {
        let mut bindings = Self::default();
        for name in [
            "portfolio",
            "section",
            "introduction",
            "featured_work",
            "excerpt",
            "related_work",
            "callout",
            "resource_card",
            "citation",
            "reading_list",
            "resource_group",
        ] {
            bindings.0.insert(
                format!("berlin.{name}@1").parse().expect("built-in key"),
                HtmlBinding {
                    template: format!("components/berlin/{name}.v1.tera"),
                    stylesheets: BTreeSet::from(["portfolio.css".into()]),
                },
            );
        }
        bindings
    }
    pub fn register(&mut self, key: ComponentKey, binding: HtmlBinding) -> Result<()> {
        if self.0.contains_key(&key) {
            return Err(error(
                Code::DuplicateComponent,
                format!("HTML binding already registered: {key}"),
            ));
        }
        logical_name(&binding.template)?;
        self.0.insert(key, binding);
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PublicationSite {
    pub url: String,
    pub title: Option<String>,
    pub author: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RenderMetrics {
    pub template_snapshots_parsed: usize,
    pub templates_parsed: usize,
    pub environments_prepared: usize,
    pub peak_fragment_bytes: usize,
    pub components_rendered: usize,
    pub source_descriptions_parsed: usize,
}
pub struct HtmlPublication {
    pub html: String,
    pub manifest: PublicationManifest,
    pub stylesheets: BTreeSet<String>,
    pub metrics: RenderMetrics,
}

#[derive(Serialize)]
struct SourceView<'a> {
    id: &'a ResourceId,
    title: &'a str,
    url: &'a str,
    origin: ResourceOrigin,
    kind: &'a str,
    kind_class: &'static str,
    kind_label: &'static str,
    description: Option<&'a str>,
    authors: &'a [String],
    publisher: Option<&'a str>,
    published: Option<&'a berlin_document::PublicationDate>,
    added: Option<&'a berlin_document::PublicationDate>,
    tags: &'a [String],
    preview: Option<ResourcePreview>,
}

/// Compiled once from an immutable effective snapshot. Each root clones this
/// environment exactly once and installs a private fragment store.
pub struct PublicationHtmlRenderer {
    tera: Tera,
    bindings: HtmlBindings,
    input_digests: BTreeMap<String, String>,
}

impl PublicationHtmlRenderer {
    pub fn new(sources: BTreeMap<String, String>, bindings: HtmlBindings) -> Result<Self> {
        let mut input_digests = BTreeMap::new();
        for (name, source) in &sources {
            logical_name(name)?;
            input_digests.insert(format!("pages/{name}"), input_digest(source.as_bytes()));
        }
        let mut tera = Tera::default();
        tera.autoescape_on(vec![".tera"]);
        tera.add_raw_templates(sources).map_err(|e| {
            error(
                Code::TemplateContextFailure,
                format!("template snapshot: {}", error_chain(&e)),
            )
        })?;
        for name in ["now", "get_random", "get_env"] {
            tera.register_function(
                name,
                move |_: &HashMap<String, Value>| -> tera::Result<Value> {
                    Err(tera::Error::msg(format!(
                        "ambient function '{name}' is disabled; provide an explicit input"
                    )))
                },
            );
        }
        tera.register_filter(
            "safe",
            |_: &Value, _: &HashMap<String, Value>| -> tera::Result<Value> {
                Err(tera::Error::msg(
                    "safe is disabled in publications; use renderer-owned slots",
                ))
            },
        );
        tera.register_filter(
            "stable_hash",
            |value: &Value, _: &HashMap<String, Value>| -> tera::Result<Value> {
                let text = value
                    .as_str()
                    .ok_or_else(|| tera::Error::msg("stable_hash expects a string"))?;
                let hash = text.bytes().fold(2_166_136_261_u32, |hash, byte| {
                    (hash ^ u32::from(byte)).wrapping_mul(16_777_619)
                });
                Ok(hash.into())
            },
        );
        Ok(Self {
            tera,
            bindings,
            input_digests,
        })
    }

    pub fn render(
        &self,
        assembly: &PublicationAssembly,
        registry: &ComponentRegistry,
        site: &PublicationSite,
    ) -> Result<HtmlPublication> {
        self.render_mounted(
            assembly,
            registry,
            site,
            "publications/page.tera",
            None,
            &BTreeMap::new(),
        )
    }
    fn render_mounted(
        &self,
        assembly: &PublicationAssembly,
        registry: &ComponentRegistry,
        site: &PublicationSite,
        page_template: &str,
        extra_context: Option<&Context>,
        local_urls: &BTreeMap<ResourceId, String>,
    ) -> Result<HtmlPublication> {
        registry.validate_channel(assembly.spec(), Channel::Html, assembly.catalog().limits())?;
        let mount: url::Url = site
            .url
            .parse()
            .map_err(|e| error(Code::UnsafeUrl, format!("destination URL: {e}")))?;
        let mount = mount_url(&mount)?;
        let mut stylesheets = BTreeSet::new();
        let mut checked_bindings = BTreeSet::new();
        for placement in assembly.placements() {
            if !checked_bindings.insert(&placement.occurrence.component) {
                continue;
            }
            let binding = self
                .bindings
                .0
                .get(&placement.occurrence.component)
                .ok_or_else(|| {
                    error(
                        Code::MissingTemplate,
                        format!(
                            "missing HTML binding for {}",
                            placement.occurrence.component
                        ),
                    )
                    .at(&assembly.spec().metadata.id.0, &placement.occurrence.path)
                })?;
            if !self.tera.templates.contains_key(&binding.template) {
                return Err(template_error(
                    Code::MissingTemplate,
                    &binding.template,
                    "required component template is absent",
                ));
            }
            crate::audit::audit(&self.tera, &binding.template)
                .map_err(|e| template_error(Code::TemplateContextFailure, &binding.template, e))?;
            for entry in &binding.stylesheets {
                logical_name(entry)?;
                stylesheets.insert(entry.clone());
            }
        }
        for key in assembly.components() {
            if checked_bindings.contains(key) {
                continue;
            }
            let binding = self.bindings.0.get(key).ok_or_else(|| {
                error(
                    Code::MissingTemplate,
                    format!("missing semantic component binding for '{key}'"),
                )
            })?;
            if !self.tera.templates.contains_key(&binding.template) {
                return Err(template_error(
                    Code::MissingTemplate,
                    &binding.template,
                    "required semantic component template is absent",
                ));
            }
            crate::audit::audit(&self.tera, &binding.template)
                .map_err(|e| template_error(Code::TemplateContextFailure, &binding.template, e))?;
            for entry in &binding.stylesheets {
                logical_name(entry)?;
                stylesheets.insert(entry.clone());
            }
        }
        if !self.tera.templates.contains_key(page_template) {
            return Err(template_error(
                Code::MissingTemplate,
                page_template,
                "required page wrapper is absent",
            ));
        }
        crate::audit::audit(&self.tera, page_template)
            .map_err(|e| template_error(Code::TemplateContextFailure, page_template, e))?;
        let mut site_value = json!({"url":mount.as_str().trim_end_matches('/'), "title":site.title, "author":site.author, "stylesheets":stylesheets.iter().map(|entry| mount.join(&format!("assets/css/{entry}")).map(|u| u.to_string())).collect::<std::result::Result<Vec<_>, _>>().map_err(|e| error(Code::UnsafeUrl,e.to_string()))?});
        // One explicit presentation hint for components in a site. Do not leak
        // the surrounding template's arbitrary context into component scopes.
        if let Some(path) = extra_context.and_then(|context| context.get("page_path")) {
            site_value["page_path"] = path.clone();
        }

        let state = Arc::new(Mutex::new(SlotStore::new(
            assembly.catalog().limits().fragment_bytes,
        )));
        let mut tera = self.tera.clone();
        tera.register_function("render_slot", SlotBridge(state.clone()));
        let routes = assembly
            .catalog()
            .resources()
            .filter_map(|entry| {
                if entry.descriptor().origin == ResourceOrigin::Local {
                    local_urls
                        .get(&entry.descriptor().id)
                        .map(String::as_str)
                        .or_else(|| entry.original_url())
                        .map(|url| {
                            (
                                berlin_document::ContentId(entry.descriptor().id.leaf().into()),
                                url.to_owned(),
                            )
                        })
                } else {
                    None
                }
            })
            .collect::<HashMap<_, _>>();
        let mut descriptions = HashMap::new();
        for placement in assembly
            .placements()
            .iter()
            .filter(|p| p.occurrence.component.to_string() == "berlin.featured_work@1")
        {
            for reference in &placement.sources {
                if descriptions.contains_key(reference.id.as_str()) {
                    continue;
                }
                let resource = assembly.catalog().resolve_resource(reference)?;
                if let ResourceHandle::Local(entry) = resource.handle()
                    && let Some(description) = &entry.document().metadata.description
                {
                    if description.len() > assembly.catalog().limits().string_bytes {
                        return Err(error(
                            Code::ResourceLimit,
                            "source description exceeds string budget",
                        ));
                    }
                    let source = markdown::Source::in_memory(description);
                    let document = markdown::Parser::new()
                        .parse(&source)
                        .map_err(|e| error(Code::InvalidSource, e.to_string()))?;
                    descriptions.insert(reference.id.as_str().to_owned(), document.blocks);
                }
            }
        }
        let source_descriptions_parsed = descriptions.len();
        let mut session = Session {
            tera: &tera,
            renderer: self,
            assembly,
            state: state.clone(),
            site: &site_value,
            semantic: SemanticRenderer::default().with_document_routes(routes),
            cursor: 0,
            rendered: 0,
            descriptions,
            local_urls,
        };
        let root = session.component(&assembly.spec().root, 0)?;
        store_slot(&state, "page", "body", root)?;
        let mut context = context(
            assembly,
            &site_value,
            "page",
            1,
            Value::Object(Default::default()),
            slot_metadata(&state, "page")?,
        );
        if let Some(extra) = extra_context {
            context.extend(extra.clone());
        }
        let html = render_template(
            &tera,
            page_template,
            &context,
            &state,
            "page",
            assembly.catalog().limits(),
        )?;
        validate_structure(
            &html,
            site_value["stylesheets"].as_array().ok_or_else(|| {
                error(
                    Code::InvalidProperty,
                    "stylesheet projection must be an array",
                )
            })?,
        )?;
        let peak_fragment_bytes = state
            .lock()
            .map_err(|_| error(Code::TemplateContextFailure, "slot store poisoned"))?
            .peak;
        let configuration_digest = fingerprint(
            "html-config/v1",
            &(
                site,
                assembly.catalog().options(),
                &stylesheets,
                &self.bindings,
            ),
        )?;
        let manifest = assembly.manifest(
            Channel::Html,
            self.input_digests.clone(),
            configuration_digest,
        )?;
        Ok(HtmlPublication {
            html,
            manifest,
            stylesheets,
            metrics: RenderMetrics {
                template_snapshots_parsed: 1,
                templates_parsed: self.tera.templates.len(),
                environments_prepared: 1,
                peak_fragment_bytes,
                components_rendered: session.rendered,
                source_descriptions_parsed,
            },
        })
    }
}

struct Slot {
    html: String,
    consumed: bool,
    token: usize,
}
struct SlotStore {
    slots: BTreeMap<(String, String), Slot>,
    next: usize,
    bytes: usize,
    peak: usize,
    maximum: usize,
}
impl SlotStore {
    fn new(maximum: usize) -> Self {
        Self {
            slots: BTreeMap::new(),
            next: 0,
            bytes: 0,
            peak: 0,
            maximum,
        }
    }
}
type Store = Arc<Mutex<SlotStore>>;

struct SlotBridge(Store);
impl Function for SlotBridge {
    fn call(&self, args: &HashMap<String, Value>) -> tera::Result<Value> {
        if args.len() != 2 {
            return Err(tera::Error::msg("render_slot accepts only owner and name"));
        }
        let owner = args
            .get("owner")
            .and_then(Value::as_str)
            .ok_or_else(|| tera::Error::msg("render_slot requires an owner string"))?;
        let name = args
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| tera::Error::msg("render_slot requires a slot name"))?;
        let mut state = self
            .0
            .lock()
            .map_err(|_| tera::Error::msg("slot store poisoned"))?;
        let slot = state
            .slots
            .get_mut(&(owner.into(), name.into()))
            .ok_or_else(|| tera::Error::msg(format!("UnknownSlot: {owner}/{name}")))?;
        if slot.consumed {
            return Err(tera::Error::msg(format!("SlotRepetition: {owner}/{name}")));
        }
        slot.consumed = true;
        // Marker is an insertion token, not caller-provided HTML. It is expanded
        // only where the HTML parser recognizes a body element, never in raw
        // script/style text or an attribute.
        Ok(Value::String(format!(
            "<bln-rendered-slot data-key=\"{}\"></bln-rendered-slot>",
            slot.token
        )))
    }
    fn is_safe(&self) -> bool {
        true
    }
}

fn store_slot(store: &Store, owner: &str, name: &str, html: String) -> Result<()> {
    let mut state = store
        .lock()
        .map_err(|_| error(Code::TemplateContextFailure, "slot store poisoned"))?;
    if html.len() > state.maximum.saturating_sub(state.bytes) {
        return Err(error(
            Code::ResourceLimit,
            "live rendered-fragment byte budget exceeded",
        ));
    }
    if state.slots.contains_key(&(owner.into(), name.into())) {
        return Err(error(Code::UnknownSlot, "slot registered twice"));
    }
    let token = state.next;
    state.next += 1;
    state.bytes += html.len();
    state.peak = state.peak.max(state.bytes);
    state.slots.insert(
        (owner.into(), name.into()),
        Slot {
            html,
            consumed: false,
            token,
        },
    );
    Ok(())
}
fn slot_metadata(store: &Store, owner: &str) -> Result<Value> {
    let state = store
        .lock()
        .map_err(|_| error(Code::TemplateContextFailure, "slot store poisoned"))?;
    Ok(Value::Object(
        state
            .slots
            .iter()
            .filter(|((o, _), _)| o == owner)
            .map(|((_, name), slot)| {
                (
                    name.clone(),
                    json!({"exists":!slot.html.is_empty(), "bytes":slot.html.len()}),
                )
            })
            .collect(),
    ))
}

fn render_template(
    tera: &Tera,
    template: &str,
    context: &Context,
    store: &Store,
    owner: &str,
    limits: &PublicationLimits,
) -> Result<String> {
    let mut sink = BoundedWriter::new(limits.output_bytes);
    tera.render_to(template, context, &mut sink).map_err(|e| {
        let detail = error_chain(&e);
        let code = if detail.contains("SlotRepetition") {
            Code::SlotRepetition
        } else if detail.contains("UnknownSlot") {
            Code::UnknownSlot
        } else if detail.contains("ResourceLimit") {
            Code::ResourceLimit
        } else {
            Code::TemplateContextFailure
        };
        template_error(code, template, detail)
    })?;
    let rendered = String::from_utf8(sink.into_bytes())
        .map_err(|e| template_error(Code::TemplateContextFailure, template, e.to_string()))?;
    let slots = {
        let mut state = store
            .lock()
            .map_err(|_| error(Code::TemplateContextFailure, "slot store poisoned"))?;
        let keys: Vec<_> = state
            .slots
            .keys()
            .filter(|(o, _)| o == owner)
            .cloned()
            .collect();
        let mut slots = BTreeMap::new();
        for key in keys {
            let slot = state.slots.remove(&key).expect("key from this map");
            state.bytes -= slot.html.len();
            if !slot.html.is_empty() && !slot.consumed {
                return Err(template_error(
                    Code::SlotOmission,
                    template,
                    format!("nonempty slot '{owner}/{}' was omitted", key.1),
                ));
            }
            if slot.consumed {
                slots.insert(slot.token.to_string(), slot.html);
            }
        }
        slots
    };
    let mut remaining = slots;
    let mut sink = BoundedWriter::new(limits.output_bytes);
    let mut output_error = None;
    {
        let mut rewriter = lol_html::HtmlRewriter::new(
            lol_html::Settings::new().append_element_content_handler(lol_html::element!(
                "bln-rendered-slot",
                |element| {
                    let key = element
                        .get_attribute("data-key")
                        .ok_or("missing slot insertion key")?;
                    let html = remaining
                        .remove(&key)
                        .ok_or("unknown or repeated slot insertion token")?;
                    element.replace(&html, lol_html::html_content::ContentType::Html);
                    Ok(())
                }
            )),
            |chunk: &[u8]| {
                if output_error.is_none() {
                    output_error = sink.write_all(chunk).err();
                }
            },
        );
        rewriter
            .write(rendered.as_bytes())
            .and_then(|_| rewriter.end())
            .map_err(|e| template_error(Code::TemplateContextFailure, template, e.to_string()))?;
    }
    if output_error.is_some() {
        return Err(template_error(
            Code::ResourceLimit,
            template,
            "expanded HTML exceeded output byte budget",
        ));
    }
    let html = String::from_utf8(sink.into_bytes())
        .map_err(|e| error(Code::TemplateContextFailure, e.to_string()))?;
    if !remaining.is_empty() || html.contains("<bln-rendered-slot") {
        return Err(template_error(
            Code::TemplateContextFailure,
            template,
            "slots require HTML-body insertion; attributes, script/style and comments are forbidden",
        ));
    }
    Ok(html)
}

fn context(
    assembly: &PublicationAssembly,
    site: &Value,
    owner: &str,
    heading: u8,
    props: Value,
    slots: Value,
) -> Context {
    let mut context = Context::new();
    context.insert("props", &props);
    context.insert("publication", &assembly.spec().metadata);
    context.insert("site", site);
    // Explicit compatibility bridge for existing notebook macros. No other
    // project/process variables are flattened into component contexts.
    context.insert("config_site_url", &site["url"]);
    context.insert(
        "view",
        &json!({"id":owner,"anchor":owner,"heading_level":heading}),
    );
    context.insert("slots", &slots);
    context
}

struct Session<'a> {
    tera: &'a Tera,
    renderer: &'a PublicationHtmlRenderer,
    assembly: &'a PublicationAssembly,
    state: Store,
    site: &'a Value,
    semantic: SemanticRenderer,
    cursor: usize,
    rendered: usize,
    descriptions: HashMap<String, Vec<berlin_document::Block>>,
    local_urls: &'a BTreeMap<ResourceId, String>,
}
impl Session<'_> {
    fn component(&mut self, call: &ComponentCall, parent_heading: u8) -> Result<String> {
        let placement = self
            .assembly
            .placements()
            .get(self.cursor)
            .ok_or_else(|| error(Code::InvalidProperty, "assembly traversal mismatch"))?;
        self.cursor += 1;
        self.rendered += 1;
        let owner = placement.occurrence.id.0.clone();
        let key = &call.component;
        let heading = if key.name() == "portfolio" {
            1
        } else if [
            "section",
            "featured_work",
            "excerpt",
            "related_work",
            "resource_card",
            "citation",
            "reading_list",
            "resource_group",
        ]
        .contains(&key.name())
        {
            parent_heading + 1
        } else {
            parent_heading
        };
        if !(1..=6).contains(&heading) {
            return Err(error(
                Code::HeadingDepthExceeded,
                "HTML heading depth must remain between 1 and 6",
            )
            .at(
                &self.assembly.spec().metadata.id.0,
                &placement.occurrence.path,
            ));
        }
        let children_heading =
            if ["section", "portfolio", "reading_list", "resource_group"].contains(&key.name()) {
                heading
            } else {
                parent_heading
            };
        for (name, children) in &call.slots {
            let mut output = BoundedWriter::new(self.assembly.catalog().limits().output_bytes);
            for child in &children.0 {
                output
                    .write_all(self.component(child, children_heading)?.as_bytes())
                    .map_err(|e| error(Code::ResourceLimit, e.to_string()))?;
            }
            let html = String::from_utf8(output.into_bytes())
                .map_err(|e| error(Code::TemplateContextFailure, e.to_string()))?;
            store_slot(&self.state, &owner, name, html)?;
        }
        let mut props = serde_json::Map::new();
        for (name, property) in &call.properties {
            props.insert(name.clone(), self.property(property)?);
        }
        if let Some(Property::Text(text)) = call.properties.get("text") {
            props.insert(
                "paragraphs".into(),
                json!(text.split("\n\n").collect::<Vec<_>>()),
            );
        }
        if let Some(Property::Text(text)) = call.properties.get("annotation") {
            props.insert(
                "annotation_paragraphs".into(),
                json!(text.split("\n\n").collect::<Vec<_>>()),
            );
        }
        if key.to_string() == "berlin.featured_work@1" {
            let Property::Resource(reference) = &call.properties["source"] else {
                return Err(error(Code::InvalidProperty, "expected source"));
            };
            let resource = self.assembly.catalog().resolve_resource(reference)?;
            let description = if let Some(blocks) = self.descriptions.get(reference.id.as_str()) {
                let ResourceHandle::Local(entry) = resource.handle() else {
                    return Err(error(
                        Code::InvalidSource,
                        "semantic description requires local content",
                    ));
                };
                let original = entry
                    .original_url
                    .as_ref()
                    .ok_or_else(|| error(Code::UnsafeUrl, "missing original URL"))?;
                let mount = self
                    .assembly
                    .catalog()
                    .options()
                    .source_site_url
                    .as_ref()
                    .map(mount_url)
                    .transpose()?;
                let mut context = berlin_document_html::FragmentContext {
                    namespace: format!("{owner}--description"),
                    ..Default::default()
                };
                walk_blocks(blocks, &mut |block| {
                    if let berlin_document::Block::Figure(figure) = block {
                        context.destinations.insert(
                            figure.source.clone(),
                            resolve_source_url(&figure.source, original, mount.as_ref())?
                                .to_string(),
                        );
                    }
                    visit_block_inlines(block, &mut |inline| {
                        let source = match inline {
                            berlin_document::Inline::Link { destination, .. }
                                if !destination.starts_with("mailto:")
                                    && !destination.starts_with("tel:") =>
                            {
                                Some(destination)
                            }
                            berlin_document::Inline::Image { source, .. } => Some(source),
                            _ => None,
                        };
                        if let Some(source) = source {
                            context.destinations.insert(
                                source.clone(),
                                resolve_source_url(source, original, mount.as_ref())?.to_string(),
                            );
                        }
                        Ok(())
                    })
                })?;
                self.semantic
                    .render_fragment(
                        blocks,
                        &context,
                        None,
                        self.assembly.catalog().limits().output_bytes,
                    )
                    .map_err(|e| error(Code::TemplateContextFailure, e.to_string()))?
                    .into_string()
            } else {
                String::new()
            };
            store_slot(&self.state, &owner, "source_description", description)?;
        }
        if key.to_string() == "berlin.excerpt@1" {
            let Property::Excerpt(reference) = &call.properties["source"] else {
                return Err(error(Code::InvalidProperty, "expected excerpt"));
            };
            let excerpt = self.assembly.excerpt(reference)?;
            let base = match excerpt.blocks().first() {
                Some(berlin_document::Block::Section { level, .. }) => *level,
                _ => 2,
            };
            let context = crate::excerpt_context(
                excerpt,
                &placement.occurrence.id,
                heading as i8 - base as i8,
            )?;
            let presenter = Presenter {
                tera: self.tera,
                store: &self.state,
                assembly: self.assembly,
                site: self.site,
            };
            let html = self
                .semantic
                .render_fragment(
                    excerpt.blocks(),
                    &context,
                    Some(&presenter),
                    self.assembly.catalog().limits().output_bytes,
                )
                .map_err(|e| error(Code::TemplateContextFailure, e.to_string()))?
                .into_string();
            store_slot(&self.state, &owner, "excerpt", html)?;
        }
        let binding = &self.renderer.bindings.0[key];
        // Resource kinds may select only these renderer-owned names. Authored
        // kind strings are never interpolated into template paths.
        let variant = if key.to_string() == "berlin.resource_card@1" {
            let Property::Resource(reference) = &call.properties["resource"] else {
                return Err(error(Code::InvalidProperty, "expected resource reference"));
            };
            match self
                .assembly
                .catalog()
                .resolve_resource(reference)?
                .descriptor()
                .kind
                .as_str()
            {
                "repository" => Some("components/berlin/resource_card.repository.v1.tera"),
                "paper" => Some("components/berlin/resource_card.paper.v1.tera"),
                _ => None,
            }
        } else {
            None
        };
        let template = variant
            .filter(|name| self.tera.templates.contains_key(*name))
            .unwrap_or(&binding.template);
        crate::audit::audit(self.tera, template)
            .map_err(|e| template_error(Code::TemplateContextFailure, template, e))?;
        render_template(
            self.tera,
            template,
            &context(
                self.assembly,
                self.site,
                &owner,
                heading,
                Value::Object(props),
                slot_metadata(&self.state, &owner)?,
            ),
            &self.state,
            &owner,
            self.assembly.catalog().limits(),
        )
        .map_err(|e| {
            e.at(
                &self.assembly.spec().metadata.id.0,
                &placement.occurrence.path,
            )
        })
    }

    fn property(&self, property: &Property) -> Result<Value> {
        Ok(match property {
            Property::Text(text) => Value::String(text.clone()),
            Property::Boolean(b) => Value::Bool(*b),
            Property::Integer(i) => (*i).into(),
            Property::None => Value::Null,
            Property::Source(source) => self.source(source)?,
            Property::Resource(resource) => self.resource(resource)?,
            Property::Excerpt(excerpt) => self.source(&excerpt.source)?,
            Property::List(values) => Value::Array(
                values
                    .iter()
                    .map(|v| self.property(v))
                    .collect::<Result<_>>()?,
            ),
        })
    }
    fn source(&self, source: &ContentRef) -> Result<Value> {
        self.resource(&ResourceRef::try_from(source)?)
    }
    fn resource(&self, reference: &ResourceRef) -> Result<Value> {
        let entry = self.assembly.catalog().resolve_resource(reference)?;
        let descriptor = entry.descriptor();
        let url = self
            .local_urls
            .get(&reference.id)
            .map(String::as_str)
            .or_else(|| entry.original_url())
            .ok_or_else(|| error(Code::UnsafeUrl, "missing original resource URL"))?;
        let (kind_class, kind_label) = match descriptor.kind.as_str() {
            "repository" => ("repository", "Repository"),
            "paper" => ("paper", "Paper"),
            "article" => ("article", "Article"),
            "guide" => ("guide", "Guide"),
            "note" => ("note", "Note"),
            "documentation" => ("documentation", "Documentation"),
            "video" => ("video", "Video"),
            "website" => ("website", "Website"),
            "social_post" => ("social-post", "Social post"),
            _ => ("custom", "Resource"),
        };
        let (publisher, preview) = match entry.handle() {
            ResourceHandle::External(record) => (
                record.resource.publisher.as_deref(),
                record.resource.preview.clone(),
            ),
            ResourceHandle::Local(local) => {
                let preview = local
                    .document()
                    .metadata
                    .preview
                    .as_ref()
                    .map(|preview| {
                        let original = local
                            .original_url
                            .as_ref()
                            .ok_or_else(|| error(Code::UnsafeUrl, "missing preview source URL"))?;
                        let mount = self
                            .assembly
                            .catalog()
                            .options()
                            .source_site_url
                            .as_ref()
                            .map(mount_url)
                            .transpose()?;
                        Ok(ResourcePreview {
                            url: resolve_source_url(&preview.source, original, mount.as_ref())?
                                .to_string(),
                            alt: preview.alt.clone(),
                        })
                    })
                    .transpose()?;
                (None, preview)
            }
        };
        let view = SourceView {
            id: &descriptor.id,
            title: &descriptor.title,
            url,
            origin: descriptor.origin,
            kind: descriptor.kind.as_str(),
            kind_class,
            kind_label,
            description: descriptor.description.as_deref(),
            authors: &descriptor.authors,
            publisher,
            published: descriptor.published.as_ref(),
            added: descriptor.added.as_ref(),
            tags: &descriptor.topics,
            preview,
        };
        serde_json::to_value(view).map_err(|e| error(Code::InvalidProperty, e.to_string()))
    }
}

struct Presenter<'a> {
    tera: &'a Tera,
    store: &'a Store,
    assembly: &'a PublicationAssembly,
    site: &'a Value,
}
impl SemanticComponentPresenter for Presenter<'_> {
    fn present(
        &self,
        id: &ComponentId,
        name: &str,
        properties: &BTreeMap<String, PropertyValue>,
        children: Html,
    ) -> std::result::Result<String, String> {
        let run = || {
            if name == "bibliography-entry" {
                if !properties.is_empty() {
                    return Err(error(
                        Code::InvalidProperty,
                        "bibliography entries have no properties",
                    ));
                }
                return Ok(format!(
                    "<div id=\"{}\" data-component=\"bibliography-entry\">{}</div>",
                    html_escape::encode_double_quoted_attribute(&id.0),
                    children.into_string()
                ));
            }
            if name != "berlin.callout@1" {
                return Err(error(
                    Code::UnsupportedChannelComponent,
                    "unknown semantic component",
                ));
            }
            let mut props = serde_json::Map::new();
            props.insert("title".into(), Value::Null);
            for (key, value) in properties {
                if let PropertyValue::Text(text) = value {
                    props.insert(key.clone(), Value::String(text.clone()));
                }
            }
            store_slot(self.store, &id.0, "body", children.into_string())?;
            render_template(
                self.tera,
                "components/berlin/callout.v1.tera",
                &context(
                    self.assembly,
                    self.site,
                    &id.0,
                    1,
                    Value::Object(props),
                    slot_metadata(self.store, &id.0)?,
                ),
                self.store,
                &id.0,
                self.assembly.catalog().limits(),
            )
        };
        run().map_err(|e: Diagnostic| e.to_string())
    }
}

fn logical_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name.chars().any(char::is_control)
        || name
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(error(
            Code::UnsafeUrl,
            "presentation inputs require logical relative paths",
        ));
    }
    Ok(())
}
fn error(code: Code, detail: impl Into<String>) -> Diagnostic {
    Diagnostic::new(code, Stage::Presentation, detail)
}
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        text.push_str(": ");
        text.push_str(&error.to_string());
        source = error.source();
    }
    text
}
fn template_error(code: Code, template: &str, detail: impl Into<String>) -> Diagnostic {
    let mut e = error(code, detail);
    e.template = Some(template.into());
    e
}

fn validate_structure(html: &str, required_stylesheets: &[Value]) -> Result<()> {
    let mut ids = BTreeSet::new();
    let mut fragments = Vec::new();
    let mut h1 = 0;
    let mut stylesheets = HashMap::new();
    lol_html::rewrite_str(
        html,
        lol_html::RewriteStrSettings::new()
            .append_element_content_handler(lol_html::element!("*", |e| {
                if let Some(id) = e.get_attribute("id")
                    && !ids.insert(id.clone())
                {
                    return Err(format!("duplicate emitted ID '{id}'").into());
                }
                if e.tag_name() == "h1" {
                    h1 += 1;
                }
                if let Some(href) = e.get_attribute("href")
                    && let Some(fragment) = href.strip_prefix('#')
                {
                    fragments.push(fragment.to_owned());
                }
                Ok(())
            }))
            .append_element_content_handler(lol_html::element!(
                "head link[rel=stylesheet][href]",
                |element| {
                    if let Some(href) = element.get_attribute("href") {
                        *stylesheets
                            .entry(html_escape::decode_html_entities(&href).into_owned())
                            .or_insert(0) += 1;
                    }
                    Ok(())
                }
            )),
    )
    .map_err(|e| error(Code::TemplateContextFailure, e.to_string()))?;
    if h1 != 1 {
        return Err(error(
            Code::TemplateContextFailure,
            "publication HTML requires exactly one h1",
        ));
    }
    for stylesheet in required_stylesheets {
        if let Some(url) = stylesheet.as_str()
            && stylesheets.get(url) != Some(&1)
        {
            return Err(error(
                Code::TemplateContextFailure,
                format!("required stylesheet '{url}' must occur once in the page head"),
            ));
        }
    }
    for fragment in fragments {
        if !ids.contains(&fragment) {
            return Err(error(
                Code::UnresolvedReference,
                format!("missing emitted anchor '{fragment}'"),
            ));
        }
    }
    Ok(())
}
