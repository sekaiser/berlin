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

#[derive(Clone)]
struct Fragment {
    nodes: Arc<Vec<PipelineNode>>,
    output: NodeId,
}

impl Fragment {
    fn source(id: &str, operation: Operation) -> Self {
        Self {
            nodes: Arc::new(vec![PipelineNode::new(id, operation)]),
            output: id.into(),
        }
    }

    fn transform(self, id: &str, operation: Operation, output: Option<&str>) -> Self {
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

        Self {
            nodes,
            output: id.into(),
        }
    }

    fn join(self, other: Self, id: &str, operation: Operation, output: Option<&str>) -> Self {
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

        Self {
            nodes,
            output: id.into(),
        }
    }

    fn named(mut self, id: &str) -> Self {
        let output = NodeId::new(id);
        let node = Arc::make_mut(&mut self.nodes)
            .iter_mut()
            .find(|node| node.id == self.output)
            .expect("a fragment output always refers to one of its nodes");
        node.id = output.clone();
        self.output = output;
        self
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
                Ok(Self(self.0.named(id.as_str())))
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
artifact!(Feed);
artifact!(WebsiteAssembly);
artifact!(CssSources);
artifact!(Stylesheets);
artifact!(StaticAssets);
artifact!(Website);
artifact!(LinkedInDrafts);
artifact!(PublicationAssembly);
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
        if value.is::<LinkedInDrafts>() {
            return value.try_cast().map(|value: LinkedInDrafts| Self(value.0));
        }
        if value.is::<Stylesheets>() {
            return value.try_cast().map(|value: Stylesheets| Self(value.0));
        }
        if value.is::<StaticAssets>() {
            return value.try_cast().map(|value: StaticAssets| Self(value.0));
        }
        None
    }

    pub(super) fn into_nodes(self) -> Vec<PipelineNode> {
        Arc::try_unwrap(self.0.nodes).unwrap_or_else(|nodes| nodes.as_ref().clone())
    }
}

pub(super) fn register_operations(engine: &mut Engine) {
    engine.register_fn("catalog", |parts: rhai::Array| {
        catalog(parts, berlin_publication::PublicationOptions::default())
    });
    engine.register_fn("catalog", catalog);
    engine.register_fn("compose", compose);
    engine.register_fn("render_html", render_html);
    engine.register_fn("render_text", render_text);
    engine.register_fn("with_assets", with_assets);
    engine.register_fn("html_config", website_config);
    engine.register_fn("documents", parse_markdown);
    engine.register_fn("documents", parse_documents);
    engine.register_fn("named", Catalog::named);
    engine.register_fn("named", Composed::named);
    engine.register_type_with_name::<berlin_publication::PublicationOptions>("PublicationOptions");
    engine.register_fn("publication_options",|settings:rhai::Map|->Result<berlin_publication::PublicationOptions,Box<rhai::EvalAltResult>>{let options:berlin_publication::PublicationOptions=rhai::serde::from_dynamic(&Dynamic::from(settings))?;options.limits.validate().map_err(|e|Box::<rhai::EvalAltResult>::from(e.to_string()))?;Ok(options)});
    engine.register_type_with_name::<berlin_publication::TextConfig>("TextConfig");
    engine.register_fn(
        "text_config",
        |settings: rhai::Map| -> Result<berlin_publication::TextConfig, Box<rhai::EvalAltResult>> {
            rhai::serde::from_dynamic(&Dynamic::from(settings))
        },
    );
    engine.register_fn("compose_publication", compose_publication);
    engine.register_fn("render_publication_website", render_publication_website);
    engine.register_fn("render_publication_text", render_publication_text);
    engine.register_fn("named", PublicationAssembly::named);
    engine.register_fn("named", PublicationText::named);
    engine.register_fn("load_latex", load_latex);
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
    engine.register_fn("load_org", load_org);
    engine.register_fn("export_org", export_org);
    engine.register_fn("load_markdown", load_markdown);
    engine.register_fn("parse_markdown", parse_markdown);
    engine.register_fn("parse_documents", parse_documents);
    engine.register_fn("merge_documents", merge_documents);
    engine.register_fn("map_documents", map_documents);
    engine.register_fn("load_data", load_data);
    engine.register_fn("resources", |source: DataSources| {
        resources(source, "resources".into())
    });
    engine.register_fn("resources", resources);
    engine.register_fn("parse_feed", parse_feed);
    engine.register_fn("assemble_website", assemble_website);
    engine.register_type_with_name::<WebsiteConfig>("WebsiteConfig");
    engine.register_fn("website_config", website_config);
    engine.register_fn("render_website", render_website);
    engine.register_type_with_name::<berlin_core::DeploymentTarget>("DeploymentTarget");
    engine.register_fn("github_pages", github_pages);
    engine.register_fn("deploy_to", deploy_website_to);
    engine.register_fn("render_linkedin", render_linkedin);
    engine.register_fn("load_css", load_css);
    engine.register_fn("compile_css", compile_css);
    engine.register_fn("load_assets", load_assets);
    engine.register_fn("copy_assets", copy_assets);

    engine.register_fn("named", OrgSources::named);
    engine.register_fn("named", LatexSources::named);
    engine.register_fn("named", MarkdownSources::named);
    engine.register_fn("named", Documents::named);
    engine.register_fn("named", DataSources::named);
    engine.register_fn("named", ExternalResources::named);
    engine.register_fn("named", Feed::named);
    engine.register_fn("named", WebsiteAssembly::named);
    engine.register_fn("named", CssSources::named);
    engine.register_fn("named", Stylesheets::named);
    engine.register_fn("named", StaticAssets::named);
    engine.register_fn("named", Website::named);
    engine.register_fn("named", LinkedInDrafts::named);
}

