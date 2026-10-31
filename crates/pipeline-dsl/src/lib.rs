//! A constrained Rhai frontend that constructs Berlin's typed pipeline plans.
//!
//! The crate is organized around the stages of translating a Rhai program into
//! executable Berlin concepts:
//!
//! - `document_api` exposes semantic documents to user-defined Rhai mappers.
//! - `operations` defines the typed artifacts and functions used to construct
//!   pipeline graph fragments.
//! - `syntax` implements the `pipeline` and `output` forms that collect those
//!   fragments into validated plans.
//!
//! This module is the public facade. It configures the constrained Rhai engine,
//! coordinates compilation, and retains the compiled functions needed while a
//! pipeline executes.

mod document_api;
mod operations;
mod publication_api;
mod syntax;

use std::collections::HashMap;

use berlin_content::DocumentCollection;
use berlin_core::FunctionRef;
use berlin_core::Operation;
use berlin_core::PipelineLoader;
use berlin_core::PipelinePlan;
use berlin_document::Document;
use document_api::ScriptDocument;
use document_api::register_document_api;
use operations::register_operations;
use rhai::AST;
use rhai::Engine;
use rhai::EvalAltResult;
use rhai::Scope;
use syntax::PipelineRegistry;
use syntax::register_pipeline_syntax;

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("publication composer '{0}' is not defined as a one-argument Rhai function")]
    UnknownComposer(String),
    #[error("publication composer '{name}' failed: {source}")]
    Composition {
        name: String,
        source: Box<EvalAltResult>,
    },
    #[error("invalid publication: {0}")]
    InvalidPublication(#[from] berlin_publication::Diagnostic),
    #[error("SOURCE_ROOTS must be a map of simple names to nonempty directory strings")]
    InvalidSourceRoots,
    #[error("DSL parsing failed: {0}")]
    Parsing(#[from] rhai::ParseError),
    #[error("DSL evaluation failed: {0}")]
    Evaluation(#[from] Box<EvalAltResult>),
    #[error("DSL pipeline registry is already borrowed")]
    RegistryBorrowed,
    #[error("pipeline '{name}' is invalid: {source}")]
    InvalidPlan {
        name: String,
        source: berlin_core::PipelineValidationError,
    },
    #[error("pipeline '{0}' is not defined")]
    UnknownPipeline(String),
    #[error("document mapper '{0}' is not defined as a one-argument Rhai function")]
    UnknownMapper(String),
    #[error("document mapper '{name}' failed: {source}")]
    Mapping {
        name: String,
        source: Box<EvalAltResult>,
    },
    #[error("document mapper produced an invalid collection: {0}")]
    InvalidDocuments(#[from] berlin_content::CollectionError),
}

pub struct RhaiPipelineLoader {
    components: std::sync::Arc<berlin_publication::ComponentRegistry>,
    plans: HashMap<String, PipelinePlan>,
    source_roots: std::collections::BTreeMap<String, String>,
    engine: Engine,
    ast: AST,
}

impl RhaiPipelineLoader {
    pub fn compose_publication(
        &self,
        composer: &FunctionRef,
        documents: &DocumentCollection,
        options: &berlin_publication::PublicationOptions,
    ) -> Result<berlin_publication::PublicationAssembly, CompileError> {
        let catalog =
            berlin_publication::PublicationCatalog::new(documents.clone(), options.clone())?;
        self.compose_catalog(composer, catalog)
    }

    pub fn compose_catalog(
        &self,
        composer: &FunctionRef,
        catalog: berlin_publication::PublicationCatalog,
    ) -> Result<berlin_publication::PublicationAssembly, CompileError> {
        Ok(self.compose_artifact(composer, catalog)?.single()?.clone())
    }
    pub fn compose_artifact(
        &self,
        composer: &FunctionRef,
        catalog: berlin_publication::PublicationCatalog,
    ) -> Result<berlin_publication::ComposedArtifact, CompileError> {
        let state = publication_api::ComposerState::new(
            catalog.limits().clone(),
            catalog.options().resource_limits.clone(),
            self.components.clone(),
        );
        let spec = self
            .engine
            .call_fn_with_options::<rhai::Dynamic>(
                rhai::CallFnOptions::new().with_tag(state.clone()),
                &mut Scope::new(),
                &self.ast,
                composer.as_str(),
                (catalog.clone(),),
            )
            .map_err(|source| CompileError::Composition {
                name: composer.as_str().into(),
                source,
            })?;
        if spec.is::<publication_api::ScriptPublication>() {
            let mut spec = spec.cast::<publication_api::ScriptPublication>().0;
            std::sync::Arc::make_mut(&mut spec).queries = state.queries();
            Ok(berlin_publication::ComposedArtifact::Single(
                std::sync::Arc::new(berlin_publication::PublicationAssembly::resolve(
                    &spec,
                    catalog,
                    &self.components,
                )?),
            ))
        } else if spec.is::<publication_api::ScriptSite>() {
            let mut spec = spec.cast::<publication_api::ScriptSite>().0;
            std::sync::Arc::make_mut(&mut spec).queries = state.queries();
            Ok(berlin_publication::ComposedArtifact::Site(
                std::sync::Arc::new(berlin_publication::SiteAssembly::resolve(
                    &spec,
                    catalog,
                    &self.components,
                )?),
            ))
        } else {
            Err(berlin_publication::Diagnostic::new(
                berlin_publication::Code::InvalidProperty,
                berlin_publication::Stage::Construction,
                "composer must return a PublicationSpec or SiteSpec",
            )
            .into())
        }
    }
    pub fn new(source: &str) -> Result<Self, CompileError> {
        Self::with_components(source, berlin_publication::ComponentRegistry::builtins())
    }
    /// Freeze the caller's extension registry before script execution. Every
    /// native constructor and final resolver use this same contract snapshot.
    pub fn with_components(
        source: &str,
        components: berlin_publication::ComponentRegistry,
    ) -> Result<Self, CompileError> {
        let registry = PipelineRegistry::default();
        let engine = configured_engine(&registry);
        let (mut ast, source_roots) = compile_and_evaluate(&engine, source)?;
        let plans = registry.validated_plans(&ast)?;
        retain_function_definitions(&mut ast);
        Ok(Self {
            components: std::sync::Arc::new(components),
            plans,
            source_roots,
            engine,
            ast,
        })
    }

    /// Explicit read-only source locations, relative to the publishing project.
    pub fn source_roots(&self) -> &std::collections::BTreeMap<String, String> {
        &self.source_roots
    }

    pub fn map_documents(
        &self,
        mapper: &FunctionRef,
        documents: &DocumentCollection,
    ) -> Result<DocumentCollection, CompileError> {
        let mapped = documents
            .as_slice()
            .iter()
            .map(|document| self.map_document(mapper, document))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DocumentCollection::new(mapped).validated()?)
    }

    fn map_document(
        &self,
        mapper: &FunctionRef,
        document: &Document,
    ) -> Result<Document, CompileError> {
        let name = mapper.as_str();
        self.engine
            .call_fn::<ScriptDocument>(
                &mut Scope::new(),
                &self.ast,
                name,
                (ScriptDocument::new(document.clone()),),
            )
            .map(ScriptDocument::into_document)
            .map_err(|source| CompileError::Mapping {
                name: name.into(),
                source,
            })
    }
}

impl PipelineLoader for RhaiPipelineLoader {
    type Error = CompileError;

    fn load(&self, name: &str) -> Result<PipelinePlan, Self::Error> {
        self.plans
            .get(name)
            .cloned()
            .ok_or_else(|| CompileError::UnknownPipeline(name.into()))
    }
}

pub fn compile(source: &str) -> Result<HashMap<String, PipelinePlan>, CompileError> {
    Ok(RhaiPipelineLoader::new(source)?.plans)
}

fn configured_engine(registry: &PipelineRegistry) -> Engine {
    let mut engine = Engine::new();
    constrain_engine(&mut engine);
    register_document_api(&mut engine);
    publication_api::register(&mut engine);
    register_operations(&mut engine);
    register_pipeline_syntax(&mut engine, registry);
    engine
}

fn constrain_engine(engine: &mut Engine) {
    // Engine::new installs a filesystem resolver by default. Helper programs
    // must be explicit pipeline files so their bytes enter input snapshots.
    engine.set_module_resolver(rhai::module_resolvers::DummyModuleResolver::new());
    engine.disable_symbol("import");
    engine.set_max_operations(100_000);
    engine.set_max_call_levels(32);
    engine.set_max_expr_depths(64, 32);
    engine.set_max_array_size(10_000);
    engine.set_max_string_size(1_000_000);
    engine.disable_symbol("eval");
    engine.disable_symbol("print");
    engine.disable_symbol("debug");
}

fn compile_and_evaluate(
    engine: &Engine,
    source: &str,
) -> Result<(AST, std::collections::BTreeMap<String, String>), CompileError> {
    let ast = engine.compile(source)?;
    let mut scope = Scope::new();
    engine.eval_ast_with_scope::<()>(&mut scope, &ast)?;
    let mut roots = std::collections::BTreeMap::new();
    if scope.contains("SOURCE_ROOTS") {
        let map = scope
            .get_value::<rhai::Map>("SOURCE_ROOTS")
            .ok_or(CompileError::InvalidSourceRoots)?;
        for (name, value) in map {
            let path = value
                .try_cast::<rhai::ImmutableString>()
                .ok_or(CompileError::InvalidSourceRoots)?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                || path.trim().is_empty()
            {
                return Err(CompileError::InvalidSourceRoots);
            }
            roots.insert(name.into(), path.into());
        }
    }
    Ok((ast, roots))
}

fn validate_document_mappers(plan: &PipelinePlan, ast: &AST) -> Result<(), CompileError> {
    for node in plan.nodes() {
        if let Operation::ComposePublication { composer, .. } | Operation::Compose { composer } =
            &node.operation
            && !ast
                .iter_functions()
                .any(|f| f.name == composer.as_str() && f.params.len() == 1)
        {
            return Err(CompileError::UnknownComposer(composer.as_str().into()));
        }
        if let Operation::MapDocuments { mapper } = &node.operation {
            let exists = ast
                .iter_functions()
                .any(|function| function.name == mapper.as_str() && function.params.len() == 1);
            if !exists {
                return Err(CompileError::UnknownMapper(mapper.as_str().into()));
            }
        }
    }
    Ok(())
}

fn retain_function_definitions(ast: &mut AST) {
    ast.clear_statements();
}
