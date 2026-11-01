use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::btree_map::Entry;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use anyhow::Error;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use sha2::Sha256;

use berlin_core::NodeId;
use berlin_core::Operation;
use berlin_core::PipelineNode;
use berlin_core::PipelinePlan;

use super::RuntimeArtifact;
use super::SourceFile;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Diagnostic {
    pub node: String,
    pub content_id: String,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BuildReceipt {
    pub schema_version: u8,
    pub berlin_version: String,
    pub pipeline: String,
    pub status: String,
    pub started_at_unix_seconds: u64,
    pub duration_ms: u64,
    pub inputs: Vec<InputReceipt>,
    pub outputs: Vec<FileReceipt>,
    pub diagnostics: Vec<Diagnostic>,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub catalogs: Vec<super::state::builds::CatalogSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InputReceipt {
    pub path: String,
    pub sha256: String,
    pub content_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileReceipt {
    pub path: String,
    pub sha256: String,
}

pub(super) struct ReceiptContext<'a> {
    pub consumed_inputs: &'a [ConsumedInput],
    pub project: &'a crate::project::Project,
    pub pipeline: &'a str,
    pub started_at: SystemTime,
    pub duration: Duration,
    pub plan: &'a PipelinePlan,
    pub artifacts: &'a HashMap<NodeId, RuntimeArtifact>,
    pub owned_roots: &'a [PathBuf],
    pub diagnostics: &'a [Diagnostic],
    pub error: Option<&'a str>,
}

pub(super) fn write(context: ReceiptContext<'_>) -> Result<(), Error> {
    let inputs = {
        let mut inputs = BTreeMap::new();
        for snapshot in context.consumed_inputs {
            let uri = url::Url::from_file_path(&snapshot.path)
                .map_err(|_| anyhow::anyhow!("invalid consumed input path"))?
                .to_string();
            inputs.insert(
                uri,
                InputReceipt {
                    path: display_path(context.project.root(), &snapshot.path),
                    sha256: snapshot.sha256.clone(),
                    content_id: None,
                },
            );
        }
        collect_source_inputs(
            &mut inputs,
            context.project.root(),
            context.plan,
            context.artifacts,
        )?;
        enrich_content_ids(&mut inputs, context.plan, context.artifacts)?;
        inputs.into_values().collect()
    };
    let receipt = BuildReceipt {
        schema_version: 1,
        berlin_version: crate::version::berlin(),
        pipeline: context.pipeline.into(),
        status: if context.error.is_some() {
            "failed"
        } else {
            "succeeded"
        }
        .into(),
        started_at_unix_seconds: context
            .started_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        duration_ms: context.duration.as_millis().try_into().unwrap_or(u64::MAX),
        inputs,
        outputs: if context.error.is_none() {
            output_receipts(context.project.root(), context.owned_roots)?
        } else {
            Vec::new()
        },
        diagnostics: context.diagnostics.to_vec(),
        error: context.error.map(str::to_owned),
        stage: Some("execution".into()),
        catalogs: context
            .plan
            .nodes()
            .iter()
            .filter_map(|node| match context.artifacts.get(&node.id) {
                Some(RuntimeArtifact::Catalog(catalog)) => Some(
                    super::state::builds::CatalogSnapshot::capture(node.id.to_string(), catalog),
                ),
                _ => None,
            })
            .collect(),
    };

    persist(context.project.root(), context.pipeline, &receipt)
}

#[derive(Clone, Debug)]
pub(crate) struct ConsumedInput {
    pub path: PathBuf,
    pub logical_name: String,
    pub sha256: String,
}
impl ConsumedInput {
    pub(crate) fn new(path: PathBuf, logical_name: String, bytes: &[u8]) -> Self {
        Self {
            path,
            logical_name,
            sha256: digest(bytes),
        }
    }
}

pub(super) fn write_setup_failure(
    project: &crate::project::Project,
    pipeline: &str,
    started_at: SystemTime,
    duration: Duration,
    error: &str,
) -> Result<(), Error> {
    let project_root = project.root();
    let mut inputs = BTreeMap::new();
    for pipeline_file in project
        .pipeline_files()
        .iter()
        .filter(|path| path.is_file())
    {
        insert_file_input(&mut inputs, project_root, pipeline_file, None)?;
    }
    let receipt = BuildReceipt {
        schema_version: 1,
        berlin_version: crate::version::berlin(),
        pipeline: pipeline.into(),
        status: "failed".into(),
        started_at_unix_seconds: started_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        duration_ms: duration.as_millis().try_into().unwrap_or(u64::MAX),
        inputs: inputs.into_values().collect(),
        outputs: Vec::new(),
        diagnostics: vec![],
        error: Some(error.into()),
        stage: Some("setup".into()),
        catalogs: vec![],
    };
    persist(project_root, pipeline, &receipt)
}

fn persist(project_root: &Path, pipeline: &str, receipt: &BuildReceipt) -> Result<(), Error> {
    if super::state::active(project_root)? {
        return super::state::Store::open(project_root)?.record_build(receipt);
    }
    let directory = project_root.join(".berlin/receipts");
    fs::create_dir_all(&directory)?;
    let destination = directory.join(receipt_filename(pipeline));
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
    serde_json::to_writer_pretty(&mut temporary, &receipt)?;
    temporary.write_all(b"\n")?;
    temporary
        .persist(&destination)
        .map_err(|error| error.error)
        .with_context(|| format!("Unable to write receipt {}", destination.display()))?;
    Ok(())
}

fn receipt_filename(pipeline: &str) -> String {
    if !pipeline.is_empty()
        && pipeline
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        format!("{pipeline}.json")
    } else {
        format!("invalid-{}.json", &digest(pipeline.as_bytes())[..16])
    }
}

fn collect_source_inputs(
    inputs: &mut BTreeMap<String, InputReceipt>,
    project_root: &Path,
    plan: &PipelinePlan,
    artifacts: &HashMap<NodeId, RuntimeArtifact>,
) -> Result<(), Error> {
    for artifact in plan
        .nodes()
        .iter()
        .filter(|node| is_source_node(node))
        .filter_map(|node| artifacts.get(&node.id))
    {
        collect_artifact_inputs(inputs, project_root, artifact)?;
    }
    Ok(())
}

// Derived artifacts (including exported Markdown) are not independent inputs.
fn is_source_node(node: &PipelineNode) -> bool {
    matches!(
        node.operation,
        Operation::LoadOrg { .. }
            | Operation::LoadLatex { .. }
            | Operation::LoadMarkdown { .. }
            | Operation::LoadData { .. }
            | Operation::LoadCss { .. }
            | Operation::LoadAssets { .. }
    )
}

fn collect_artifact_inputs(
    inputs: &mut BTreeMap<String, InputReceipt>,
    project_root: &Path,
    artifact: &RuntimeArtifact,
) -> Result<(), Error> {
    match artifact {
        RuntimeArtifact::LatexSources(sources) => {
            for source in sources {
                collect_text_inputs(inputs, project_root, std::slice::from_ref(&source.text));
                collect_text_inputs(inputs, project_root, &source.snapshot.dependencies);
                for resource in &source.snapshot.resources {
                    inputs.entry(resource.uri.clone()).or_insert(InputReceipt {
                        path: source_path(project_root, &resource.uri, &resource.path),
                        sha256: digest(&resource.bytes),
                        content_id: None,
                    });
                }
            }
        }
        RuntimeArtifact::OrgSources(sources) => {
            collect_text_inputs(inputs, project_root, sources);
            for source in sources {
                if let Some(input) = inputs.get_mut(&source.uri) {
                    input.content_id = org_content_id(&source.text);
                }
            }
        }
        RuntimeArtifact::MarkdownSources(sources)
        | RuntimeArtifact::DataSources(sources)
        | RuntimeArtifact::CssSources(sources) => {
            collect_text_inputs(inputs, project_root, sources)
        }
        RuntimeArtifact::StaticSources {
            source_root,
            snapshot,
            ..
        } => {
            for file in snapshot.iter() {
                let path = source_root.join(&file.relative);
                let uri = url::Url::from_file_path(&path)
                    .map_err(|_| anyhow::anyhow!("invalid asset input path"))?
                    .to_string();
                inputs.entry(uri.clone()).or_insert(InputReceipt {
                    path: source_path(project_root, &uri, &path),
                    sha256: digest(&file.bytes),
                    content_id: None,
                });
            }
        }
        RuntimeArtifact::Catalog(_)
        | RuntimeArtifact::Composed(_)
        | RuntimeArtifact::ExternalResources(_)
        | RuntimeArtifact::Documents(_) => {}
    }
    Ok(())
}

fn collect_text_inputs(
    inputs: &mut BTreeMap<String, InputReceipt>,
    project_root: &Path,
    sources: &[SourceFile],
) {
    for source in sources {
        let receipt = InputReceipt {
            path: source_path(project_root, &source.uri, &source.path),
            sha256: digest(source.text.as_bytes()),
            content_id: None,
        };
        match inputs.entry(source.uri.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(receipt);
            }
            Entry::Occupied(mut entry) => {
                // Loaded text is the consumed snapshot; retain any known identity.
                let input = entry.get_mut();
                input.path = receipt.path;
                input.sha256 = receipt.sha256;
            }
        }
    }
}

