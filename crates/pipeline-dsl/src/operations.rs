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

#[derive(Clone)]
struct Fragment {
    nodes: Vec<PipelineNode>,
    output: NodeId,
}

impl Fragment {
    fn source(id: &str, operation: Operation) -> Self {
        Self {
            nodes: vec![PipelineNode::new(id, operation)],
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
            nodes.push(node);
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
            nodes.extend(other.nodes);
            nodes.push(node);
            nodes
        };

        Self {
            nodes,
            output: id.into(),
        }
    }

    fn named(mut self, id: &str) -> Self {
        let output = NodeId::new(id);
        let node = self
            .nodes
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
            fn named(self, id: ImmutableString) -> Self {
                Self(self.0.named(id.as_str()))
            }
        }
    };
}

artifact!(OrgSources);
artifact!(MarkdownSources);
artifact!(Documents);
artifact!(DataSources);
artifact!(Feed);
artifact!(WebsiteAssembly);
artifact!(CssSources);
artifact!(Stylesheets);
artifact!(StaticAssets);
artifact!(Website);
artifact!(LinkedInDrafts);

#[derive(Clone)]
pub(super) struct PipelineTarget(Fragment);

impl PipelineTarget {
    pub(super) fn from_dynamic(value: Dynamic) -> Option<Self> {
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
        self.0.nodes
    }
}

pub(super) fn register_operations(engine: &mut Engine) {
    engine.register_fn("load_org", load_org);
    engine.register_fn("export_org", export_org);
    engine.register_fn("load_markdown", load_markdown);
    engine.register_fn("parse_markdown", parse_markdown);
    engine.register_fn("map_documents", map_documents);
    engine.register_fn("load_data", load_data);
    engine.register_fn("parse_feed", parse_feed);
    engine.register_fn("assemble_website", assemble_website);
    engine.register_type_with_name::<WebsiteConfig>("WebsiteConfig");
    engine.register_fn("website_config", website_config);
    engine.register_fn("render_website", render_website);
    engine.register_fn("render_linkedin", render_linkedin);
    engine.register_fn("load_css", load_css);
    engine.register_fn("compile_css", compile_css);
    engine.register_fn("load_assets", load_assets);
    engine.register_fn("copy_assets", copy_assets);

    engine.register_fn("named", OrgSources::named);
    engine.register_fn("named", MarkdownSources::named);
    engine.register_fn("named", Documents::named);
    engine.register_fn("named", DataSources::named);
    engine.register_fn("named", Feed::named);
    engine.register_fn("named", WebsiteAssembly::named);
    engine.register_fn("named", CssSources::named);
    engine.register_fn("named", Stylesheets::named);
    engine.register_fn("named", StaticAssets::named);
    engine.register_fn("named", Website::named);
    engine.register_fn("named", LinkedInDrafts::named);
}

fn load_org(pattern: ImmutableString) -> OrgSources {
    OrgSources(Fragment::source(
        "org_sources",
        Operation::LoadOrg {
            pattern: pattern.into(),
        },
    ))
}

fn export_org(
    source: OrgSources,
    backend: ImmutableString,
    output: ImmutableString,
) -> MarkdownSources {
    MarkdownSources(source.0.transform(
        "exported_markdown",
        Operation::ExportOrg {
            backend: backend.into(),
        },
        Some(output.as_str()),
    ))
}

fn load_markdown(pattern: ImmutableString) -> MarkdownSources {
    MarkdownSources(Fragment::source(
        "markdown_sources",
        Operation::LoadMarkdown {
            pattern: pattern.into(),
        },
    ))
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

fn load_data(pattern: ImmutableString) -> DataSources {
    DataSources(Fragment::source(
        "feed_data",
        Operation::LoadData {
            pattern: pattern.into(),
        },
    ))
}

fn parse_feed(source: DataSources) -> Feed {
    Feed(source.0.transform("feed", Operation::ParseFeed, None))
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

fn load_css(pattern: ImmutableString) -> CssSources {
    CssSources(Fragment::source(
        "css_sources",
        Operation::LoadCss {
            pattern: pattern.into(),
        },
    ))
}

fn compile_css(source: CssSources, output: ImmutableString) -> Stylesheets {
    Stylesheets(
        source
            .0
            .transform("stylesheets", Operation::CompileCss, Some(output.as_str())),
    )
}

fn load_assets(pattern: ImmutableString) -> StaticAssets {
    StaticAssets(Fragment::source(
        "static_sources",
        Operation::LoadAssets {
            pattern: pattern.into(),
        },
    ))
}

fn copy_assets(source: StaticAssets, output: ImmutableString) -> StaticAssets {
    StaticAssets(source.0.transform(
        "static_assets",
        Operation::CopyAssets,
        Some(output.as_str()),
    ))
}