fn catalog(
    parts: rhai::Array,
    options: berlin_publication::PublicationOptions,
) -> Result<Catalog, Box<rhai::EvalAltResult>> {
    if parts.is_empty() || parts.len() > 64 {
        return Err("catalog requires 1..64 typed document/resource inputs".into());
    }
    options.limits.validate().map_err(|e| e.to_string())?;
    options
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
    let mut nodes = Vec::with_capacity(node_count + 1);
    for fragment in fragments {
        nodes.extend(fragment.nodes.iter().cloned());
    }
    let mut node = PipelineNode::new(
        "catalog",
        Operation::AssembleCatalog {
            parts: kinds,
            options: Box::new(options),
        },
    );
    node.dependencies = dependencies;
    nodes.push(node);
    Ok(Catalog(Fragment {
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
            composer: FunctionRef::new(composer.fn_name()),
        },
        None,
    )))
}
fn render_html(composed: Composed, output: ImmutableString, config: WebsiteConfig) -> Website {
    Website(composed.0.transform(
        "html",
        Operation::RenderHtml {
            config: Box::new(config),
            asset_mounts: vec![],
        },
        Some(&output),
    ))
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
    config: berlin_publication::TextConfig,
) -> PublicationText {
    PublicationText(
        composed
            .0
            .transform("text", Operation::RenderText { config }, Some(&output)),
    )
}

fn compose_publication(
    documents: Documents,
    composer: FnPtr,
    options: berlin_publication::PublicationOptions,
) -> Result<PublicationAssembly, Box<rhai::EvalAltResult>> {
    if composer.is_curried() || composer.is_anonymous() {
        return Err("publication composers must be named, uncurried one-argument functions".into());
    }
    Ok(PublicationAssembly(documents.0.transform(
        "publication_assembly",
        Operation::ComposePublication {
            composer: FunctionRef::new(composer.fn_name()),
            options: Box::new(options),
        },
        None,
    )))
}
fn render_publication_website(
    assembly: PublicationAssembly,
    output: ImmutableString,
    config: WebsiteConfig,
) -> Website {
    Website(assembly.0.transform(
        "publication_website",
        Operation::RenderPublicationWebsite {
            config: Box::new(config),
        },
        Some(output.as_str()),
    ))
}
fn render_publication_text(
    assembly: PublicationAssembly,
    output: ImmutableString,
    config: berlin_publication::TextConfig,
) -> PublicationText {
    PublicationText(assembly.0.transform(
        "publication_text",
        Operation::RenderPublicationText { config },
        Some(output.as_str()),
    ))
}

fn load_org(pattern: ImmutableString) -> Result<OrgSources, Box<rhai::EvalAltResult>> {
    if pattern.len() > 4096 || pattern.contains('\0') {
        return Err("source pattern exceeds its byte limit or contains NUL".into());
    }
    Ok(OrgSources(Fragment::source(
        "org_sources",
        Operation::LoadOrg {
            pattern: pattern.into(),
        },
    )))
}

fn load_latex(pattern: ImmutableString) -> Result<LatexSources, Box<rhai::EvalAltResult>> {
    if pattern.len() > 4096 || pattern.contains('\0') {
        return Err("source pattern exceeds its byte limit or contains NUL".into());
    }
    Ok(LatexSources(Fragment::source(
        "latex_sources",
        Operation::LoadLatex {
            pattern: pattern.into(),
        },
    )))
}

fn compile_papers(
    source: LatexSources,
    output: ImmutableString,
    url_prefix: ImmutableString,
) -> Documents {
    Documents(source.0.transform(
        "paper_documents",
        Operation::CompilePapers {
            url_prefix: url_prefix.into(),
        },
        Some(output.as_str()),
    ))
}

