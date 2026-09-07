use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Error;
use berlin_content::DocumentCollection;
use berlin_content::Feed as ContentFeed;
use berlin_content::WebsiteAssembly;
use berlin_core::ArtifactKind;

use crate::project::Project;
use crate::util::fs::load_files;

mod copy_static;
mod css;
mod executor;
mod feed;
mod org;
mod output;
mod receipt;
mod run;
mod website;

#[derive(Clone, Debug)]
struct SourceFile {
    path: PathBuf,
    uri: String,
    text: String,
}

#[derive(Clone, Debug)]
enum RuntimeArtifact {
    OrgSources(Vec<PathBuf>),
    MarkdownSources(Vec<SourceFile>),
    Documents(DocumentCollection),
    DataSources(Vec<SourceFile>),
    Feed(ContentFeed),
    WebsiteAssembly(WebsiteAssembly),
    CssSources(Vec<SourceFile>),
    StaticSources {
        source_root: PathBuf,
        files: Vec<PathBuf>,
    },
}

impl RuntimeArtifact {
    fn kind(&self) -> ArtifactKind {
        match self {
            Self::OrgSources(_) => ArtifactKind::OrgSources,
            Self::MarkdownSources(_) => ArtifactKind::MarkdownSources,
            Self::Documents(_) => ArtifactKind::Documents,
            Self::DataSources(_) => ArtifactKind::DataSources,
            Self::Feed(_) => ArtifactKind::Feed,
            Self::WebsiteAssembly(_) => ArtifactKind::WebsiteAssembly,
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
    pub dry_run: bool,
}

fn pattern_base(root: &Path, pattern: &str) -> PathBuf {
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

fn load_runtime_sources(root: &Path, pattern: &str) -> Result<Vec<SourceFile>, Error> {
    load_files(root, pattern)?
        .into_iter()
        .map(SourceFile::read)
        .collect()
}

pub fn run_pipeline(project: &Project, pipeline: &str) -> Result<(), Error> {
    run_pipeline_with_options(project, pipeline, ExecutionOptions::default())
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
            RuntimeArtifact::Documents(DocumentCollection::default()).kind(),
            ArtifactKind::Documents
        );
        assert_eq!(
            RuntimeArtifact::CssSources(Vec::new()).kind(),
            ArtifactKind::CssSources
        );
        assert_eq!(
            RuntimeArtifact::Feed(ContentFeed::default()).kind(),
            ArtifactKind::Feed
        );
        assert_eq!(
            RuntimeArtifact::WebsiteAssembly(
                WebsiteAssembly::new(DocumentCollection::default(), ContentFeed::default())
                    .unwrap()
            )
            .kind(),
            ArtifactKind::WebsiteAssembly
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
