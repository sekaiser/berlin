//! Executes individual pipeline operations using the current runtime context.

use super::{
    ExecutionOptions, RuntimeArtifact, copy_static, css, feed, load_runtime_sources, org,
    pattern_base, receipt, website,
};
use crate::project::Project;
use crate::util::fs::load_files;
use anyhow::{Context, Error};
use berlin_content::{DocumentCollection, WebsiteAssembly};
use berlin_core::{NodeId, Operation, PipelineNode, WebsiteConfig};
use berlin_document_linkedin::LinkedInDrafts;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub(super) struct NodeExecutor<'a> {
    pub(super) project: &'a Project,
    pub(super) artifacts: &'a HashMap<NodeId, RuntimeArtifact>,
    pub(super) program: &'a berlin_pipeline_dsl::RhaiPipelineLoader,
    pub(super) options: ExecutionOptions,
    pub(super) output_root: &'a Path,
    pub(super) diagnostics: &'a mut Vec<receipt::Diagnostic>,
}

impl<'a> NodeExecutor<'a> {
    pub(super) fn execute(
        &mut self,
        node: &PipelineNode,
    ) -> Result<Option<RuntimeArtifact>, Error> {
        let expected = node.operation.signature().inputs.len();
        if node.dependencies.len() != expected {
            anyhow::bail!(
                "pipeline node '{}' expects {expected} inputs, found {}",
                node.id,
                node.dependencies.len()
            );
        }
        match &node.operation {
            Operation::LoadOrg { pattern } => Ok(Some(RuntimeArtifact::OrgSources(load_files(
                self.project,
                pattern,
            )?))),
            Operation::LoadMarkdown { pattern } => Ok(Some(RuntimeArtifact::MarkdownSources(
                load_runtime_sources(self.project, pattern)?,
            ))),
            Operation::LoadData { pattern } => Ok(Some(RuntimeArtifact::DataSources(
                load_runtime_sources(self.project, pattern)?,
            ))),
            Operation::LoadCss { pattern } => Ok(Some(RuntimeArtifact::CssSources(
                load_runtime_sources(self.project, pattern)?,
            ))),
            Operation::ParseMarkdown => self.parse_markdown(node),
            Operation::ParseFeed => self.parse_feed(node),
            Operation::MapDocuments { mapper } => self.map_documents(node, mapper),
            Operation::AssembleWebsite => self.assemble_website(node),
            Operation::RenderWebsite { config } => self.render_website(node, config),
            Operation::RenderLinkedIn => self.render_linkedin(node),
            Operation::CompileCss => self.compile_css(node),
            Operation::LoadAssets { pattern } => self.load_assets(pattern),
            Operation::CopyAssets => self.copy_assets(node),
            Operation::ExportOrg { backend, section } => self.export_org(node, backend, section),
        }
    }

    fn input<T>(
        &self,
        node: &PipelineNode,
        index: usize,
        extract: impl FnOnce(&'a RuntimeArtifact) -> Option<T>,
    ) -> Result<T, Error> {
        runtime_input(node, self.artifacts, index, extract)
    }

    fn parse_markdown(&self, node: &PipelineNode) -> Result<Option<RuntimeArtifact>, Error> {
        let sources = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::MarkdownSources(sources) => Some(sources),
            _ => None,
        })?;
        let parser = markdown::Parser::new();
        let documents = DocumentCollection::new(
            sources
                .iter()
                .map(|source| {
                    markdown::Source::new(&source.text, &source.uri)
                        .and_then(|source| parser.parse(&source))
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
        .validated()?;
        Ok(Some(RuntimeArtifact::Documents(documents)))
    }

    fn parse_feed(&self, node: &PipelineNode) -> Result<Option<RuntimeArtifact>, Error> {
        let sources = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::DataSources(sources) => Some(sources),
            _ => None,
        })?;
        Ok(Some(RuntimeArtifact::Feed(feed::parse(sources)?)))
    }

