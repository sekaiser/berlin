use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Error;
use berlin_content::DocumentCollection;
use berlin_core::ArtifactKind;

use crate::project::Project;
use crate::util::fs::load_files;

pub(crate) mod announcement;
mod cache;
pub(crate) mod check;
mod composed;
mod copy_static;
mod css;
mod executor;
pub(crate) mod graphics;
mod links;
mod org;
mod origins;
mod output;
pub(crate) mod paper;
mod production;
pub(crate) mod publication;
pub(crate) mod receipt;
pub(crate) mod release;
mod resources;
mod run;
pub(crate) mod theme;

#[derive(Clone, Debug)]
struct SourceFile {
    path: PathBuf,
    uri: String,
    text: String,
}

#[derive(Clone, Debug)]
enum RuntimeArtifact {
    Catalog(berlin_publication::PublicationCatalog),
    Composed(berlin_publication::ComposedArtifact),
    LatexSources(Vec<paper::Source>),
    OrgSources(Vec<SourceFile>),
    MarkdownSources(Vec<SourceFile>),
    Documents(std::sync::Arc<DocumentCollection>),
    DataSources(Vec<SourceFile>),
    ExternalResources(berlin_publication::ExternalResourceCollection),
    CssSources(Vec<SourceFile>),
    StaticSources {
        source_root: PathBuf,
        snapshot: std::sync::Arc<Vec<graphics::Input>>,
    },
}

impl RuntimeArtifact {
    fn kind(&self) -> ArtifactKind {
        match self {
            Self::Catalog(_) => ArtifactKind::Catalog,
            Self::Composed(_) => ArtifactKind::Composed,
            Self::LatexSources(_) => ArtifactKind::LatexSources,
            Self::OrgSources(_) => ArtifactKind::OrgSources,
            Self::MarkdownSources(_) => ArtifactKind::MarkdownSources,
            Self::Documents(_) => ArtifactKind::Documents,
            Self::DataSources(_) => ArtifactKind::DataSources,
            Self::ExternalResources(_) => ArtifactKind::ExternalResources,
            Self::CssSources(_) => ArtifactKind::CssSources,
            Self::StaticSources { .. } => ArtifactKind::StaticAssets,
        }
    }
}

impl SourceFile {
    fn read(path: PathBuf) -> Result<Self, Error> {
        Self::read_with_uri_path(path.clone(), &path)
    }

    fn read_with_uri_path(path: PathBuf, uri_path: &Path) -> Result<Self, Error> {
        let uri = url::Url::from_file_path(uri_path)
            .map_err(|_| anyhow::anyhow!("Invalid source path: {}", path.display()))?;
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("Unable to read source {}", path.display()))?;
        Ok(Self {
            path,
            uri: uri.to_string(),
            text,
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExecutionOptions {
    pub no_cache: bool,
    pub allow_tex: bool,
    pub dry_run: bool,
    pub production: bool,
}

pub(crate) fn pattern_base(root: &Path, pattern: &str) -> PathBuf {
    let wildcard = pattern
        .char_indices()
        .find_map(|(index, character)| "*?[".contains(character).then_some(index));
    let literal = wildcard.map_or(pattern, |index| &pattern[..index]);
    let candidate = root.join(literal.trim_end_matches('/'));
    if wildcard.is_some() || literal.ends_with('/') {
        candidate
    } else {
        candidate
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf())
    }
}

fn load_runtime_sources(project: &Project, pattern: &str) -> Result<Vec<SourceFile>, Error> {
    let files = load_files(project, pattern)?;
    if files.is_empty() && pattern.starts_with(".berlin/generated/") {
        anyhow::bail!(
            "Generated input '{pattern}' matched no files; build its producer pipeline first"
        );
    }
    files.into_iter().map(SourceFile::read).collect()
}

/// Structured data inputs are bounded before materialization and confined to
/// the project or explicitly named root, including ordinary symlink traversal.
fn load_runtime_data(project: &Project, pattern: &str) -> Result<Vec<SourceFile>, Error> {
    use std::io::Read;
    let limits = berlin_publication::ResourceLimits::default();
    let files = load_files(project, pattern)?;
    if files.is_empty() && pattern.starts_with(".berlin/generated/") {
        anyhow::bail!(
            "Generated input '{pattern}' matched no files; build its producer pipeline first"
        );
    }
    if files.len() > limits.catalog_items {
        anyhow::bail!("ResourceLimit: data input file count exceeded");
    }
    let (root, _) = crate::util::fs::source_location(project, pattern)?;
    let root = root.canonicalize()?;
    let mut total_bytes = 0usize;
    files
        .into_iter()
        .map(|path| {
            let path = path.canonicalize()?;
            if !path.starts_with(&root) {
                anyhow::bail!("data input resolves outside its declared source root");
            }
            let mut bytes = Vec::new();
            std::fs::File::open(&path)?
                .take(limits.file_bytes as u64 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > limits.file_bytes {
                anyhow::bail!("ResourceLimit: data input file byte limit exceeded");
            }
            total_bytes = total_bytes
                .checked_add(bytes.len())
                .context("data input size overflow")?;
            if total_bytes > limits.descriptor_bytes {
                anyhow::bail!("ResourceLimit: combined data input byte limit exceeded");
            }
            let text = String::from_utf8(bytes).context("data input is not UTF-8")?;
            let uri = url::Url::from_file_path(&path)
                .map_err(|_| anyhow::anyhow!("invalid data input path"))?
                .to_string();
            Ok(SourceFile { path, uri, text })
        })
        .collect()
}

pub(crate) fn prepare_release(
    project: &Project,
    pipeline: &str,
    options: ExecutionOptions,
) -> Result<release::WebsiteRelease, Error> {
    run::PipelineRun::new(project, pipeline, options).release()
}

pub fn run_pipeline_with_options(
    project: &Project,
    pipeline: &str,
    options: ExecutionOptions,
) -> Result<(), Error> {
    run::PipelineRun::new(project, pipeline, options).run()
}

#[cfg(test)]
mod runtime_tests {
    use super::*;

    #[test]
    fn missing_generated_sources_require_an_explicit_producer_build() {
        let root = tempfile::tempdir().unwrap();
        let error = load_runtime_sources(
            &Project::new(root.path().to_owned(), vec![]),
            ".berlin/generated/org/content/notes/*.md",
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("build its producer pipeline first")
        );
        assert!(
            load_runtime_sources(
                &Project::new(root.path().to_owned(), vec![]),
                "optional/*.md"
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn runtime_artifacts_report_their_plan_kinds() {
        assert_eq!(
            RuntimeArtifact::OrgSources(Vec::new()).kind(),
            ArtifactKind::OrgSources
        );
        assert_eq!(
            RuntimeArtifact::MarkdownSources(Vec::new()).kind(),
            ArtifactKind::MarkdownSources
        );
        assert_eq!(
            RuntimeArtifact::Documents(DocumentCollection::default().into()).kind(),
            ArtifactKind::Documents
        );
        assert_eq!(
            RuntimeArtifact::CssSources(Vec::new()).kind(),
            ArtifactKind::CssSources
        );
    }

    #[test]
    fn asset_pattern_base_excludes_the_glob_suffix() {
        let root = Path::new("/project");

        assert_eq!(
            pattern_base(root, "static/**/*"),
            Path::new("/project/static")
        );
        assert_eq!(
            pattern_base(root, "assets/images/*.png"),
            Path::new("/project/assets/images")
        );
    }
}