/// Enrich in plan order, rejecting ambiguous identities instead of choosing a
/// winner based on HashMap iteration order. Repeated identical IDs are harmless.
fn enrich_content_ids(
    inputs: &mut BTreeMap<String, InputReceipt>,
    plan: &PipelinePlan,
    artifacts: &HashMap<NodeId, RuntimeArtifact>,
) -> Result<(), Error> {
    for artifact in plan
        .nodes()
        .iter()
        .filter_map(|node| artifacts.get(&node.id))
    {
        if let RuntimeArtifact::Documents(documents) = artifact {
            for document in documents.as_slice() {
                if let Some(input) = inputs.get_mut(&document.provenance.source) {
                    merge_content_id(input, &document.id.0)?;
                }
            }
        }
    }
    Ok(())
}

fn merge_content_id(input: &mut InputReceipt, content_id: &str) -> Result<(), Error> {
    match input.content_id.as_deref() {
        Some(existing) if existing != content_id => anyhow::bail!(
            "Conflicting content IDs for input '{}': '{}' and '{}'",
            input.path,
            existing,
            content_id,
        ),
        Some(_) => {}
        None => input.content_id = Some(content_id.into()),
    }
    Ok(())
}

fn insert_file_input(
    inputs: &mut BTreeMap<String, InputReceipt>,
    project_root: &Path,
    path: &Path,
    content_id: Option<String>,
) -> Result<(), Error> {
    let uri = url::Url::from_file_path(path)
        .map_err(|_| anyhow::anyhow!("Invalid input path: {}", path.display()))?
        .to_string();
    inputs.entry(uri).or_insert(InputReceipt {
        path: display_path(project_root, path),
        sha256: digest(&fs::read(path)?),
        content_id,
    });
    Ok(())
}

