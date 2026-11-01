//! Typed building blocks for pipeline graph construction.
//!
//! Each Rhai operation consumes and produces a distinct artifact type, making
//! invalid pipeline compositions fail during script evaluation. The artifacts
//! wrap graph fragments; they do not perform content processing themselves.

use berlin_core::FunctionRef;
use berlin_core::NodeId;
use berlin_core::Operation;
use berlin_core::PipelineNode;
use berlin_core::WebsiteConfig;
use rhai::Dynamic;
use rhai::Engine;
use rhai::FnPtr;
use rhai::ImmutableString;
use std::sync::Arc;

// Rhai counts script instructions, not the cost of cloning native graph values.
// Charge cumulative materialization before copy-on-write or configuration expansion.
#[derive(Clone, Default)]
struct GraphBudget(Arc<std::sync::atomic::AtomicUsize>);
impl GraphBudget {
    const MAX_BYTES: usize = 64 * 1024 * 1024;
    fn charge(&self, value: &impl serde::Serialize) -> Result<(), Box<rhai::EvalAltResult>> {
        let bytes = berlin_publication::encoded_size(value, Self::MAX_BYTES).map_err(|_| {
            Box::<rhai::EvalAltResult>::from("ResourceLimit: native graph byte budget exceeded")
        })?;
        self.0
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |used| {
                    used.checked_add(bytes)
                        .filter(|next| *next <= Self::MAX_BYTES)
                },
            )
            .map_err(|_| {
                Box::<rhai::EvalAltResult>::from(
                    "ResourceLimit: cumulative native graph byte budget exceeded",
                )
            })?;
        Ok(())
    }
}
#[derive(Clone)]
struct HtmlConfig(Arc<WebsiteConfig>);
#[derive(Clone)]
struct CatalogConfig(Arc<berlin_publication::PublicationOptions>);
#[derive(Clone)]
struct TextConfig(Arc<berlin_publication::TextConfig>);

#[derive(Clone)]
struct Fragment {
    budget: GraphBudget,
    nodes: Arc<Vec<PipelineNode>>,
    output: NodeId,
}

impl Fragment {
    fn source(
        id: &str,
        operation: Operation,
        budget: GraphBudget,
    ) -> Result<Self, Box<rhai::EvalAltResult>> {
        budget.charge(&operation)?;
        Ok(Self {
            budget,
            nodes: Arc::new(vec![PipelineNode::new(id, operation)]),
            output: id.into(),
        })
    }

    fn transform(
        self,
        id: &str,
        operation: Operation,
        output: Option<&str>,
    ) -> Result<Self, Box<rhai::EvalAltResult>> {
        if self.nodes.len() >= 10_000 {
            return Err("ResourceLimit: graph node count exceeded".into());
        }
        self.budget
            .charge(&(&self.nodes.as_ref(), &operation, output))?;
        let node = {
            let mut node = PipelineNode::new(id, operation).depends_on(self.output);
            if let Some(path) = output {
                node = node.output_to(path);
            }
            node
        };

        let nodes = {
            let mut nodes = self.nodes;
            Arc::make_mut(&mut nodes).push(node);
            nodes
        };

        Ok(Self {
            budget: self.budget,
            nodes,
            output: id.into(),
        })
    }

    fn join(
        self,
        other: Self,
        id: &str,
        operation: Operation,
        output: Option<&str>,
    ) -> Result<Self, Box<rhai::EvalAltResult>> {
        if self.nodes.len().saturating_add(other.nodes.len()) >= 10_000 {
            return Err("ResourceLimit: graph node count exceeded".into());
        }
        self.budget.charge(&(
            self.nodes.as_ref(),
            other.nodes.as_ref(),
            &operation,
            output,
        ))?;
        let node = {
            let mut node = PipelineNode::new(id, operation)
                .depends_on(self.output)
                .depends_on(other.output);
            if let Some(path) = output {
                node = node.output_to(path);
            }
            node
        };

        let nodes = {
            let mut nodes = self.nodes;
            Arc::make_mut(&mut nodes).extend(other.nodes.iter().cloned());
            Arc::make_mut(&mut nodes).push(node);
            nodes
        };

        Ok(Self {
            budget: self.budget,
            nodes,
            output: id.into(),
        })
    }