    fn map_documents(
        &self,
        node: &PipelineNode,
        mapper: &berlin_core::FunctionRef,
    ) -> Result<Option<RuntimeArtifact>, Error> {
        let documents = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::Documents(documents) => Some(documents),
            _ => None,
        })?;
        let documents = self
            .program
            .map_documents(mapper, documents)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        Ok(Some(RuntimeArtifact::Documents(documents)))
    }

    fn assemble_website(&self, node: &PipelineNode) -> Result<Option<RuntimeArtifact>, Error> {
        let documents = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::Documents(value) => Some(value),
            _ => None,
        })?;
        let feed = self.input(node, 1, |artifact| match artifact {
            RuntimeArtifact::Feed(value) => Some(value),
            _ => None,
        })?;
        let website = WebsiteAssembly::new(documents.clone(), feed.clone())?;
        Ok(Some(RuntimeArtifact::WebsiteAssembly(website)))
    }

    fn render_website(
        &self,
        node: &PipelineNode,
        config: &WebsiteConfig,
    ) -> Result<Option<RuntimeArtifact>, Error> {
        let website = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::WebsiteAssembly(value) => Some(value),
            _ => None,
        })?;
        let output = output_path(node)?;
        if self.options.dry_run {
            println!("would render website -> {}", output.display());
        } else {
            website::render(
                self.project,
                website,
                config,
                &self.output_root.join(output),
            )?;
        }
        Ok(None)
    }

    fn render_linkedin(&mut self, node: &PipelineNode) -> Result<Option<RuntimeArtifact>, Error> {
        let documents = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::Documents(documents) => Some(documents),
            _ => None,
        })?;
        let renderer = berlin_document_linkedin::Renderer::default();
        let drafts = renderer.render_collection(documents);
        self.diagnostics.extend(linkedin_diagnostics(node, &drafts));
        let output = output_path(node)?;
        if self.options.dry_run {
            println!("would render linkedin -> {}", output.display());
        } else {
            materialize_linkedin_drafts(&self.output_root.join(output), &drafts)?;
        }
        Ok(None)
    }

    fn compile_css(&self, node: &PipelineNode) -> Result<Option<RuntimeArtifact>, Error> {
        let sources = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::CssSources(sources) => Some(sources),
            _ => None,
        })?;
        let output = output_path(node)?;
        let [source] = sources.as_slice() else {
            anyhow::bail!(
                "CompileCss expects exactly one root stylesheet, found {}",
                sources.len()
            );
        };
        if self.options.dry_run {
            println!("would compile css -> {}", output.display());
        } else {
            let compiled = css::compile(&source.path)?;
            write_output(&self.output_root.join(output), &compiled)?;
        }
        Ok(None)
    }

    fn load_assets(&self, pattern: &str) -> Result<Option<RuntimeArtifact>, Error> {
        let (source_root, relative) = crate::util::fs::source_location(self.project, pattern)?;
        let source_root = pattern_base(
            &source_root,
            relative.to_str().context("Asset pattern is not UTF-8")?,
        );
        let source_root = if pattern.starts_with('@') {
            source_root.canonicalize()?
        } else {
            source_root
        };
        Ok(Some(RuntimeArtifact::StaticSources {
            source_root,
            files: load_files(self.project, pattern)?
                .into_iter()
                .filter(|path| {
                    path.is_file()
                        && path.file_name().and_then(|name| name.to_str()) != Some(".DS_Store")
                })
                .collect(),
        }))
    }

    fn copy_assets(&self, node: &PipelineNode) -> Result<Option<RuntimeArtifact>, Error> {
        let (source_root, files) = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::StaticSources { source_root, files } => Some((source_root, files)),
            _ => None,
        })?;
        let output = output_path(node)?;
        if self.options.dry_run {
            println!("would copy assets -> {}", output.display());
        } else {
            copy_static::copy(files, source_root, &self.output_root.join(output))?;
        }
        Ok(None)
    }

    fn export_org(
        &self,
        node: &PipelineNode,
        backend: &str,
        section: &str,
    ) -> Result<Option<RuntimeArtifact>, Error> {
        let sources = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::OrgSources(sources) => Some(sources),
            _ => None,
        })?;
        let output = output_path(node)?;
        Ok(Some(RuntimeArtifact::MarkdownSources(org::export(
            self.project,
            self.output_root,
            sources,
            backend,
            output,
            section,
            self.options.dry_run,
        )?)))
    }
}

fn materialize_linkedin_drafts(output: &Path, drafts: &LinkedInDrafts) -> Result<(), Error> {
    std::fs::create_dir_all(output)
        .with_context(|| format!("Unable to create {}", output.display()))?;
    let mut filenames = HashSet::with_capacity(drafts.len());
    for draft in drafts.as_slice() {
        let filename = format!("{}.txt", draft.artifact_name);
        if !filenames.insert(filename.clone()) {
            anyhow::bail!(
                "LinkedIn draft IDs collide at output filename '{}'",
                filename
            );
        }
        let path = output.join(filename);
        std::fs::write(&path, format!("{}\n", draft.text))
            .with_context(|| format!("Unable to write {}", path.display()))?;
    }
    let manifest_path = output.join("manifest.json");
    let manifest = serde_json::to_string_pretty(drafts)?;
    std::fs::write(&manifest_path, format!("{manifest}\n"))
        .with_context(|| format!("Unable to write {}", manifest_path.display()))?;
    Ok(())
}

