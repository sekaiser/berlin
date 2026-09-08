use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use anyhow::Error;
use anyhow::bail;

use super::{SourceFile, origins};

const DEFAULT_EXPORTER: &str = include_str!("../../support/ox-hugo/export.el");

/// Existing project overrides remain supported; ordinary projects use the bundled adapter.
pub(super) fn exporter_override(project_root: &Path) -> Option<PathBuf> {
    let path = project_root.join("support/ox-hugo/export.el");
    path.exists().then_some(path)
}

#[derive(Debug, Eq, PartialEq)]
struct Export {
    source: PathBuf,
    /// Physical location of the staged Markdown output.
    destination: PathBuf,
    /// Final project location used for reporting and source identity.
    published_path: PathBuf,
}

pub(super) fn export(
    project: &crate::project::Project,
    output_root: &Path,
    sources: &[PathBuf],
    backend: &str,
    output_directory: &Path,
    section: &str,
    dry_run: bool,
) -> Result<Vec<SourceFile>, Error> {
    let project_root = project.root();
    if backend != "ox-hugo" {
        bail!("Unsupported Org export backend '{backend}'");
    }

    validate_section(section)?;
    let workspace = output_root.join(output_directory);
    let content = Path::new("content").join(section);
    let exports = plan_exports(
        sources,
        &workspace.join(&content),
        &project_root.join(output_directory).join(&content),
    )?;
    report_exports(project_root, &exports, dry_run);
    if dry_run {
        return Ok(Vec::new());
    }

    run_ox_hugo(project, &workspace, section, &exports)?;
    read_exported_sources(exports)
}

