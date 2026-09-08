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
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

use berlin_core::NodeId;
use berlin_core::Operation;
use berlin_core::PipelineNode;
use berlin_core::PipelinePlan;

use super::RuntimeArtifact;
use super::SourceFile;
use crate::util::fs::load_files;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct Diagnostic {
    pub node: String,
    pub content_id: String,
    pub code: String,
    pub message: String,
}

#[derive(Serialize)]
struct BuildReceipt<'a> {
    schema_version: u8,
    berlin_version: String,
    pipeline: &'a str,
    status: &'static str,
    started_at_unix_seconds: u64,
    duration_ms: u64,
    inputs: Vec<InputReceipt>,
    outputs: Vec<FileReceipt>,
    diagnostics: &'a [Diagnostic],
    error: Option<&'a str>,
}

#[derive(Clone, Serialize)]
struct InputReceipt {
    path: String,
    sha256: String,
    content_id: Option<String>,
}

#[derive(Serialize)]
struct FileReceipt {
    path: String,
    sha256: String,
}

pub(super) struct ReceiptContext<'a> {
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
    let receipt = BuildReceipt {
        schema_version: 1,
        berlin_version: crate::version::berlin(),
        pipeline: context.pipeline,
        status: if context.error.is_some() {
            "failed"
        } else {
            "succeeded"
        },
        started_at_unix_seconds: context
            .started_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        duration_ms: context.duration.as_millis().try_into().unwrap_or(u64::MAX),
        inputs: input_receipts(context.project, context.plan, context.artifacts)?,
        outputs: if context.error.is_none() {
            output_receipts(context.project.root(), context.owned_roots)?
        } else {
            Vec::new()
        },
        diagnostics: context.diagnostics,
        error: context.error,
    };

    persist(context.project.root(), context.pipeline, &receipt)
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
        pipeline,
        status: "failed",
        started_at_unix_seconds: started_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        duration_ms: duration.as_millis().try_into().unwrap_or(u64::MAX),
        inputs: inputs.into_values().collect(),
        outputs: Vec::new(),
        diagnostics: &[],
        error: Some(error),
    };
    persist(project_root, pipeline, &receipt)
}

fn persist(project_root: &Path, pipeline: &str, receipt: &BuildReceipt<'_>) -> Result<(), Error> {
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

fn input_receipts(
    project: &crate::project::Project,
    plan: &PipelinePlan,
    artifacts: &HashMap<NodeId, RuntimeArtifact>,
) -> Result<Vec<InputReceipt>, Error> {
    let mut inputs = BTreeMap::new();
    collect_operational_inputs(&mut inputs, project, plan)?;
    collect_source_inputs(&mut inputs, project.root(), plan, artifacts)?;
    enrich_content_ids(&mut inputs, plan, artifacts)?;
    Ok(inputs.into_values().collect())
}

fn collect_operational_inputs(
    inputs: &mut BTreeMap<String, InputReceipt>,
    project: &crate::project::Project,
    plan: &PipelinePlan,
) -> Result<(), Error> {
    for path in operational_inputs(project, plan)? {
        insert_file_input(inputs, project.root(), &path, None)?;
    }
    Ok(())
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
        RuntimeArtifact::OrgSources(paths) => collect_org_inputs(inputs, project_root, paths)?,
        RuntimeArtifact::MarkdownSources(sources)
        | RuntimeArtifact::DataSources(sources)
        | RuntimeArtifact::CssSources(sources) => {
            collect_text_inputs(inputs, project_root, sources)
        }
        RuntimeArtifact::StaticSources { files, .. } => {
            collect_file_inputs(inputs, project_root, files)?
        }
        RuntimeArtifact::Documents(_)
        | RuntimeArtifact::Feed(_)
        | RuntimeArtifact::WebsiteAssembly(_) => {}
    }
    Ok(())
}

fn collect_org_inputs(
    inputs: &mut BTreeMap<String, InputReceipt>,
    project_root: &Path,
    paths: &[PathBuf],
) -> Result<(), Error> {
    for path in paths {
        let text = fs::read_to_string(path)?;
        insert_file_input(inputs, project_root, path, org_content_id(&text))?;
    }
    Ok(())
}

fn collect_file_inputs(
    inputs: &mut BTreeMap<String, InputReceipt>,
    project_root: &Path,
    paths: &[PathBuf],
) -> Result<(), Error> {
    for path in paths {
        insert_file_input(inputs, project_root, path, None)?;
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

fn operational_inputs(
    project: &crate::project::Project,
    plan: &PipelinePlan,
) -> Result<Vec<PathBuf>, Error> {
    let project_root = project.root();
    let mut inputs = project.pipeline_files().to_vec();
    for node in plan.nodes() {
        if let Operation::RenderWebsite { config } = &node.operation {
            match &config.theme {
                Some(selected) => inputs.extend(
                    super::theme::Theme::load(project_root, selected)?
                        .inputs()
                        .cloned(),
                ),
                None => inputs.extend(load_files(project, "pages/**/*.tera")?),
            }
        }
    }
    if plan
        .nodes()
        .iter()
        .any(|node| matches!(&node.operation, Operation::ExportOrg { .. }))
        && let Some(path) = super::org::exporter_override(project_root)
    {
        inputs.push(path);
    }
    Ok(inputs)
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
                            RuntimeArtifact::Documents(berlin_content::DocumentCollection::new(
                                vec![document],
                            )),
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
