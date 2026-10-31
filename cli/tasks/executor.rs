//! Executes individual pipeline operations using the current runtime context.

use super::{
    ExecutionOptions, RuntimeArtifact, copy_static, css, load_runtime_sources, org, pattern_base,
    receipt,
};
use crate::project::Project;
use crate::util::fs::load_files;
use anyhow::{Context, Error};
use berlin_content::DocumentCollection;
use berlin_core::{NodeId, Operation, PipelineNode};
use std::collections::HashMap;
use std::path::Path;

pub(super) struct NodeExecutor<'a> {
    pub(super) consumed_inputs: &'a mut Vec<receipt::ConsumedInput>,
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
            Operation::AssembleCatalog { options, .. } => {
                let mut documents = Vec::new();
                let mut resources = Vec::new();
                for dependency in &node.dependencies {
                    match self
                        .artifacts
                        .get(dependency)
                        .context("missing catalog input")?
                    {
                        RuntimeArtifact::Documents(batch) => documents.push(batch.clone()),
                        RuntimeArtifact::ExternalResources(batch) => resources.push(batch.clone()),
                        _ => anyhow::bail!("catalog accepts only typed document/resource batches"),
                    }
                }
                let resources = berlin_publication::ExternalResourceCollection::merge(
                    &resources,
                    &options.resource_limits,
                    &options.limits,
                )?;
                let catalog = berlin_publication::PublicationCatalog::with_document_batches(
                    documents,
                    resources,
                    options.as_ref().clone(),
                    Default::default(),
                )?;
                Ok(Some(RuntimeArtifact::Catalog(catalog)))
            }
            Operation::Compose { composer } => {
                let catalog = self.input(node, 0, |a| match a {
                    RuntimeArtifact::Catalog(c) => Some(c),
                    _ => None,
                })?;
                let composed = self
                    .program
                    .compose_artifact(composer, catalog.clone())
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                Ok(Some(RuntimeArtifact::Composed(composed)))
            }
            Operation::RenderHtml {
                config,
                asset_mounts,
            } => {
                let composed = self.input(node, 0, |a| match a {
                    RuntimeArtifact::Composed(c) => Some(c),
                    _ => None,
                })?;
                let mut assets = Vec::new();
                let mut bytes = 0usize;
                for (index, mount) in asset_mounts.iter().enumerate() {
                    let (root, snapshot) = self.input(node, index + 1, |a| match a {
                        RuntimeArtifact::StaticSources {
                            source_root,
                            snapshot,
                            ..
                        } => Some((source_root, snapshot)),
                        _ => None,
                    })?;
                    for input in snapshot.iter() {
                        bytes = bytes
                            .checked_add(input.bytes.len())
                            .context("asset byte count overflow")?;
                        if bytes > composed.catalog().limits().fragment_bytes {
                            anyhow::bail!("HTML asset snapshot byte limit exceeded");
                        }
                        let path = root.join(&input.relative);
                        self.consumed_inputs.push(receipt::ConsumedInput::new(
                            path,
                            format!("assets/{mount}/{}", input.relative.to_string_lossy()),
                            &input.bytes,
                        ));
                        assets.push((Path::new(mount).join(&input.relative), input.bytes.clone()));
                    }
                }
                match composed {
                    berlin_publication::ComposedArtifact::Single(assembly) => {
                        super::composed::website(
                            self.project,
                            assembly,
                            config,
                            &self.output_root.join(output_path(node)?),
                            self.options,
                            self.consumed_inputs,
                            &assets,
                        )?
                    }
                    berlin_publication::ComposedArtifact::Site(assembly) => super::composed::site(
                        self.project,
                        assembly,
                        config,
                        &self.output_root.join(output_path(node)?),
                        self.options,
                        self.consumed_inputs,
                        &assets,
                    )?,
                }
                Ok(None)
            }
            Operation::RenderText { config } => {
                let composed = self.input(node, 0, |a| match a {
                    RuntimeArtifact::Composed(c) => Some(c),
                    _ => None,
                })?;
                let warnings = super::composed::text(
                    composed.single()?,
                    config,
                    &self.output_root.join(output_path(node)?),
                    self.options.dry_run,
                    self.consumed_inputs,
                )?;
                for warning in warnings {
                    let berlin_document_linkedin::LinkedInDraftDiagnostic::CharacterLimitExceeded {
                        limit,
                        actual,
                    } = warning;
                    self.diagnostics.push(receipt::Diagnostic{node:node.id.to_string(),content_id:String::new(),code:"character_limit_exceeded".into(),message:format!("LinkedIn draft has {actual} characters; limit is {limit}. Full draft retained.")});
                }
                Ok(None)
            }
            Operation::LoadLatex { pattern } => Ok(Some(RuntimeArtifact::LatexSources(
                super::paper::load(self.project, pattern)?,
            ))),
            Operation::CompilePapers { url_prefix } => self.compile_papers(node, url_prefix),
            Operation::CompileDiagrams { engine } => self.compile_diagrams(node, *engine),
            Operation::LoadOrg { pattern } => Ok(Some(RuntimeArtifact::OrgSources(
                load_runtime_sources(self.project, pattern)?,
            ))),
            Operation::LoadMarkdown { pattern } => Ok(Some(RuntimeArtifact::MarkdownSources(
                load_runtime_sources(self.project, pattern)?,
            ))),
            Operation::LoadData { pattern } => Ok(Some(RuntimeArtifact::DataSources(
                super::load_runtime_data(self.project, pattern)?,
            ))),
            Operation::LoadCss { pattern } => Ok(Some(RuntimeArtifact::CssSources(
                load_runtime_sources(self.project, pattern)?,
            ))),
            Operation::ParseMarkdown => self.parse_markdown(node),
            Operation::ParseDocuments => self.parse_documents(node),
            Operation::MergeDocuments => self.merge_documents(node),
            Operation::ParseResources { namespace } => self.parse_resources(node, namespace),
            Operation::MapDocuments { mapper } => self.map_documents(node, mapper),
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
        Ok(Some(RuntimeArtifact::Documents(documents.into())))
    }

    fn parse_resources(
        &self,
        node: &PipelineNode,
        namespace: &str,
    ) -> Result<Option<RuntimeArtifact>, Error> {
        let sources = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::DataSources(sources) => Some(sources),
            _ => None,
        })?;
        let limits = berlin_publication::ResourceLimits::default();
        let inputs = sources
            .iter()
            .map(|source| {
                let logical = crate::util::fs::source_reference(self.project, &source.path)?;
                let logical = logical
                    .to_str()
                    .context("resource input logical name is not UTF-8")?
                    .trim_start_matches('@');
                super::resources::ResourceInput::capture(
                    namespace,
                    logical,
                    source.text.as_bytes(),
                    &limits,
                )
                .map_err(Error::from)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(Some(RuntimeArtifact::ExternalResources(
            super::resources::parse(
                &inputs,
                &limits,
                &berlin_publication::PublicationLimits::default(),
            )?,
        )))
    }

    fn compile_diagrams(
        &self,
        node: &PipelineNode,
        engine: berlin_core::TexEngine,
    ) -> Result<Option<RuntimeArtifact>, Error> {
        let sources = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::DataSources(sources) => Some(sources),
            _ => None,
        })?;
        let inputs = self.input(node, 1, |artifact| match artifact {
            RuntimeArtifact::StaticSources { snapshot, .. } => Some(snapshot),
            _ => None,
        })?;
        let output = output_path(node)?;
        let names = super::graphics::output_names(sources)?;
        let mut generated = Vec::new();
        if self.options.dry_run {
            println!(
                "would compile {} diagram(s) -> {}",
                sources.len(),
                output.display()
            );
        } else {
            if !self.options.allow_tex {
                anyhow::bail!("diagram compilation requires --allow-tex");
            }
            let output = self.output_root.join(output);
            std::fs::create_dir_all(&output)?;
            for (source, name) in sources.iter().zip(names) {
                let svg = super::graphics::render(
                    &source.text,
                    inputs,
                    engine,
                    self.project.root(),
                    self.options.no_cache,
                )?;
                std::fs::write(output.join(&name), &svg)?;
                generated.push(super::graphics::Input {
                    relative: name.into(),
                    bytes: svg,
                });
            }
        }
        Ok(Some(RuntimeArtifact::StaticSources {
            source_root: self.project.root().join(output),
            snapshot: std::sync::Arc::new(generated),
        }))
    }

    fn compile_papers(
        &self,
        node: &PipelineNode,
        url_prefix: &str,
    ) -> Result<Option<RuntimeArtifact>, Error> {
        let sources = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::LatexSources(sources) => Some(sources),
            _ => None,
        })?;
        let output = output_path(node)?;
        let documents = if self.options.dry_run {
            println!(
                "would compile {} LaTeX paper(s) -> {}",
                sources.len(),
                output.display()
            );
            DocumentCollection::default()
        } else {
            if !self.options.allow_tex {
                anyhow::bail!("LaTeX compilation requires --allow-tex");
            }
            super::paper::compile(
                self.project.root(),
                sources,
                &self.output_root.join(output),
                url_prefix,
                self.options.no_cache,
            )?
        };
        Ok(Some(RuntimeArtifact::Documents(documents.into())))
    }

    fn parse_documents(&self, node: &PipelineNode) -> Result<Option<RuntimeArtifact>, Error> {
        let sources = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::DataSources(sources) => Some(sources),
            _ => None,
        })?;
        if sources.is_empty() {
            anyhow::bail!("ParseDocuments matched no semantic document files");
        }
        let documents = sources
            .iter()
            .map(|source| {
                serde_json::from_str::<berlin_document::Document>(&source.text)
                    .with_context(|| format!("invalid semantic document in {}", source.uri))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(RuntimeArtifact::Documents(
            DocumentCollection::new(documents).validated()?.into(),
        )))
    }

    fn merge_documents(&self, node: &PipelineNode) -> Result<Option<RuntimeArtifact>, Error> {
        let documents = |index| {
            self.input(node, index, |artifact| match artifact {
                RuntimeArtifact::Documents(documents) => Some(documents),
                _ => None,
            })
        };
        let merged = documents(0)?
            .as_slice()
            .iter()
            .chain(documents(1)?.as_slice())
            .cloned()
            .collect::<Vec<_>>();
        Ok(Some(RuntimeArtifact::Documents(
            DocumentCollection::new(merged).validated()?.into(),
        )))
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
        Ok(Some(RuntimeArtifact::Documents(documents.into())))
    }

    fn compile_css(&mut self, node: &PipelineNode) -> Result<Option<RuntimeArtifact>, Error> {
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
            let source_root = crate::util::fs::source_reference(self.project, &source.path)
                .and_then(|reference| {
                    crate::util::fs::source_location(
                        self.project,
                        reference
                            .to_str()
                            .context("CSS source reference is not UTF-8")?,
                    )
                    .map(|(root, _)| root)
                })
                .ok();
            let cache = (!self.options.no_cache && source_root.is_some())
                .then(|| super::cache::ArtifactCache::new(self.project.root()));
            let (compiled, inputs) = css::compile_loaded(
                source,
                source_root.as_deref().unwrap_or(self.project.root()),
                cache.as_ref(),
            )?;
            self.consumed_inputs.extend(inputs);
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
        let files = load_files(self.project, pattern)?
            .into_iter()
            .filter(|path| {
                path.is_file()
                    && path.file_name().and_then(|name| name.to_str()) != Some(".DS_Store")
            })
            .collect::<Vec<_>>();
        let snapshot = std::sync::Arc::new(super::graphics::snapshot_files(&source_root, &files)?);
        Ok(Some(RuntimeArtifact::StaticSources {
            source_root,
            snapshot,
        }))
    }

    fn copy_assets(&self, node: &PipelineNode) -> Result<Option<RuntimeArtifact>, Error> {
        let snapshot = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::StaticSources { snapshot, .. } => Some(snapshot),
            _ => None,
        })?;
        let output = output_path(node)?;
        if self.options.dry_run {
            println!("would copy assets -> {}", output.display());
        } else {
            copy_static::copy(snapshot, &self.output_root.join(output))?;
        }
        Ok(None)
    }

    fn export_org(
        &mut self,
        node: &PipelineNode,
        backend: &str,
        section: &str,
    ) -> Result<Option<RuntimeArtifact>, Error> {
        let sources = self.input(node, 0, |artifact| match artifact {
            RuntimeArtifact::OrgSources(sources) => Some(sources),
            _ => None,
        })?;
        let paths = sources
            .iter()
            .map(|source| source.path.clone())
            .collect::<Vec<_>>();
        let output = output_path(node)?;
        Ok(Some(RuntimeArtifact::MarkdownSources(org::export(
            self.project,
            self.output_root,
            &paths,
            output,
            org::ExportOptions {
                backend,
                section,
                dry_run: self.options.dry_run,
                captured: Some(sources),
            },
            self.consumed_inputs,
        )?)))
    }
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