fn export_org(
    source: OrgSources,
    backend: ImmutableString,
    output: ImmutableString,
    section: ImmutableString,
) -> MarkdownSources {
    MarkdownSources(source.0.transform(
        "exported_markdown",
        Operation::ExportOrg {
            backend: backend.into(),
            section: section.into(),
        },
        Some(output.as_str()),
    ))
}

fn load_markdown(pattern: ImmutableString) -> Result<MarkdownSources, Box<rhai::EvalAltResult>> {
    if pattern.len() > 4096 || pattern.contains('\0') {
        return Err("source pattern exceeds its byte limit or contains NUL".into());
    }
    Ok(MarkdownSources(Fragment::source(
        "markdown_sources",
        Operation::LoadMarkdown {
            pattern: pattern.into(),
        },
    )))
}

fn parse_markdown(source: MarkdownSources) -> Documents {
    Documents(
        source
            .0
            .transform("parsed_documents", Operation::ParseMarkdown, None),
    )
}

fn map_documents(source: Documents, mapper: FnPtr) -> Documents {
    Documents(source.0.transform(
        "documents",
        Operation::MapDocuments {
            mapper: FunctionRef::new(mapper.fn_name()),
        },
        None,
    ))
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
    )))
}

fn load_data(pattern: ImmutableString) -> Result<DataSources, Box<rhai::EvalAltResult>> {
    if pattern.len() > 4096 || pattern.contains('\0') {
        return Err("source pattern exceeds its byte limit or contains NUL".into());
    }
    Ok(DataSources(Fragment::source(
        "feed_data",
        Operation::LoadData {
            pattern: pattern.into(),
        },
    )))
}

fn parse_feed(source: DataSources) -> Feed {
    Feed(source.0.transform("feed", Operation::ParseFeed, None))
}

fn resources(source: DataSources, namespace: ImmutableString) -> ExternalResources {
    ExternalResources(source.0.transform(
        "resources",
        Operation::ParseResources {
            namespace: namespace.into(),
        },
        None,
    ))
}

fn parse_documents(source: DataSources) -> Documents {
    Documents(
        source
            .0
            .transform("imported_documents", Operation::ParseDocuments, None),
    )
}

fn merge_documents(left: Documents, right: Documents) -> Documents {
    Documents(
        left.0
            .join(right.0, "merged_documents", Operation::MergeDocuments, None),
    )
}

fn assemble_website(documents: Documents, feed: Feed) -> WebsiteAssembly {
    WebsiteAssembly(
        documents
            .0
            .join(feed.0, "website_assembly", Operation::AssembleWebsite, None),
    )
}

fn website_config(settings: rhai::Map) -> Result<WebsiteConfig, Box<rhai::EvalAltResult>> {
    rhai::serde::from_dynamic(&Dynamic::from(settings))
}

fn render_website(
    assembly: WebsiteAssembly,
    output: ImmutableString,
    config: WebsiteConfig,
) -> Website {
    Website(assembly.0.transform(
        "website",
        Operation::RenderWebsite {
            config: Box::new(config),
        },
        Some(output.as_str()),
    ))
}

fn render_linkedin(documents: Documents, output: ImmutableString) -> LinkedInDrafts {
    LinkedInDrafts(documents.0.transform(
        "linkedin_drafts",
        Operation::RenderLinkedIn,
        Some(output.as_str()),
    ))
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

fn load_css(pattern: ImmutableString) -> Result<CssSources, Box<rhai::EvalAltResult>> {
    if pattern.len() > 4096 || pattern.contains('\0') {
        return Err("source pattern exceeds its byte limit or contains NUL".into());
    }
    Ok(CssSources(Fragment::source(
        "css_sources",
        Operation::LoadCss {
            pattern: pattern.into(),
        },
    )))
}

fn compile_css(source: CssSources, output: ImmutableString) -> Stylesheets {
    Stylesheets(
        source
            .0
            .transform("stylesheets", Operation::CompileCss, Some(output.as_str())),
    )
}

fn load_assets(pattern: ImmutableString) -> Result<StaticAssets, Box<rhai::EvalAltResult>> {
    if pattern.len() > 4096 || pattern.contains('\0') {
        return Err("source pattern exceeds its byte limit or contains NUL".into());
    }
    Ok(StaticAssets(Fragment::source(
        "static_sources",
        Operation::LoadAssets {
            pattern: pattern.into(),
        },
    )))
}

fn copy_assets(source: StaticAssets, output: ImmutableString) -> StaticAssets {
    StaticAssets(source.0.transform(
        "static_assets",
        Operation::CopyAssets,
        Some(output.as_str()),
    ))
}
