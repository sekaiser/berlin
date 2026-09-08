use std::path::Path;
use std::path::PathBuf;

use crate::project::Project;
use anyhow::Context;
use anyhow::Error;
use anyhow::bail;
use glob::glob;

pub fn load_files(project: &Project, pattern: &str) -> Result<Vec<PathBuf>, Error> {
    let (root, relative) = source_location(project, pattern)?;
    let pattern_path = root.join(relative);
    let pattern_path_str = pattern_path
        .to_str()
        .context("Input pattern is not valid UTF-8")?;
    let mut files = glob(pattern_path_str)?.collect::<Result<Vec<_>, _>>()?;
    if pattern.starts_with('@') {
        let canonical_root = root.canonicalize()?;
        for file in &mut files {
            *file = file.canonicalize()?;
            if !file.starts_with(&canonical_root) {
                bail!(
                    "Source resolves outside its configured root: {}",
                    file.display()
                );
            }
        }
    }
    files.sort();

    Ok(files)
}

/// Resolves an explicitly named input root, never an output destination.
pub fn source_location(project: &Project, pattern: &str) -> Result<(PathBuf, PathBuf), Error> {
    let cwd = project.root();
    let Some(named) = pattern.strip_prefix('@') else {
        return Ok((cwd.to_owned(), pattern.into()));
    };
    let (name, relative) = named
        .split_once('/')
        .context("Named source needs @name/relative-path")?;
    if relative.is_empty()
        || !Path::new(relative)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        bail!("Named source path must remain inside its configured root");
    }
    let program = crate::pipeline::load_pipeline_program(project)?;
    let directory = program
        .source_roots()
        .get(name)
        .with_context(|| format!("Unknown source root '{name}'"))?;
    Ok((cwd.join(directory), relative.into()))
}

/// Resolves a cached source reference using current configuration, not cache authority.
pub fn resolve_source(project: &Project, reference: &Path) -> Result<PathBuf, Error> {
    let text = reference
        .to_str()
        .context("Source reference is not UTF-8")?;
    let (root, relative) = source_location(project, text)?;
    let source = root.join(relative).canonicalize()?;
    if !source.starts_with(root.canonicalize()?) {
        bail!("Source resolves outside its configured root");
    }
    Ok(source)
}

pub fn source_reference(project: &Project, source: &Path) -> Result<PathBuf, Error> {
    let cwd = project.root();
    if let Ok(relative) = source.strip_prefix(cwd) {
        return Ok(relative.to_owned());
    }
    let source = source.canonicalize()?;
    if let Ok(relative) = source.strip_prefix(cwd.canonicalize()?) {
        return Ok(relative.to_owned());
    }
    let program = crate::pipeline::load_pipeline_program(project)?;
    for (name, directory) in program.source_roots() {
        if let Ok(root) = cwd.join(directory).canonicalize()
            && let Ok(relative) = source.strip_prefix(root)
        {
            return Ok(PathBuf::from(format!("@{name}")).join(relative));
        }
    }
    bail!("Source is outside the project and its configured source roots")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn named_roots_are_explicit_and_confined() {
        let workspace = tempfile::tempdir().unwrap();
        let project = workspace.path().join("site");
        let data = workspace.path().join("data");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&data).unwrap();
        fs::write(
            project.join("berlin.pipeline.rhai"),
            "const SOURCE_ROOTS = #{data: \"../data\"};",
        )
        .unwrap();
        fs::write(data.join("note.org"), "A note").unwrap();
        let files = load_files(&Project::new(project.clone(), vec![]), "@data/*.org").unwrap();
        assert_eq!(files, vec![data.join("note.org").canonicalize().unwrap()]);
        assert!(load_files(&Project::new(project.clone(), vec![]), "@unknown/*.org").is_err());
        assert!(load_files(&Project::new(project.clone(), vec![]), "@data/../site/*").is_err());
        assert!(load_files(&Project::new(project.clone(), vec![]), "@data//etc/passwd").is_err());
        assert_eq!(
            source_reference(&Project::new(project.clone(), vec![]), &files[0]).unwrap(),
            Path::new("@data/note.org")
        );
        assert_eq!(
            resolve_source(
                &Project::new(project.clone(), vec![]),
                Path::new("@data/note.org")
            )
            .unwrap(),
            files[0]
        );
        assert!(
            source_reference(
                &Project::new(project.clone(), vec![]),
                &workspace.path().join("outside.org")
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn named_roots_reject_symlink_escapes() {
        let workspace = tempfile::tempdir().unwrap();
        let project = workspace.path().join("site");
        let data = workspace.path().join("data");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&data).unwrap();
        fs::write(
            project.join("berlin.pipeline.rhai"),
            "const SOURCE_ROOTS = #{data: \"../data\"};",
        )
        .unwrap();
        fs::write(workspace.path().join("secret.org"), "Outside").unwrap();
        std::os::unix::fs::symlink(workspace.path().join("secret.org"), data.join("escape.org"))
            .unwrap();
        assert!(
            load_files(&Project::new(project.clone(), vec![]), "@data/*.org")
                .unwrap_err()
                .to_string()
                .contains("outside")
        );
        assert!(
            resolve_source(
                &Project::new(project.clone(), vec![]),
                Path::new("@data/escape.org")
            )
            .is_err()
        );
    }
}
