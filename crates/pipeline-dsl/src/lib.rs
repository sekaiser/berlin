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
    plans: HashMap<String, PipelinePlan>,
    source_roots: std::collections::BTreeMap<String, String>,
    engine: Engine,
    ast: AST,
}

impl RhaiPipelineLoader {
    pub fn new(source: &str) -> Result<Self, CompileError> {
        let registry = PipelineRegistry::default();
        let engine = configured_engine(&registry);
        let (mut ast, source_roots) = compile_and_evaluate(&engine, source)?;
        let plans = registry.validated_plans(&ast)?;
        retain_function_definitions(&mut ast);
        Ok(Self {
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
    register_operations(&mut engine);
    register_pipeline_syntax(&mut engine, registry);
    engine
}

fn constrain_engine(engine: &mut Engine) {
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