fn plan_exports(
    sources: &[PathBuf],
    staged_output_directory: &Path,
    published_output_directory: &Path,
) -> Result<Vec<Export>, Error> {
    if sources.is_empty() {
        bail!("Org source pattern matched no files");
    }

    let exports = sources
        .iter()
        .map(|source| {
            Ok(Export {
                source: source.clone(),
                destination: markdown_path_for_org_source(source, staged_output_directory)?,
                published_path: markdown_path_for_org_source(source, published_output_directory)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let mut destinations = HashMap::new();
    for export in &exports {
        if let Some(previous) = destinations.insert(&export.destination, &export.source) {
            bail!(
                "Org sources '{}' and '{}' both export to '{}'",
                previous.display(),
                export.source.display(),
                export.destination.display()
            );
        }
    }
    Ok(exports)
}

fn report_exports(project_root: &Path, exports: &[Export], dry_run: bool) {
    for export in exports {
        println!(
            "{} {} -> {}",
            if dry_run { "would export" } else { "exporting" },
            relative_or_absolute(project_root, &export.source).display(),
            relative_or_absolute(project_root, &export.published_path).display(),
        );
    }
}

fn validate_section(section: &str) -> Result<(), Error> {
    if section.is_empty()
        || section
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || section.contains(['\\', ':'])
    {
        bail!("Ox-Hugo section must be a confined relative path");
    }
    Ok(())
}

fn run_ox_hugo(
    project: &crate::project::Project,
    output_root: &Path,
    section: &str,
    exports: &[Export],
) -> Result<(), Error> {
    std::fs::create_dir_all(output_root.join("content").join(section))?;
    std::fs::create_dir_all(output_root.join("static"))?;

    let emacs = std::env::var("BERLIN_EMACS").unwrap_or_else(|_| "emacs".into());
    let mut bundled = tempfile::Builder::new().suffix(".el").tempfile()?;
    let script = match exporter_override(project.root()) {
        Some(path) => path,
        None => {
            bundled.write_all(DEFAULT_EXPORTER.as_bytes())?;
            bundled.path().to_path_buf()
        }
    };
    let source_map = tempfile::NamedTempFile::new()?;
    let mut command = Command::new(&emacs);
    command
        .env("BERLIN_ORG_SOURCE_MAP", source_map.path())
        .arg("--batch")
        .arg("--load")
        .arg(&script)
        .arg("--berlin-export-org")
        .arg(output_root)
        .arg(section);
    command.args(exports.iter().map(|export| &export.source));

    let status = command
        .status()
        .with_context(|| format!("Failed to start Emacs using '{emacs}'"))?;
    if !status.success() {
        bail!("Ox-Hugo export failed with {status}");
    }
    // Navigation is optional and local. A missing/old adapter must not prevent
    // publication; exact export hashes keep a failed transaction's maps inert.
    if let Err(error) = store_origins(project, exports, source_map.path()) {
        log::warn!("Exported Markdown, but could not store Org navigation: {error:#}");
    }
    Ok(())
}

fn store_origins(
    project: &crate::project::Project,
    exports: &[Export],
    file: &Path,
) -> Result<(), Error> {
    let origins: origins::ExportOrigins = serde_json::from_slice(&std::fs::read(file)?)?;
    if origins.schema_version != 1 {
        bail!("Unsupported Org origin schema");
    }
    for mut origin in origins.documents {
        let source = origin.source.canonicalize()?;
        let markdown = origin.markdown.canonicalize()?;
        let export = exports
            .iter()
            .find(|export| {
                export
                    .destination
                    .canonicalize()
                    .is_ok_and(|path| path == markdown)
                    && export
                        .source
                        .canonicalize()
                        .is_ok_and(|path| path == source)
            })
            .context("Org origin does not match a declared export")?;
        if origins::digest(&std::fs::read(&export.destination)?) != origin.markdown_hash
            || origins::digest(&std::fs::read(&export.source)?) != origin.source_hash
        {
            bail!("Org or Markdown changed while exporting source navigation");
        }
        origin.source = export.source.clone();
        origins::store(project, &export.published_path, origin)?;
    }
    Ok(())
}

fn read_exported_sources(exports: Vec<Export>) -> Result<Vec<SourceFile>, Error> {
    exports
        .into_iter()
        .map(|export| SourceFile::read_with_uri_path(export.destination, &export.published_path))
        .collect()
}

/// Maps an Org source filename to a Markdown path in the output directory.
/// Source subdirectories are not preserved; only the final extension is replaced.
fn markdown_path_for_org_source(
    org_source: &Path,
    output_directory: &Path,
) -> Result<PathBuf, Error> {
    let file_name = org_source
        .file_name()
        .context("Org source has no file name")?;
    let markdown_path = output_directory.join(file_name).with_extension("md");
    Ok(markdown_path)
}

fn relative_or_absolute<'a>(root: &Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_uses_source_basename() {
        let destination = markdown_path_for_org_source(
            Path::new("/project/data/20230520134122-test.org"),
            Path::new("/project/content/notes"),
        )
        .unwrap();

        assert_eq!(
            destination,
            Path::new("/project/content/notes/20230520134122-test.md")
        );
    }

    #[test]
    fn destination_preserves_intermediate_filename_suffixes() {
        let destination = markdown_path_for_org_source(
            Path::new("/project/data/article.en.org"),
            Path::new("/project/content/notes"),
        )
        .unwrap();

        assert_eq!(
            destination,
            Path::new("/project/content/notes/article.en.md")
        );
    }

    #[test]
    fn destination_requires_a_source_filename() {
        let error =
            markdown_path_for_org_source(Path::new("/"), Path::new("/project/content/notes"))
                .unwrap_err();

        assert_eq!(error.to_string(), "Org source has no file name");
    }

    #[test]
    fn dry_run_does_not_require_emacs_or_existing_outputs() {
        let sources = vec![PathBuf::from("/project/data/article.org")];
        let result = export(
            &crate::project::Project::new("/project".into(), vec![]),
            Path::new("/staging"),
            &sources,
            "ox-hugo",
            Path::new("content/notes"),
            "notes",
            true,
        )
        .unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn plans_staged_and_published_paths_separately() {
        let source = PathBuf::from("/project/data/article.en.org");
        let exports = plan_exports(
            std::slice::from_ref(&source),
            Path::new("/staging/content/notes"),
            Path::new("/project/content/notes"),
        )
        .unwrap();

        assert_eq!(
            exports,
            vec![Export {
                source,
                destination: PathBuf::from("/staging/content/notes/article.en.md"),
                published_path: PathBuf::from("/project/content/notes/article.en.md"),
            }]
        );
    }

    #[test]
    fn planning_requires_sources() {
        let error = plan_exports(&[], Path::new("/staging"), Path::new("/project")).unwrap_err();
        assert_eq!(error.to_string(), "Org source pattern matched no files");
    }

    #[test]
    fn reads_staged_content_with_published_identity() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("staged.md");
        let published_path = root.path().join("published.md");
        std::fs::write(&destination, "Exported Markdown").unwrap();

        let sources = read_exported_sources(vec![Export {
            source: root.path().join("source.org"),
            destination: destination.clone(),
            published_path: published_path.clone(),
        }])
        .unwrap();

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].path, destination);
        assert_eq!(sources[0].text, "Exported Markdown");
        assert_eq!(
            sources[0].uri,
            url::Url::from_file_path(published_path)
                .unwrap()
                .to_string()
        );
    }

    #[test]
    fn dry_run_rejects_unsafe_hugo_sections() {
        for section in [
            "",
            ".",
            "..",
            "../notes",
            "/notes",
            "notes/../../escape",
            "notes\\escape",
            "notes//child",
        ] {
            let error = export(
                &crate::project::Project::new("/project".into(), vec![]),
                Path::new("/staging"),
                &[PathBuf::from("/project/data/article.org")],
                "ox-hugo",
                Path::new(".berlin/generated/org"),
                section,
                true,
            )
            .unwrap_err();

            assert_eq!(
                error.to_string(),
                "Ox-Hugo section must be a confined relative path"
            );
        }
    }

    #[test]
    fn duplicate_source_basenames_are_rejected() {
        let sources = vec![
            PathBuf::from("/project/data/one/article.org"),
            PathBuf::from("/project/data/two/article.org"),
        ];

        let error = export(
            &crate::project::Project::new("/project".into(), vec![]),
            Path::new("/staging"),
            &sources,
            "ox-hugo",
            Path::new("content/notes"),
            "notes",
            true,
        )
        .unwrap_err();

        assert!(error.to_string().contains("both export to"));
    }
}