pub(super) fn runtime_input<'a, T>(
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
        .copied()
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
    if actual != expected {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_load_freezes_bytes_before_later_render_or_copy() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("assets")).unwrap();
        let source = root.path().join("assets/diagram.svg");
        std::fs::write(&source, "captured artwork").unwrap();
        let project = Project::new(root.path().to_owned(), vec![]);
        let program = berlin_pipeline_dsl::RhaiPipelineLoader::new(
            "pipeline assets {output copy_assets(load_assets(\"assets/*\"),\"public\");}",
        )
        .unwrap();
        let mut consumed_inputs = vec![];
        let mut diagnostics = vec![];
        let mut artifacts = HashMap::new();
        let captured = NodeExecutor {
            consumed_inputs: &mut consumed_inputs,
            diagnostics: &mut diagnostics,
            project: &project,
            program: &program,
            artifacts: &artifacts,
            options: ExecutionOptions::default(),
            output_root: root.path(),
        }
        .load_assets("assets/*")
        .unwrap()
        .unwrap();
        std::fs::write(&source, "changed artwork").unwrap();
        artifacts.insert(NodeId::from("assets"), captured);
        let node = PipelineNode::new("copy", Operation::CopyAssets)
            .depends_on("assets")
            .output_to("public");
        NodeExecutor {
            consumed_inputs: &mut consumed_inputs,
            diagnostics: &mut diagnostics,
            project: &project,
            program: &program,
            artifacts: &artifacts,
            options: ExecutionOptions::default(),
            output_root: root.path(),
        }
        .execute(&node)
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.path().join("public/diagram.svg")).unwrap(),
            "captured artwork"
        );
    }

    #[test]
    fn standalone_css_uses_loaded_entry_and_records_its_consumed_imports() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("styles")).unwrap();
        let entry = root.path().join("styles/main.css");
        let imported = root.path().join("styles/colors.css");
        let original = "@import './colors.css'; .captured-entry { color: red; }";
        let color = ".imported-entry { color: blue; }";
        std::fs::write(&entry, original).unwrap();
        std::fs::write(&imported, color).unwrap();
        let project = Project::new(root.path().to_owned(), vec![]);
        let program = berlin_pipeline_dsl::RhaiPipelineLoader::new("").unwrap();
        let mut artifacts = HashMap::new();
        let mut consumed_inputs = vec![];
        let mut diagnostics = vec![];
        let loaded = NodeExecutor {
            project: &project,
            program: &program,
            artifacts: &artifacts,
            consumed_inputs: &mut consumed_inputs,
            diagnostics: &mut diagnostics,
            output_root: root.path(),
            options: ExecutionOptions::default(),
        }
        .execute(&PipelineNode::new(
            "css",
            Operation::LoadCss {
                pattern: "styles/main.css".into(),
            },
        ))
        .unwrap()
        .unwrap();
        artifacts.insert(NodeId::from("css"), loaded);
        std::fs::write(&entry, ".late-entry { color: green; }").unwrap();
        let compile = PipelineNode::new("compile", Operation::CompileCss)
            .depends_on("css")
            .output_to("public/styles.css");
        for _ in 0..2 {
            consumed_inputs.clear();
            NodeExecutor {
                project: &project,
                program: &program,
                artifacts: &artifacts,
                consumed_inputs: &mut consumed_inputs,
                diagnostics: &mut diagnostics,
                output_root: root.path(),
                options: ExecutionOptions::default(),
            }
            .execute(&compile)
            .unwrap();
            let output = std::fs::read_to_string(root.path().join("public/styles.css")).unwrap();
            assert!(output.contains("captured-entry"), "{output}");
            assert!(!output.contains("late-entry"));
            assert!(output.contains("imported-entry"));
            let input = consumed_inputs
                .iter()
                .find(|input| {
                    input.path.canonicalize().unwrap() == imported.canonicalize().unwrap()
                })
                .expect("CSS import must be attributed");
            assert_eq!(
                input.sha256,
                berlin_publication::input_digest(color.as_bytes())
            );
        }
    }

    fn document_input<'a>(
        node: &PipelineNode,
        artifacts: &'a HashMap<NodeId, RuntimeArtifact>,
    ) -> Result<&'a DocumentCollection, Error> {
        runtime_input(node, artifacts, 0, |artifact| match artifact {
            RuntimeArtifact::Documents(documents) => Some(documents.as_ref()),
            _ => None,
        })
    }

    #[test]
    fn inputs_follow_signature_positions_instead_of_searching_by_kind() {
        let artifacts = HashMap::from([
            (
                NodeId::from("documents"),
                RuntimeArtifact::Documents(DocumentCollection::default().into()),
            ),
            (
                NodeId::from("resources"),
                RuntimeArtifact::ExternalResources(Default::default()),
            ),
        ]);
        let valid = PipelineNode::new(
            "assembly",
            Operation::AssembleCatalog {
                parts: vec![
                    berlin_core::ArtifactKind::Documents,
                    berlin_core::ArtifactKind::ExternalResources,
                ],
                options: Box::default(),
            },
        )
        .depends_on("documents")
        .depends_on("resources");
        assert!(document_input(&valid, &artifacts).is_ok());
        assert!(
            runtime_input(&valid, &artifacts, 1, |artifact| match artifact {
                RuntimeArtifact::ExternalResources(resources) => Some(resources),
                _ => None,
            })
            .is_ok()
        );

        let reversed = PipelineNode::new(
            "assembly",
            Operation::AssembleCatalog {
                parts: vec![
                    berlin_core::ArtifactKind::Documents,
                    berlin_core::ArtifactKind::ExternalResources,
                ],
                options: Box::default(),
            },
        )
        .depends_on("resources")
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
        let node = PipelineNode::new(
            "assembly",
            Operation::AssembleCatalog {
                parts: vec![
                    berlin_core::ArtifactKind::Documents,
                    berlin_core::ArtifactKind::ExternalResources,
                ],
                options: Box::default(),
            },
        );
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
