use crate::project::Project;
use std::fs;

use anyhow::Context;
use anyhow::Error;
use berlin_core::Operation;
use berlin_core::PipelineLoader as _;

use crate::args::PlanFlags;

pub fn load_pipeline_program(
    project: &Project,
) -> Result<berlin_pipeline_dsl::RhaiPipelineLoader, Error> {
    let mut source = String::new();
    let mut locations = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut line = 1;
    for path in project.pipeline_files() {
        let text = fs::read_to_string(path)
            .with_context(|| format!("Failed reading Rhai pipeline '{}'", path.display()))?;
        if !seen.insert(path.canonicalize()?) {
            anyhow::bail!("Pipeline file selected more than once: {}", path.display());
        }
        locations.push(format!("{} starts at line {line}", path.display()));
        line += text.bytes().filter(|byte| *byte == b'\n').count() + 1;
        source.push_str(&text);
        source.push('\n');
    }
    berlin_pipeline_dsl::RhaiPipelineLoader::new(&source)
        .map_err(|error| anyhow::anyhow!("{}\nProgram sources: {}", error, locations.join("; ")))
}

pub fn print_current_plan(
    flags: PlanFlags,
    pipeline_files: Vec<std::path::PathBuf>,
) -> Result<(), Error> {
    let project = Project::load(pipeline_files)?;
    let plan = load_pipeline_program(&project)?
        .load(&flags.pipeline)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    plan.validate()?;

    if flags.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }

    for node in plan.topological_order()? {
        let dependencies = if node.dependencies.is_empty() {
            "source".to_string()
        } else {
            node.dependencies
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let output = node
            .output_path
            .as_ref()
            .map(|path| format!(" -> {}", path.display()))
            .unwrap_or_default();

        println!(
            "{:<18} {:<24} <- {:<30} [{:?}]{}",
            node.id,
            operation_name(&node.operation),
            dependencies,
            node.operation.output_kind(),
            output,
        );
        if let Some(destination) = &node.deployment {
            println!(
                "  deployment (explicit release/publish only): {}",
                serde_json::to_string(destination)?
            );
        }
    }

    Ok(())
}

fn operation_name(operation: &Operation) -> String {
    match operation {
        Operation::LoadOrg { pattern } => format!("load org {pattern}"),
        Operation::LoadMarkdown { pattern } => format!("load markdown {pattern}"),
        Operation::LoadData { pattern } => format!("load data {pattern}"),
        Operation::LoadCss { pattern } => format!("load css {pattern}"),
        Operation::LoadAssets { pattern } => format!("load assets {pattern}"),
        Operation::ExportOrg { backend, section } => format!("export org ({backend}, {section})"),
        Operation::ParseMarkdown => "parse markdown".into(),
        Operation::ParseFeed => "parse feed".into(),
        Operation::MapDocuments { mapper } => format!("map documents ({})", mapper.as_str()),
        Operation::AssembleWebsite => "assemble website".into(),
        Operation::CompileCss => "compile css".into(),
        Operation::CopyAssets => "copy assets".into(),
        Operation::RenderWebsite { .. } => "render Website".into(),
        Operation::RenderLinkedIn => "render LinkedIn".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scripted_plan(name: &str) -> berlin_core::PipelinePlan {
        berlin_pipeline_dsl::RhaiPipelineLoader::new(include_str!(
            "../support/fixtures/publishing/berlin.pipeline.rhai"
        ))
        .unwrap()
        .load(name)
        .unwrap()
    }

    #[test]
    fn scripted_plans_are_valid() {
        for name in ["org", "site", "linkedin"] {
            assert_eq!(scripted_plan(name).validate(), Ok(()));
        }

        assert_eq!(
            scripted_plan("site")
                .nodes()
                .iter()
                .find(|node| node.id.as_str() == "website")
                .map(|node| node.operation.output_kind()),
            Some(berlin_core::ArtifactKind::Website)
        );
        assert_eq!(
            scripted_plan("linkedin")
                .nodes()
                .last()
                .map(|node| node.operation.output_kind()),
            Some(berlin_core::ArtifactKind::LinkedInDrafts)
        );
    }
}
