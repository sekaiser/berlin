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
    },
}
impl Report {
    pub fn has_errors(&self) -> bool {
        match self {
            Self::Authoring(report) => report.has_errors(),
            Self::Publication { .. } => false,
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
                Operation::RenderPublicationWebsite { .. }
                    | Operation::RenderPublicationText { .. }
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
                Operation::RenderPublicationWebsite { .. }
                    | Operation::RenderPublicationText { .. }
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
    }
    if let Some(RuntimeArtifact::PublicationAssembly(assembly)) = artifacts
        .values()
        .find(|artifact| matches!(artifact, RuntimeArtifact::PublicationAssembly(_)))
    {
        return Ok(Report::Publication {
            kind: "publication",
            schema_version: 1,
            publication: assembly.spec().metadata.id.0.clone(),
            placements: assembly.placements().len(),
            sources: assembly.sources().len(),
            composition_fingerprint: assembly.composition_fingerprint().into(),
            diagnostics,
        });
    }
    let Some(RuntimeArtifact::Documents(documents)) = artifacts.get(document_root) else {
        bail!("Website input '{document_root}' did not produce documents");
    };
    let mut report = AuthoringReport::analyze(documents)?;
    super::origins::enrich(project, documents, &mut report);
    Ok(Report::Authoring(report))
}

fn publication_documents(plan: &PipelinePlan) -> Result<&NodeId, Error> {
    let outputs: Vec<_> = plan
        .nodes()
        .iter()
        .filter(|node| {
            matches!(
                node.operation,
                Operation::RenderWebsite { .. }
                    | Operation::RenderPublicationWebsite { .. }
                    | Operation::RenderPublicationText { .. }
            )
        })
        .collect();
    let [website] = outputs.as_slice() else {
        bail!(
            "check requires exactly one website output; found {}. Select a website pipeline with --pipeline (export Org separately).",
            outputs.len()
        );
    };
    if !matches!(website.operation, Operation::RenderWebsite { .. }) {
        return Ok(&website.id);
    }
    let assembly_id = website
        .dependencies
        .first()
        .context("Website has no assembly input")?;
    let assembly = plan
        .nodes()
        .iter()
        .find(|node| &node.id == assembly_id)
        .context("Website assembly node is missing")?;
    assembly
        .dependencies
        .first()
        .context("Website assembly has no documents input")
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
            | Operation::LoadData { .. }
            | Operation::ParseDocuments
            | Operation::MergeDocuments
            | Operation::ParseMarkdown
            | Operation::MapDocuments { .. } => {}
            Operation::ComposePublication { .. }
            | Operation::RenderPublicationWebsite { .. }
            | Operation::RenderPublicationText { .. } => {}
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
