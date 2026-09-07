use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use anyhow::Error;
use anyhow::bail;

use super::SourceFile;

#[derive(Debug, Eq, PartialEq)]
struct Export {
    source: PathBuf,
    /// Physical location of the staged Markdown output.
    destination: PathBuf,
    /// Final project location used for reporting and source identity.
    published_path: PathBuf,
}

pub(super) fn export(
    project_root: &Path,
    output_root: &Path,
    sources: &[PathBuf],
    backend: &str,
    output_directory: &Path,
    dry_run: bool,
) -> Result<Vec<SourceFile>, Error> {
    if backend != "ox-hugo" {
        bail!("Unsupported Org export backend '{backend}'");
    }

    let section = hugo_section(output_directory)?;
    let exports = plan_exports(
        sources,
        &output_root.join(output_directory),
        &project_root.join(output_directory),
    )?;
    report_exports(project_root, &exports, dry_run);
    if dry_run {
        return Ok(Vec::new());
    }

    run_ox_hugo(project_root, output_root, section, &exports)?;
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

fn hugo_section(output_directory: &Path) -> Result<&str, Error> {
    let section = output_directory
        .strip_prefix("content")
        .context("Ox-Hugo output must be within the content directory")?;
    let section = section
        .to_str()
        .context("Ox-Hugo section is not valid UTF-8")?;
    if section.is_empty() {
        bail!("Ox-Hugo output must name a section below the content directory");
    }
    Ok(section)
}

fn run_ox_hugo(
    project_root: &Path,
    output_root: &Path,
    section: &str,
    exports: &[Export],
) -> Result<(), Error> {
    std::fs::create_dir_all(output_root.join("content").join(section))?;
    std::fs::create_dir_all(output_root.join("static"))?;

    let emacs = std::env::var("BERLIN_EMACS").unwrap_or_else(|_| "emacs".into());
    let script = project_root.join("support/ox-hugo/export.el");
    let mut command = Command::new(&emacs);
    command
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
            Path::new("/project"),
            Path::new("/staging"),
            &sources,
            "ox-hugo",
            Path::new("content/notes"),
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
    fn dry_run_rejects_invalid_hugo_output_directories() {
        for (directory, expected) in [
            (
                "notes",
                "Ox-Hugo output must be within the content directory",
            ),
            (
                "content",
                "Ox-Hugo output must name a section below the content directory",
            ),
        ] {
            let error = export(
                Path::new("/project"),
                Path::new("/staging"),
                &[PathBuf::from("/project/data/article.org")],
                "ox-hugo",
                Path::new(directory),
                true,
            )
            .unwrap_err();

            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn duplicate_source_basenames_are_rejected() {
        let sources = vec![
            PathBuf::from("/project/data/one/article.org"),
            PathBuf::from("/project/data/two/article.org"),
        ];

        let error = export(
            Path::new("/project"),
            Path::new("/staging"),
            &sources,
            "ox-hugo",
            Path::new("content/notes"),
            true,
        )
        .unwrap_err();

        assert!(error.to_string().contains("both export to"));
    }
}