    fn named(mut self, id: &str) -> Result<Self, Box<rhai::EvalAltResult>> {
        self.budget.charge(&(self.nodes.as_ref(), id))?;
        let output = NodeId::new(id);
        let node = Arc::make_mut(&mut self.nodes)
            .iter_mut()
            .find(|node| node.id == self.output)
            .expect("a fragment output always refers to one of its nodes");
        node.id = output.clone();
        self.output = output;
        Ok(self)
    }
}

macro_rules! artifact {
    ($name:ident) => {
        #[derive(Clone)]
        struct $name(Fragment);

        impl $name {
            fn named(self, id: ImmutableString) -> Result<Self, Box<rhai::EvalAltResult>> {
                if id.is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
                    return Err(
                        "graph node identity must be nonempty, control-free and at most 256 bytes"
                            .into(),
                    );
                }
                Ok(Self(self.0.named(id.as_str())?))
            }
        }
    };
}

artifact!(OrgSources);
artifact!(LatexSources);
artifact!(MarkdownSources);
artifact!(Documents);
artifact!(DataSources);
artifact!(ExternalResources);
artifact!(Catalog);
artifact!(Composed);
artifact!(CssSources);
artifact!(Stylesheets);
artifact!(StaticAssets);
artifact!(Website);
artifact!(PublicationText);

#[derive(Clone)]
pub(super) struct PipelineTarget(Fragment);

impl PipelineTarget {
    pub(super) fn from_dynamic(value: Dynamic) -> Option<Self> {
        if value.is::<PublicationText>() {
            return value.try_cast().map(|value: PublicationText| Self(value.0));
        }
        if value.is::<MarkdownSources>() {
            return value.try_cast().map(|value: MarkdownSources| Self(value.0));
        }
        if value.is::<Website>() {
            return value.try_cast().map(|value: Website| Self(value.0));
        }
        if value.is::<Stylesheets>() {
            return value.try_cast().map(|value: Stylesheets| Self(value.0));
        }
        if value.is::<StaticAssets>() {
            return value.try_cast().map(|value: StaticAssets| Self(value.0));
        }
        None
    }

    pub(super) fn into_nodes(self) -> Result<Vec<PipelineNode>, Box<rhai::EvalAltResult>> {
        self.0.budget.charge(self.0.nodes.as_ref())?;
        Ok(Arc::try_unwrap(self.0.nodes).unwrap_or_else(|nodes| nodes.as_ref().clone()))
    }
}

// Inspect shared Rhai values by borrowing them before serde can expand a reused
// string/array into many owned configuration values.
fn preflight_preset(settings: &rhai::Map) -> Result<(), Box<rhai::EvalAltResult>> {
    fn charge(remaining: &mut usize, bytes: usize) -> Result<(), Box<rhai::EvalAltResult>> {
        *remaining = remaining
            .checked_sub(bytes)
            .ok_or("ResourceLimit: preset configuration byte budget exceeded before decoding")?;
        Ok(())
    }
    fn map(
        values: &rhai::Map,
        remaining: &mut usize,
        depth: usize,
    ) -> Result<(), Box<rhai::EvalAltResult>> {
        if depth > 32 {
            return Err("ResourceLimit: preset configuration nesting exceeded".into());
        }
        for (name, value) in values {
            charge(remaining, name.len().saturating_add(16))?;
            visit(value, remaining, depth + 1)?;
        }
        Ok(())
    }
    fn visit(
        value: &Dynamic,
        remaining: &mut usize,
        depth: usize,
    ) -> Result<(), Box<rhai::EvalAltResult>> {
        if depth > 32 {
            return Err("ResourceLimit: preset configuration nesting exceeded".into());
        }
        charge(remaining, 16)?;
        if let Some(text) = value.read_lock::<ImmutableString>() {
            if text.len() > 1_000_000 {
                return Err("ResourceLimit: preset string budget exceeded before decoding".into());
            }
            charge(remaining, text.len())
        } else if let Some(values) = value.read_lock::<rhai::Array>() {
            if values.len() > 10_000 {
                return Err("ResourceLimit: preset array budget exceeded before decoding".into());
            }
            for value in values.iter() {
                visit(value, remaining, depth + 1)?;
            }
            Ok(())
        } else if let Some(values) = value.read_lock::<rhai::Map>() {
            map(&values, remaining, depth + 1)
        } else if value.is::<bool>() || value.is::<rhai::INT>() || value.is_unit() {
            Ok(())
        } else {
            Err("InvalidProperty: preset options require plain configuration values".into())
        }
    }
    map(
        settings,
        &mut berlin_publication::PublicationLimits::default().property_bytes,
        0,
    )
}

