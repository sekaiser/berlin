//! Prepares only the document branch of one website output; no publishing effects.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Error, bail};
use berlin_content::AuthoringReport;
use berlin_core::{NodeId, Operation, PipelineLoader, PipelinePlan};

use super::{ExecutionOptions, RuntimeArtifact, executor::NodeExecutor};
use crate::project::Project;

pub(crate) fn inspect(project: &Project, pipeline: &str) -> Result<AuthoringReport, Error> {
    let program = crate::pipeline::load_pipeline_program(project.root())?;
    let plan = program
        .load(pipeline)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    plan.validate()?;
    let document_root = publication_documents(&plan)?;
    let required = document_branch(&plan, document_root)?;
    let mut artifacts = HashMap::new();
    let mut diagnostics = Vec::new();

    // The complete branch has been checked against a read-only allowlist before
    // executing any node. In particular, dry-run Org export is not a substitute
    // for the Markdown that would actually be published.
    for node in plan
        .topological_order()?
        .into_iter()
        .filter(|node| required.contains(&node.id))
    {
        let artifact = NodeExecutor {
            project,
            artifacts: &artifacts,
            program: &program,
            options: ExecutionOptions { dry_run: true },
            output_root: project.root(),
            diagnostics: &mut diagnostics,
        }
        .execute(node)?
        .context("Document preparation did not produce an artifact")?;
        if let RuntimeArtifact::MarkdownSources(sources) = &artifact
            && sources.is_empty()
        {
            bail!(
                "Markdown source node '{}' matched no files; export Org first if this pipeline reads generated Markdown",
                node.id
            );
        }
        artifacts.insert(node.id.clone(), artifact);
    }
    let Some(RuntimeArtifact::Documents(documents)) = artifacts.get(document_root) else {
        bail!("Website input '{document_root}' did not produce documents");
    };
    let mut report = AuthoringReport::analyze(documents)?;
    super::origins::enrich(project.root(), documents, &mut report);
    Ok(report)
}

fn publication_documents(plan: &PipelinePlan) -> Result<&NodeId, Error> {
    let outputs: Vec<_> = plan
        .nodes()
        .iter()
        .filter(|node| matches!(node.operation, Operation::RenderWebsite { .. }))
        .collect();
    let [website] = outputs.as_slice() else {
        bail!(
            "check requires exactly one website output; found {}. Select a website pipeline with --pipeline (export Org separately).",
            outputs.len()
        );
    };
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
            | Operation::ParseMarkdown
            | Operation::MapDocuments { .. } => {}
            _ => bail!(
                "check cannot execute document input '{}' ({:?}); it only loads Markdown, parses documents and applies mappings. Export Org explicitly first and check a pipeline that loads that Markdown.",
                node.id,
                node.operation
            ),
        }
        pending.extend(&node.dependencies);
    }
    Ok(required)
}