fn output_receipts(project_root: &Path, roots: &[PathBuf]) -> Result<Vec<FileReceipt>, Error> {
    let mut files = Vec::new();
    for root in roots {
        collect_files(&project_root.join(root), &mut files)?;
    }
    files.sort();
    files
        .into_iter()
        .map(|path| {
            Ok(FileReceipt {
                path: display_path(project_root, &path),
                sha256: digest(&fs::read(path)?),
            })
        })
        .collect()
}

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), Error> {
    if path.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn display_path(project_root: &Path, path: &Path) -> String {
    let relative = path
        .strip_prefix(project_root)
        .ok()
        .map(Path::to_owned)
        .or_else(|| {
            // A theme is resolved canonically; the project may use an OS alias
            // such as macOS /var rather than /private/var.
            path.canonicalize()
                .ok()?
                .strip_prefix(project_root.canonicalize().ok()?)
                .ok()
                .map(Path::to_owned)
        });
    relative
        .as_deref()
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn source_path(project_root: &Path, uri: &str, fallback: &Path) -> String {
    url::Url::parse(uri)
        .ok()
        .and_then(|uri| uri.to_file_path().ok())
        .map(|path| display_path(project_root, &path))
        .unwrap_or_else(|| display_path(project_root, fallback))
}

fn org_content_id(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        line.strip_prefix(":ID:")
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
    })
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f12_composed_receipt_uses_consumed_scripts_templates_and_source_bytes() {
        use berlin_core::PipelineLoader;
        let root = tempfile::tempdir().unwrap();
        let project = crate::project::Project::new(root.path().to_owned(), vec![]);
        super::super::state::migrate(&project).unwrap();
        let script = root.path().join("berlin.pipeline.rhai");
        let template = root.path().join("page.tera");
        let source = root.path().join("source.md");
        let program=berlin_pipeline_dsl::RhaiPipelineLoader::new(r#"
            fn compose(c){throw "not run in receipt test";}
            pipeline text {output render_text(compose(catalog([documents(load_markdown("*.md"))]),Fn("compose")),"_text",text_config(#{}));}
        "#).unwrap();
        let plan = program.load("text").unwrap();
        let snapshots = vec![
            ConsumedInput::new(
                script.clone(),
                "pipelines/0.rhai".into(),
                b"consumed script",
            ),
            ConsumedInput::new(
                template.clone(),
                "pages/page.tera".into(),
                b"consumed template",
            ),
        ];
        for path in [&script, &template, &source] {
            fs::write(path, "changed after consumption").unwrap();
        }
        let load = plan
            .nodes()
            .iter()
            .find(|node| matches!(node.operation, Operation::LoadMarkdown { .. }))
            .unwrap();
        let artifacts = HashMap::from([(
            load.id.clone(),
            RuntimeArtifact::MarkdownSources(vec![SourceFile {
                uri: url::Url::from_file_path(&source).unwrap().to_string(),
                path: source,
                text: "consumed source".into(),
            }]),
        )]);
        write(ReceiptContext {
            consumed_inputs: &snapshots,
            project: &project,
            pipeline: "text",
            started_at: SystemTime::now(),
            duration: Duration::ZERO,
            plan: &plan,
            artifacts: &artifacts,
            owned_roots: &[],
            diagnostics: &[],
            error: None,
        })
        .unwrap();
        let receipt = super::super::state::Store::open(root.path())
            .unwrap()
            .build(1)
            .unwrap()["receipt"]
            .clone();
        assert!(!root.path().join(".berlin/receipts").exists());
        let inputs = receipt["inputs"].as_array().unwrap();
        for (name, bytes) in [
            ("berlin.pipeline.rhai", b"consumed script".as_slice()),
            ("page.tera", b"consumed template".as_slice()),
            ("source.md", b"consumed source".as_slice()),
        ] {
            assert_eq!(
                inputs.iter().find(|input| input["path"] == name).unwrap()["sha256"],
                digest(bytes)
            );
        }
    }

    #[test]
    fn digest_is_stable_sha256() {
        assert_eq!(
            digest(b"berlin"),
            "fb38a93fd89e1a5fc7852a5d7109e321d23a721a1899292273ac5a21dc4db378"
        );
    }

    #[test]
    fn extracts_a_file_level_org_id() {
        assert_eq!(
            org_content_id(":PROPERTIES:\n:ID: stable-id\n:END:\n"),
            Some("stable-id".into())
        );
    }

    #[test]
    fn original_org_receipt_uses_loaded_text_after_source_changes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("note.org");
        fs::write(&path, ":PROPERTIES:\n:ID: captured\n:END:\nOriginal.").unwrap();
        let source = SourceFile::read(path.clone()).unwrap();
        fs::write(&path, ":PROPERTIES:\n:ID: changed\n:END:\nLater.").unwrap();
        let mut inputs = BTreeMap::new();
        collect_artifact_inputs(
            &mut inputs,
            root.path(),
            &RuntimeArtifact::OrgSources(vec![source.clone()]),
        )
        .unwrap();
        assert_eq!(inputs[&source.uri].sha256, digest(source.text.as_bytes()));
        assert_eq!(inputs[&source.uri].content_id.as_deref(), Some("captured"));
    }

    #[test]
    fn loaded_text_replaces_file_snapshot_without_losing_identity() {
        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("article.md");
        fs::write(&path, "on disk").unwrap();
        let source = SourceFile {
            uri: url::Url::from_file_path(&path).unwrap().to_string(),
            path,
            text: "loaded snapshot".into(),
        };
        let mut inputs = BTreeMap::new();
        insert_file_input(
            &mut inputs,
            project.path(),
            &source.path,
            Some("article".into()),
        )
        .unwrap();

        collect_text_inputs(&mut inputs, project.path(), std::slice::from_ref(&source));

        let input = &inputs[&source.uri];
        assert_eq!(inputs.len(), 1);
        assert_eq!(input.path, "article.md");
        assert_eq!(input.sha256, digest(source.text.as_bytes()));
        assert_eq!(input.content_id.as_deref(), Some("article"));
    }

    #[test]
    fn source_collection_excludes_exported_markdown_and_missing_artifacts() {
        let plan = PipelinePlan::new()
            .with_node(PipelineNode::new(
                "loaded",
                Operation::LoadMarkdown {
                    pattern: "*.md".into(),
                },
            ))
            .with_node(PipelineNode::new(
                "exported",
                Operation::ExportOrg {
                    backend: "ox-hugo".into(),
                    section: "notes".into(),
                },
            ))
            .with_node(PipelineNode::new(
                "missing",
                Operation::LoadCss {
                    pattern: "*.css".into(),
                },
            ));
        let sources = |name: &str| {
            RuntimeArtifact::MarkdownSources(vec![SourceFile {
                path: PathBuf::from(format!("/project/{name}.md")),
                uri: format!("file:///project/{name}.md"),
                text: name.into(),
            }])
        };
        let artifacts = HashMap::from([
            (NodeId::from("loaded"), sources("loaded")),
            (NodeId::from("exported"), sources("exported")),
        ]);
        let mut inputs = BTreeMap::new();

        collect_source_inputs(&mut inputs, Path::new("/project"), &plan, &artifacts).unwrap();

        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs["file:///project/loaded.md"].path, "loaded.md");
    }

    #[test]
    fn content_id_enrichment_is_consistent_across_artifacts() {
        let uri = "file:///project/article.md";
        let source = markdown::Source::new("# Article", uri).unwrap();
        let original = markdown::Parser::new().parse(&source).unwrap();
        let plan = PipelinePlan::new()
            .with_node(PipelineNode::new("first", Operation::ParseMarkdown))
            .with_node(PipelineNode::new("second", Operation::ParseMarkdown));

        for second_id in ["article", "conflicting"] {
            for reverse_insertion in [false, true] {
                let mut entries = [("first", "article"), ("second", second_id)];
                if reverse_insertion {
                    entries.reverse();
                }
                let artifacts = entries
                    .into_iter()
                    .map(|(node, id)| {
                        let mut document = original.clone();
                        document.id.0 = id.into();
                        (
                            NodeId::from(node),
                            RuntimeArtifact::Documents(
                                berlin_content::DocumentCollection::new(vec![document]).into(),
                            ),
                        )
                    })
                    .collect();
                let mut inputs = BTreeMap::from([(
                    uri.into(),
                    InputReceipt {
                        path: "article.md".into(),
                        sha256: digest(b"# Article"),
                        content_id: None,
                    },
                )]);

                let result = enrich_content_ids(&mut inputs, &plan, &artifacts);

                if second_id == "article" {
                    result.unwrap();
                    assert_eq!(inputs[uri].content_id.as_deref(), Some("article"));
                } else {
                    assert_eq!(
                        result.unwrap_err().to_string(),
                        "Conflicting content IDs for input 'article.md': 'article' and 'conflicting'"
                    );
                }
            }
        }
    }

    #[test]
    fn unsafe_pipeline_names_cannot_escape_the_receipt_directory() {
        assert_eq!(receipt_filename("site"), "site.json");
        let filename = receipt_filename("../../../outside");
        assert!(filename.starts_with("invalid-"));
        assert!(!filename.contains('/'));
    }
}