pub(super) fn register_operations(engine: &mut Engine) {
    let budget = GraphBudget::default();
    engine.register_fn("catalog", |parts: rhai::Array| {
        catalog(parts, CatalogConfig(Arc::default()))
    });
    engine.register_fn("catalog", catalog);
    engine.register_fn("compose", compose);
    engine
        .register_type_with_name::<berlin_publication::StandardComposition>("StandardComposition");
    engine.register_fn("notebook", || {
        berlin_publication::StandardComposition::Notebook(Default::default())
    });
    engine.register_fn("article_draft", || {
        berlin_publication::StandardComposition::ArticleDraft(Default::default())
    });
    let preset_budget = budget.clone();
    engine.register_fn(
        "notebook",
        move |settings: rhai::Map| -> Result<berlin_publication::StandardComposition, Box<rhai::EvalAltResult>> {
            preflight_preset(&settings)?;
            let value = berlin_publication::StandardComposition::Notebook(
                rhai::serde::from_dynamic(&Dynamic::from(settings))?,
            );
            preset_budget.charge(&value)?;
            Ok(value)
        },
    );
    let preset_budget = budget.clone();
    engine.register_fn(
        "article_draft",
        move |settings: rhai::Map| -> Result<berlin_publication::StandardComposition, Box<rhai::EvalAltResult>> {
            preflight_preset(&settings)?;
            let value = berlin_publication::StandardComposition::ArticleDraft(
                rhai::serde::from_dynamic(&Dynamic::from(settings))?,
            );
            preset_budget.charge(&value)?;
            Ok(value)
        },
    );
    engine.register_fn(
        "compose",
        |catalog: Catalog,
         preset: berlin_publication::StandardComposition|
         -> Result<Composed, Box<rhai::EvalAltResult>> {
            Ok(Composed(catalog.0.transform(
                "composition",
                Operation::Compose {
                    composer: berlin_core::Composer::Standard(preset),
                },
                None,
            )?))
        },
    );
    engine.register_fn("render_html", render_html);
    engine.register_fn("render_text", render_text);
    engine.register_fn("with_assets", with_assets);
    let html_budget = budget.clone();
    engine.register_fn(
        "html_config",
        move |settings: rhai::Map| -> Result<HtmlConfig, Box<rhai::EvalAltResult>> {
            let config: WebsiteConfig = rhai::serde::from_dynamic(&Dynamic::from(settings))?;
            html_budget.charge(&config)?;
            Ok(HtmlConfig(Arc::new(config)))
        },
    );
    engine.register_fn("documents", parse_markdown);
    engine.register_fn("documents", parse_documents);
    engine.register_fn("named", Catalog::named);
    engine.register_fn("named", Composed::named);
    engine.register_type_with_name::<CatalogConfig>("CatalogConfig");
    let catalog_budget = budget.clone();
    engine.register_fn(
        "publication_options",
        move |settings: rhai::Map| -> Result<CatalogConfig, Box<rhai::EvalAltResult>> {
            let config: berlin_publication::PublicationOptions =
                rhai::serde::from_dynamic(&Dynamic::from(settings))?;
            config.limits.validate().map_err(|e| e.to_string())?;
            config
                .resource_limits
                .validate()
                .map_err(|e| e.to_string())?;
            catalog_budget.charge(&config)?;
            Ok(CatalogConfig(Arc::new(config)))
        },
    );
    engine.register_type_with_name::<TextConfig>("TextConfig");
    let text_budget = budget.clone();
    engine.register_fn(
        "text_config",
        move |settings: rhai::Map| -> Result<TextConfig, Box<rhai::EvalAltResult>> {
            let config: berlin_publication::TextConfig =
                rhai::serde::from_dynamic(&Dynamic::from(settings))?;
            text_budget.charge(&config)?;
            Ok(TextConfig(Arc::new(config)))
        },
    );
    engine.register_fn("named", PublicationText::named);
    let source_budget = budget.clone();
    engine.register_fn("load_latex", move |pattern: ImmutableString| {
        load_latex(pattern, source_budget.clone())
    });
    engine.register_fn(
        "compile_papers",
        |sources: LatexSources, output: ImmutableString| {
            compile_papers(sources, output, "/papers".into())
        },
    );
    engine.register_fn("compile_papers", compile_papers);
    engine.register_fn("compile_diagrams", compile_diagrams);
    engine.register_fn(
        "compile_diagrams",
        |source: DataSources, inputs: StaticAssets, output: ImmutableString| {
            compile_diagrams(source, inputs, output, "pdflatex".into())
        },
    );
    let source_budget = budget.clone();
    engine.register_fn("load_org", move |pattern: ImmutableString| {
        load_org(pattern, source_budget.clone())
    });
    engine.register_fn("export_org", export_org);
    let source_budget = budget.clone();
    engine.register_fn("load_markdown", move |pattern: ImmutableString| {
        load_markdown(pattern, source_budget.clone())
    });
    engine.register_fn("merge_documents", merge_documents);
    engine.register_fn("map_documents", map_documents);
    let source_budget = budget.clone();
    engine.register_fn("load_data", move |pattern: ImmutableString| {
        load_data(pattern, source_budget.clone())
    });
    engine.register_fn("resources", |source: DataSources| {
        resources(source, "resources".into())
    });
    engine.register_fn("resources", resources);
    engine.register_type_with_name::<HtmlConfig>("HtmlConfig");
    engine.register_type_with_name::<berlin_core::DeploymentTarget>("DeploymentTarget");
    engine.register_fn("github_pages", github_pages);
    engine.register_fn("deploy_to", deploy_website_to);
    let source_budget = budget.clone();
    engine.register_fn("load_css", move |pattern: ImmutableString| {
        load_css(pattern, source_budget.clone())
    });
    engine.register_fn("compile_css", compile_css);
    let source_budget = budget.clone();
    engine.register_fn("load_assets", move |pattern: ImmutableString| {
        load_assets(pattern, source_budget.clone())
    });
    engine.register_fn("copy_assets", copy_assets);

    engine.register_fn("named", OrgSources::named);
    engine.register_fn("named", LatexSources::named);
    engine.register_fn("named", MarkdownSources::named);
    engine.register_fn("named", Documents::named);
    engine.register_fn("named", DataSources::named);
    engine.register_fn("named", ExternalResources::named);
    engine.register_fn("named", CssSources::named);
    engine.register_fn("named", Stylesheets::named);
    engine.register_fn("named", StaticAssets::named);
    engine.register_fn("named", Website::named);
}