fn write_output(output: &Path, contents: &str) -> Result<(), Error> {
    let parent = output
        .parent()
        .with_context(|| format!("Output has no parent: {}", output.display()))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("Unable to create output directory {}", parent.display()))?;
    std::fs::write(output, contents)
        .with_context(|| format!("Unable to write {}", output.display()))
}

fn output_path(node: &PipelineNode) -> Result<&Path, Error> {
    node.output_path
        .as_deref()
        .with_context(|| format!("pipeline node '{}' has no output path", node.id))
}

fn runtime_input<'a, T>(
    node: &PipelineNode,
    artifacts: &'a HashMap<NodeId, RuntimeArtifact>,
    index: usize,
    extract: impl FnOnce(&'a RuntimeArtifact) -> Option<T>,
) -> Result<T, Error> {
    let expected = node
        .operation
        .signature()
        .inputs
        .get(index)
        .with_context(|| format!("pipeline node '{}' has no declared input {index}", node.id))?;
    let dependency = node.dependencies.get(index).with_context(|| {
        format!(
            "pipeline node '{}' is missing input {index} ({expected:?})",
            node.id
        )
    })?;
    let artifact = artifacts.get(dependency).with_context(|| {
        format!(
            "pipeline node '{}' input {index}: dependency '{dependency}' has no runtime value",
            node.id
        )
    })?;
    let actual = artifact.kind();
    if actual != *expected {
        anyhow::bail!(
            "pipeline node '{}' input {index} from '{dependency}' has kind {actual:?}, expected {expected:?}",
            node.id
        );
    }
    extract(artifact).with_context(|| {
        format!(
            "pipeline node '{}' input {index}: executor cannot consume {actual:?}",
            node.id
        )
    })
}

fn linkedin_diagnostics(node: &PipelineNode, drafts: &LinkedInDrafts) -> Vec<receipt::Diagnostic> {
    let mut diagnostics = Vec::new();
    for draft in drafts.as_slice() {
        for diagnostic in &draft.diagnostics {
            match diagnostic {
                berlin_document_linkedin::LinkedInDraftDiagnostic::CharacterLimitExceeded {
                    limit,
                    actual,
                } => diagnostics.push(receipt::Diagnostic {
                    node: node.id.as_str().into(),
                    content_id: draft.source.0.0.clone(),
                    code: "character_limit_exceeded".into(),
                    message: format!(
                        "LinkedIn feed post has {actual} characters; limit is {limit}"
                    ),
                }),
            }
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use berlin_content::Feed;

    fn document_input<'a>(
        node: &PipelineNode,
        artifacts: &'a HashMap<NodeId, RuntimeArtifact>,
    ) -> Result<&'a DocumentCollection, Error> {
        runtime_input(node, artifacts, 0, |artifact| match artifact {
            RuntimeArtifact::Documents(documents) => Some(documents),
            _ => None,
        })
    }

    #[test]
    fn inputs_follow_signature_positions_instead_of_searching_by_kind() {
        let artifacts = HashMap::from([
            (
                NodeId::from("documents"),
                RuntimeArtifact::Documents(DocumentCollection::default()),
            ),
            (NodeId::from("feed"), RuntimeArtifact::Feed(Feed::default())),
        ]);
        let valid = PipelineNode::new("assembly", Operation::AssembleWebsite)
            .depends_on("documents")
            .depends_on("feed");
        assert!(document_input(&valid, &artifacts).is_ok());
        assert!(
            runtime_input(&valid, &artifacts, 1, |artifact| match artifact {
                RuntimeArtifact::Feed(feed) => Some(feed),
                _ => None,
            })
            .is_ok()
        );

        let reversed = PipelineNode::new("assembly", Operation::AssembleWebsite)
            .depends_on("feed")
            .depends_on("documents");
        let error = document_input(&reversed, &artifacts)
            .unwrap_err()
            .to_string();
        assert!(error.contains("assembly"));
        assert!(error.contains("input 0"));
        assert!(error.contains("expected Documents"));
    }

    #[test]
    fn reports_missing_dependencies_and_runtime_values() {
        let artifacts = HashMap::new();
        let node = PipelineNode::new("assembly", Operation::AssembleWebsite);
        assert!(
            document_input(&node, &artifacts)
                .unwrap_err()
                .to_string()
                .contains("missing input 0")
        );
        let node = node.depends_on("documents");
        let error = document_input(&node, &artifacts).unwrap_err().to_string();
        assert!(error.contains("documents"));
        assert!(error.contains("no runtime value"));
    }
}
