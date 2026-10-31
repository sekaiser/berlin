//! Prepares only the document branch of one website output; no publishing effects.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Error, bail};
use berlin_content::AuthoringReport;
use berlin_core::{NodeId, Operation, PipelineLoader, PipelinePlan};

use super::{ExecutionOptions, RuntimeArtifact, executor::NodeExecutor};
use crate::project::Project;

#[derive(serde::Serialize)]
#[serde(untagged)]
pub(crate) enum Report {
    Authoring(AuthoringReport),
    Publication {
        kind: &'static str,
        schema_version: u8,
        publication: String,
        placements: usize,
        sources: usize,
        composition_fingerprint: String,
        diagnostics: Vec<super::receipt::Diagnostic>,
        authoring: Option<AuthoringReport>,
    },
}
impl Report {
    pub fn has_errors(&self) -> bool {
        match self {
            Self::Authoring(report) => report.has_errors(),
            Self::Publication { authoring, .. } => {
                authoring.as_ref().is_some_and(AuthoringReport::has_errors)
            }
        }
    }
}

pub(crate) fn inspect(project: &Project, pipeline: &str) -> Result<Report, Error> {
    let program = crate::pipeline::load_pipeline_program(project)?;
    let plan = program
        .load(pipeline)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    plan.validate()?;
    let document_root = publication_documents(&plan)?;
    let composed = plan.nodes().iter().any(|node| {
        &node.id == document_root
            && matches!(
                node.operation,
                Operation::RenderHtml { .. } | Operation::RenderText { .. }
            )
    });
    let required = document_branch(&plan, document_root)?;
    let mut artifacts = HashMap::new();
    let mut diagnostics = Vec::new();
    let mut consumed_inputs = Vec::new();

    // The complete branch has been checked against a read-only allowlist before
    // executing any node. In particular, dry-run Org export is not a substitute
    // for the Markdown that would actually be published.
    for node in plan
        .topological_order()?
        .into_iter()
        .filter(|node| required.contains(&node.id))
    {
        let artifact = NodeExecutor {
            consumed_inputs: &mut consumed_inputs,
            project,
            artifacts: &artifacts,
            program: &program,
            options: ExecutionOptions {
                dry_run: true,
                ..ExecutionOptions::default()
            },
            output_root: project.root(),
            diagnostics: &mut diagnostics,
        }
        .execute(node)?;
        let Some(artifact) = artifact else {
            if matches!(
                node.operation,
                Operation::RenderHtml { .. } | Operation::RenderText { .. }
            ) {
                continue;
            }
            bail!("Document preparation did not produce an artifact");
        };
        if matches!(&artifact,RuntimeArtifact::MarkdownSources(sources) if sources.is_empty())
            || (composed
                && matches!(&artifact,RuntimeArtifact::DataSources(sources) if sources.is_empty()))
        {
            bail!(
                "Markdown source node '{}' matched no files; export Org first if this pipeline reads generated Markdown",
                node.id
            );
        }
        artifacts.insert(node.id.clone(), artifact);
        if let Some(RuntimeArtifact::Catalog(catalog)) = artifacts.get(&node.id) {
            let report = authoring_report(project, catalog)?;
            if report.has_errors() {
                return Ok(Report::Authoring(report));
            }
        }
    }
    if let Some(RuntimeArtifact::Composed(composed)) = artifacts
        .values()
        .find(|artifact| matches!(artifact, RuntimeArtifact::Composed(_)))
    {
        let (publication, placements, sources, composition_fingerprint) = match composed {
            berlin_publication::ComposedArtifact::Single(assembly) => (
                assembly.spec().metadata.id.0.clone(),
                assembly.placements().len(),
                assembly.sources().len(),
                assembly.composition_fingerprint().to_owned(),
            ),
            berlin_publication::ComposedArtifact::Site(assembly) => (
                assembly.spec().id.0.clone(),
                assembly.routes().len(),
                assembly.catalog().len(),
                assembly.composition_fingerprint().to_owned(),
            ),
        };
        return Ok(Report::Publication {
            kind: "publication",
            schema_version: 2,
            publication,
            placements,
            sources,
            composition_fingerprint,
            diagnostics,
            authoring: Some(authoring_report(project, composed.catalog())?),
        });
    }
    bail!("publication branch did not produce composed content")
}

fn authoring_report(
    project: &Project,
    catalog: &berlin_publication::PublicationCatalog,
) -> Result<AuthoringReport, Error> {
    let mut report = AuthoringReport::analyze_documents(catalog.local_documents())?;
    report.excluded_drafts = catalog.excluded_drafts();
    super::origins::enrich_documents(project, catalog.local_documents(), &mut report);
    Ok(report)
}

fn publication_documents(plan: &PipelinePlan) -> Result<&NodeId, Error> {
    let outputs: Vec<_> = plan
        .nodes()
        .iter()
        .filter(|node| {
            matches!(
                node.operation,
                Operation::RenderHtml { .. } | Operation::RenderText { .. }
            )
        })
        .collect();
    let [website] = outputs.as_slice() else {
        bail!(
            "check requires exactly one publication output; found {}. Select a publication pipeline with --pipeline (export Org separately).",
            outputs.len()
        );
    };
    Ok(&website.id)
}

fn document_branch(plan: &PipelinePlan, root: &NodeId) -> Result<HashSet<NodeId>, Error> {
    let nodes: HashMap<_, _> = plan.nodes().iter().map(|node| (&node.id, node)).collect();
    let mut pending = vec![root];
    let mut required = HashSet::new();
    while let Some(id) = pending.pop() {
        if !required.insert(id.clone()) {
            continue;
        }
        let node = nodes
            .get(id)
            .context("Document preparation node is missing")?;
        match node.operation {
            Operation::LoadMarkdown { .. }
            | Operation::LoadAssets { .. }
            | Operation::LoadData { .. }
            | Operation::ParseDocuments
            | Operation::ParseResources { .. }
            | Operation::MergeDocuments
            | Operation::ParseMarkdown
            | Operation::MapDocuments { .. } => {}
            Operation::AssembleCatalog { .. }
            | Operation::Compose { .. }
            | Operation::RenderHtml { .. }
            | Operation::RenderText { .. } => {}
            _ => bail!(
                "check cannot execute document input '{}' ({:?}); it only reads Markdown/semantic documents, merges collections and applies mappings. Export Org or import papers explicitly first.",
                node.id,
                node.operation
            ),
        }
        pending.extend(&node.dependencies);
    }
    Ok(required)
}