fn catalog(
    parts: rhai::Array,
    options: CatalogConfig,
) -> Result<Catalog, Box<rhai::EvalAltResult>> {
    if parts.is_empty() || parts.len() > 64 {
        return Err("catalog requires 1..64 typed document/resource inputs".into());
    }
    options.0.limits.validate().map_err(|e| e.to_string())?;
    options
        .0
        .resource_limits
        .validate()
        .map_err(|e| e.to_string())?;
    let mut fragments = Vec::new();
    let mut kinds = Vec::new();
    let mut dependencies = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut node_count = 0usize;
    for part in parts {
        let (fragment, kind) = if part.is::<Documents>() {
            (
                part.cast::<Documents>().0,
                berlin_core::ArtifactKind::Documents,
            )
        } else if part.is::<ExternalResources>() {
            (
                part.cast::<ExternalResources>().0,
                berlin_core::ArtifactKind::ExternalResources,
            )
        } else {
            return Err("catalog parts must be documents(...) or resources(...)".into());
        };
        if !seen.insert(fragment.output.as_str().to_owned()) {
            return Err("catalog has a duplicate dependency".into());
        }
        node_count = node_count
            .checked_add(fragment.nodes.len())
            .ok_or("catalog graph size overflow")?;
        if node_count > 10_000 {
            return Err("catalog graph node limit exceeded".into());
        }
        kinds.push(kind);
        dependencies.push(fragment.output.clone());
        fragments.push(fragment);
    }
    let budget = fragments[0].budget.clone();
    budget.charge(options.0.as_ref())?;
    for fragment in &fragments {
        budget.charge(fragment.nodes.as_ref())?;
    }
    let mut nodes = Vec::with_capacity(node_count + 1);
    for fragment in fragments {
        nodes.extend(fragment.nodes.iter().cloned());
    }
    let mut node = PipelineNode::new(
        "catalog",
        Operation::AssembleCatalog {
            parts: kinds,
            options: Box::new(options.0.as_ref().clone()),
        },
    );
    node.dependencies = dependencies;
    nodes.push(node);
    Ok(Catalog(Fragment {
        budget,
        nodes: Arc::new(nodes),
        output: "catalog".into(),
    }))
}
fn compose(catalog: Catalog, composer: FnPtr) -> Result<Composed, Box<rhai::EvalAltResult>> {
    if composer.is_curried() || composer.is_anonymous() {
        return Err("composers must be named, uncurried one-argument functions".into());
    }
    Ok(Composed(catalog.0.transform(
        "composition",
        Operation::Compose {
            composer: FunctionRef::new(composer.fn_name()).into(),
        },
        None,
    )?))
}
fn render_html(
    composed: Composed,
    output: ImmutableString,
    config: HtmlConfig,
) -> Result<Website, Box<rhai::EvalAltResult>> {
    composed.0.budget.charge(config.0.as_ref())?;
    Ok(Website(composed.0.transform(
        "html",
        Operation::RenderHtml {
            config: Box::new(config.0.as_ref().clone()),
            asset_mounts: vec![],
        },
        Some(&output),
    )?))
}
fn with_assets(
    mut website: Website,
    assets: StaticAssets,
    mount: ImmutableString,
) -> Result<Website, Box<rhai::EvalAltResult>> {
    if mount.is_empty()
        || mount.len() > 1024
        || mount.split('/').any(|p| {
            p.is_empty()
                || p == "."
                || p == ".."
                || !p
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
    {
        return Err("asset mount must be a confined relative path".into());
    }
    website.0.budget.charge(&(
        website.0.nodes.as_ref(),
        assets.0.nodes.as_ref(),
        mount.as_str(),
    ))?;
    let output = website.0.output.clone();
    let nodes = Arc::make_mut(&mut website.0.nodes);
    let node = nodes
        .iter_mut()
        .find(|n| n.id == output)
        .expect("website output node");
    let Operation::RenderHtml { asset_mounts, .. } = &mut node.operation else {
        return Err("with_assets requires render_html".into());
    };
    if asset_mounts.len() >= 64 {
        return Err("asset mount count exceeds 64".into());
    }
    if node.dependencies.contains(&assets.0.output) {
        return Err("duplicate asset dependency".into());
    }
    asset_mounts.push(mount.into());
    node.dependencies.push(assets.0.output);
    if nodes.len().saturating_add(assets.0.nodes.len()) > 10_000 {
        return Err("HTML graph node count exceeded".into());
    }
    nodes.extend(assets.0.nodes.iter().cloned());
    Ok(website)
}
fn render_text(
    composed: Composed,
    output: ImmutableString,
    config: TextConfig,
) -> Result<PublicationText, Box<rhai::EvalAltResult>> {
    composed.0.budget.charge(config.0.as_ref())?;
    Ok(PublicationText(composed.0.transform(
        "text",
        Operation::RenderText {
            config: config.0.as_ref().clone(),
        },
        Some(&output),
    )?))
}

fn load_org(
    pattern: ImmutableString,
    budget: GraphBudget,
) -> Result<OrgSources, Box<rhai::EvalAltResult>> {
    if pattern.len() > 4096 || pattern.contains('\0') {
        return Err("source pattern exceeds its byte limit or contains NUL".into());
    }
    Ok(OrgSources(Fragment::source(
        "org_sources",
        Operation::LoadOrg {
            pattern: pattern.into(),
        },
        budget,
    )?))
}

fn load_latex(
    pattern: ImmutableString,
    budget: GraphBudget,
) -> Result<LatexSources, Box<rhai::EvalAltResult>> {
    if pattern.len() > 4096 || pattern.contains('\0') {
        return Err("source pattern exceeds its byte limit or contains NUL".into());
    }
    Ok(LatexSources(Fragment::source(
        "latex_sources",
        Operation::LoadLatex {
            pattern: pattern.into(),
        },
        budget,
    )?))
}

fn compile_papers(
    source: LatexSources,
    output: ImmutableString,
    url_prefix: ImmutableString,
) -> Result<Documents, Box<rhai::EvalAltResult>> {
    Ok(Documents(source.0.transform(
        "paper_documents",
        Operation::CompilePapers {
            url_prefix: url_prefix.into(),
        },
        Some(output.as_str()),
    )?))
}

fn export_org(
    source: OrgSources,
    backend: ImmutableString,
    output: ImmutableString,
    section: ImmutableString,
) -> Result<MarkdownSources, Box<rhai::EvalAltResult>> {
    Ok(MarkdownSources(source.0.transform(
        "exported_markdown",
        Operation::ExportOrg {
            backend: backend.into(),
            section: section.into(),
        },
        Some(output.as_str()),
    )?))
}

fn load_markdown(
    pattern: ImmutableString,
    budget: GraphBudget,
) -> Result<MarkdownSources, Box<rhai::EvalAltResult>> {
    if pattern.len() > 4096 || pattern.contains('\0') {
        return Err("source pattern exceeds its byte limit or contains NUL".into());
    }
    Ok(MarkdownSources(Fragment::source(
        "markdown_sources",
        Operation::LoadMarkdown {
            pattern: pattern.into(),
        },
        budget,
    )?))
}

fn parse_markdown(source: MarkdownSources) -> Result<Documents, Box<rhai::EvalAltResult>> {
    Ok(Documents(source.0.transform(
        "parsed_documents",
        Operation::ParseMarkdown,
        None,
    )?))
}

fn map_documents(source: Documents, mapper: FnPtr) -> Result<Documents, Box<rhai::EvalAltResult>> {
    Ok(Documents(source.0.transform(
        "documents",
        Operation::MapDocuments {
            mapper: FunctionRef::new(mapper.fn_name()),
        },
        None,
    )?))
}

fn compile_diagrams(
    source: DataSources,
    inputs: StaticAssets,
    output: ImmutableString,
    engine: ImmutableString,
) -> Result<StaticAssets, Box<rhai::EvalAltResult>> {
    let engine = engine
        .as_str()
        .parse::<berlin_core::TexEngine>()
        .map_err(Box::<rhai::EvalAltResult>::from)?;
    Ok(StaticAssets(source.0.join(
        inputs.0,
        "diagrams",
        Operation::CompileDiagrams { engine },
        Some(output.as_str()),
    )?))
}

fn load_data(
    pattern: ImmutableString,
    budget: GraphBudget,
) -> Result<DataSources, Box<rhai::EvalAltResult>> {
    if pattern.len() > 4096 || pattern.contains('\0') {
        return Err("source pattern exceeds its byte limit or contains NUL".into());
    }
    Ok(DataSources(Fragment::source(
        "feed_data",
        Operation::LoadData {
            pattern: pattern.into(),
        },
        budget,
    )?))
}

fn resources(
    source: DataSources,
    namespace: ImmutableString,
) -> Result<ExternalResources, Box<rhai::EvalAltResult>> {
    Ok(ExternalResources(source.0.transform(
        "resources",
        Operation::ParseResources {
            namespace: namespace.into(),
        },
        None,
    )?))
}

fn parse_documents(source: DataSources) -> Result<Documents, Box<rhai::EvalAltResult>> {
    Ok(Documents(source.0.transform(
        "imported_documents",
        Operation::ParseDocuments,
        None,
    )?))
}

fn merge_documents(
    left: Documents,
    right: Documents,
) -> Result<Documents, Box<rhai::EvalAltResult>> {
    Ok(Documents(left.0.join(
        right.0,
        "merged_documents",
        Operation::MergeDocuments,
        None,
    )?))
}

fn github_pages(
    repository: ImmutableString,
) -> Result<berlin_core::DeploymentTarget, Box<rhai::EvalAltResult>> {
    let target = berlin_core::DeploymentTarget::GitHubPages {
        repository: repository.into(),
    };
    target
        .validate()
        .map_err(Box::<rhai::EvalAltResult>::from)?;
    Ok(target)
}

fn deploy_website_to(
    mut website: Website,
    target: berlin_core::DeploymentTarget,
) -> Result<Website, Box<rhai::EvalAltResult>> {
    website
        .0
        .budget
        .charge(&(website.0.nodes.as_ref(), &target))?;
    let output = Arc::make_mut(&mut website.0.nodes)
        .iter_mut()
        .find(|node| node.id == website.0.output)
        .expect("website output refers to a node");
    if output.deployment.is_some() {
        return Err("website already has a deployment target".into());
    }
    output.deployment = Some(target);
    Ok(website)
}

fn load_css(
    pattern: ImmutableString,
    budget: GraphBudget,
) -> Result<CssSources, Box<rhai::EvalAltResult>> {
    if pattern.len() > 4096 || pattern.contains('\0') {
        return Err("source pattern exceeds its byte limit or contains NUL".into());
    }
    Ok(CssSources(Fragment::source(
        "css_sources",
        Operation::LoadCss {
            pattern: pattern.into(),
        },
        budget,
    )?))
}

fn compile_css(
    source: CssSources,
    output: ImmutableString,
) -> Result<Stylesheets, Box<rhai::EvalAltResult>> {
    Ok(Stylesheets(source.0.transform(
        "stylesheets",
        Operation::CompileCss,
        Some(output.as_str()),
    )?))
}

fn load_assets(
    pattern: ImmutableString,
    budget: GraphBudget,
) -> Result<StaticAssets, Box<rhai::EvalAltResult>> {
    if pattern.len() > 4096 || pattern.contains('\0') {
        return Err("source pattern exceeds its byte limit or contains NUL".into());
    }
    Ok(StaticAssets(Fragment::source(
        "static_sources",
        Operation::LoadAssets {
            pattern: pattern.into(),
        },
        budget,
    )?))
}

fn copy_assets(
    source: StaticAssets,
    output: ImmutableString,
) -> Result<StaticAssets, Box<rhai::EvalAltResult>> {
    Ok(StaticAssets(source.0.transform(
        "static_assets",
        Operation::CopyAssets,
        Some(output.as_str()),
    )?))
}

#[cfg(test)]
mod allocation_tests {
    use super::*;

    #[test]
    fn native_graph_budget_accepts_exact_boundary_and_refuses_before_cow() {
        let budget = GraphBudget::default();
        let mut fragment = load_markdown("article.md".into(), budget.clone())
            .unwrap()
            .0;
        let retained = fragment.clone();
        let charge = berlin_publication::encoded_size(
            &(fragment.nodes.as_ref(), "renamed"),
            GraphBudget::MAX_BYTES,
        )
        .unwrap();
        budget.0.store(
            GraphBudget::MAX_BYTES - charge,
            std::sync::atomic::Ordering::Relaxed,
        );
        fragment = fragment.named("renamed").unwrap();
        assert_eq!(fragment.output.as_str(), "renamed");
        assert_eq!(retained.output.as_str(), "markdown_sources");
        let before = retained.nodes.clone();
        assert!(retained.named("overflow").is_err());
        assert_eq!(before[0].id.as_str(), "markdown_sources");
        assert_eq!(
            budget.0.load(std::sync::atomic::Ordering::Relaxed),
            GraphBudget::MAX_BYTES
        );
    }

    #[test]
    fn preset_configuration_preflight_bounds_shared_payloads_before_decoding() {
        // Exercise the native boundary directly, independently of interpreter
        // checks that can also refuse large compound values earlier.
        let text = ImmutableString::from("x".repeat(65_536));
        let page = rhai::Map::from_iter([
            ("id".into(), Dynamic::from("about")),
            ("title".into(), Dynamic::from("About")),
            ("route".into(), Dynamic::from("about.html")),
            ("introduction".into(), Dynamic::from(text)),
        ]);
        let small = rhai::Map::from_iter([(
            "pages".into(),
            Dynamic::from(vec![Dynamic::from(page.clone())]),
        )]);
        assert!(preflight_preset(&small).is_ok());
        let shared = rhai::Map::from_iter([(
            "pages".into(),
            Dynamic::from(vec![Dynamic::from(page); 200]),
        )]);
        let error = preflight_preset(&shared).unwrap_err();
        assert!(error.to_string().contains("before decoding"));
    }

    #[test]
    fn repeated_native_configuration_values_share_the_payload() {
        let config = HtmlConfig(Arc::new(WebsiteConfig::default()));
        let clones = vec![config.clone(); 10_000];
        assert!(clones.iter().all(|c| Arc::ptr_eq(&c.0, &config.0)));
    }
}
