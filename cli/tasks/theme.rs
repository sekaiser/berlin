//! Resolves local presentation files without installing into project sources.
//! Project files override shared theme files, which override theme defaults.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Error, bail};

use crate::templates::Templates;

/// Effective source files keyed by their project-relative presentation path.
pub(crate) struct Theme {
    files: BTreeMap<PathBuf, PathBuf>,
}

impl Theme {
    pub fn load(project_root: &Path, selected: &Path) -> Result<Self, Error> {
        let root = theme_root(project_root, selected)?;
        let mut files = BTreeMap::new();
        for layer in [
            root.join("defaults"),
            root.join("shared"),
            project_root.to_owned(),
        ] {
            for directory in ["pages", "styles", "static"] {
                collect_layer(&layer, &layer.join(directory), &mut files)?;
            }
        }
        Ok(Self { files })
    }

    pub fn inputs(&self) -> impl Iterator<Item = &PathBuf> {
        self.files.values()
    }

    pub fn templates(&self) -> Result<Templates, Error> {
        let sources = self
            .files
            .iter()
            .filter_map(|(relative, source)| {
                let name = relative.strip_prefix("pages").ok()?;
                (name.extension()? == "tera").then(|| {
                    let name = name.to_str().context("Template path is not valid UTF-8")?;
                    Ok((name.replace('\\', "/"), fs::read_to_string(source)?))
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Templates::from_sources(sources)
    }

    /// Files in styles/entries declare the public stylesheets. Other styles are
    /// import-only sources. CSS imports see the same override hierarchy.
    /// Only a temporary build workspace and the caller's staged output are written.
    pub fn write_assets(&self, output_root: &Path) -> Result<(), Error> {
        let workspace = tempfile::Builder::new().prefix("berlin-theme-").tempdir()?;
        for (relative, source) in &self.files {
            if relative.starts_with("styles") {
                copy_file(source, &workspace.path().join(relative))?;
            }
        }
        let entries = self.stylesheet_entries()?;
        let compiled = entries
            .iter()
            .map(|entry| {
                let css = super::css::compile_stylesheet(
                    &workspace.path().join(entry),
                    &workspace.path().join("styles"),
                )?;
                Ok((entry.file_name().expect("entry has a filename"), css))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        for (relative, source) in &self.files {
            if relative.starts_with("static") {
                copy_file(source, &output_root.join(relative))?;
            }
        }
        let destination = output_root.join("assets/css");
        fs::create_dir_all(&destination)?;
        for (name, css) in compiled {
            fs::write(destination.join(name), css)?;
        }
        Ok(())
    }

    fn stylesheet_entries(&self) -> Result<Vec<&PathBuf>, Error> {
        let entries = self
            .files
            .keys()
            .filter(|path| {
                path.parent() == Some(Path::new("styles/entries"))
                    && path.extension().is_some_and(|extension| extension == "css")
            })
            .collect::<Vec<_>>();
        if entries.is_empty() {
            bail!("Theme requires at least one styles/entries/*.css entry point");
        }
        Ok(entries)
    }
}

/// An explicit local path may point outside the project (for a sibling theme repo).
/// Descendant symlinks are rejected when collecting files, avoiding accidental
/// inclusion of unrelated private files or recursive directory cycles.
pub(crate) fn theme_root(project_root: &Path, selected: &Path) -> Result<PathBuf, Error> {
    if selected.as_os_str().is_empty() {
        bail!("Website theme path must not be empty");
    }
    let location = resolve_theme_location(
        project_root,
        selected,
        std::env::var_os("BERLIN_THEME_DIR").as_deref(),
    )?;
    let root = location
        .canonicalize()
        .with_context(|| format!("Unable to resolve website theme '{}'", selected.display()))?;
    for directory in ["shared", "defaults"] {
        let metadata = fs::symlink_metadata(root.join(directory))
            .with_context(|| format!("Website theme '{}' requires {directory}/", root.display()))?;
        if !metadata.is_dir() || metadata.is_symlink() {
            bail!(
                "Website theme layer must be a real directory: {}",
                root.join(directory).display()
            );
        }
    }
    Ok(root)
}

fn resolve_theme_location(
    project_root: &Path,
    selected: &Path,
    directory: Option<&std::ffi::OsStr>,
) -> Result<PathBuf, Error> {
    if let Some(name) = selected.to_str().and_then(|value| value.strip_prefix('@')) {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            bail!("Named themes require a simple name after @");
        }
        let directory = directory.context("Set BERLIN_THEME_DIR to resolve named themes")?;
        let directory = Path::new(directory);
        if !directory.is_absolute() {
            bail!("BERLIN_THEME_DIR must be an absolute directory");
        }
        return Ok(directory.join(name));
    }
    Ok(project_root.join(selected))
}

fn collect_layer(
    root: &Path,
    path: &Path,
    files: &mut BTreeMap<PathBuf, PathBuf>,
) -> Result<(), Error> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.is_symlink() {
        bail!(
            "Theme presentation files must not be symlinks: {}",
            path.display()
        );
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let path = entry?.path();
            if path.file_name().and_then(|name| name.to_str()) != Some(".DS_Store") {
                collect_layer(root, &path, files)?;
            }
        }
    } else if metadata.is_file() {
        files.insert(path.strip_prefix(root)?.to_owned(), path.to_owned());
    } else {
        bail!("Unsupported theme presentation file: {}", path.display());
    }
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), Error> {
    fs::create_dir_all(
        destination
            .parent()
            .context("Asset destination has no parent")?,
    )?;
    // Copy bytes without the OS clone/copyfile optimization, which can emit
    // source-file notifications on macOS and retrigger the theme watcher.
    let mut input = fs::File::open(source)
        .with_context(|| format!("Unable to read theme asset '{}'", source.display()))?;
    let mut output = fs::File::create(destination)?;
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_themes_use_explicit_locations_without_checkout_assumptions() {
        let root = Path::new("/site");
        let directory = std::ffi::OsStr::new("/installed/themes");
        assert_eq!(
            resolve_theme_location(root, Path::new("@notebook"), Some(directory)).unwrap(),
            Path::new("/installed/themes/notebook")
        );
        assert!(resolve_theme_location(root, Path::new("@notebook"), None).is_err());
        assert!(resolve_theme_location(root, Path::new("@../private"), Some(directory)).is_err());
        assert!(
            resolve_theme_location(
                root,
                Path::new("@notebook"),
                Some(std::ffi::OsStr::new("relative"))
            )
            .is_err()
        );
        assert_eq!(
            resolve_theme_location(root, Path::new("../theme"), None).unwrap(),
            Path::new("/site/../theme")
        );
    }

    fn write(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn fixture(root: &Path) {
        write(
            root,
            "theme/defaults/pages/_base.tera",
            "<main>{% block content %}{% endblock %}</main>",
        );
        write(
            root,
            "theme/shared/pages/index.tera",
            "{% extends \"_base.tera\" %}{% block content %}Theme{% endblock %}",
        );
        write(
            root,
            "theme/shared/styles/entries/notebook.css",
            "@import '../tokens.css'; p { color: var(--ink); }",
        );
        write(
            root,
            "theme/shared/styles/tokens.css",
            ":root { --ink: green; }",
        );
        write(root, "theme/defaults/static/default.txt", "default asset");
        write(
            root,
            "theme/shared/styles/entries/article.css",
            ".article { display: block; }",
        );
    }

    #[test]
    fn resolves_overrides_before_parsing_inheritance_and_compiling_imports() {
        let root = tempfile::tempdir().unwrap();
        fixture(root.path());
        let project = root.path().join("project");
        write(
            &project,
            "pages/_base.tera",
            "<article>{% block content %}{% endblock %}</article>",
        );
        write(&project, "styles/tokens.css", ":root { --ink: purple; }");
        let theme = Theme::load(&project, Path::new("../theme")).unwrap();
        let templates = theme.templates().unwrap();
        assert_eq!(
            templates
                .render_template("index.tera", &tera::Context::new())
                .unwrap(),
            "<article>Theme</article>"
        );
        let output = root.path().join("output");
        theme.write_assets(&output).unwrap();
        assert!(
            fs::read_to_string(output.join("assets/css/notebook.css"))
                .unwrap()
                .contains("--ink:purple")
        );
        assert_eq!(
            fs::read_to_string(output.join("static/default.txt")).unwrap(),
            "default asset"
        );
        assert!(!project.join("pages/index.tera").exists());
        assert!(!project.join("css").exists());
        assert!(output.join("assets/css/article.css").exists());
        assert!(!output.join("assets/css/tokens.css").exists());
        assert!(!output.join("styles").exists());
        assert!(
            !theme
                .inputs()
                .any(|path| path == &root.path().join("theme/shared/styles/tokens.css"))
        );
        assert!(
            theme
                .inputs()
                .any(|path| path == &project.join("styles/tokens.css"))
        );
    }

    #[test]
    fn missing_themes_and_entry_points_fail_explicitly() {
        let root = tempfile::tempdir().unwrap();
        assert!(Theme::load(root.path(), Path::new("")).is_err());
        assert!(Theme::load(root.path(), Path::new("missing")).is_err());
        fixture(root.path());
        fs::remove_file(root.path().join("theme/shared/styles/entries/notebook.css")).unwrap();
        fs::remove_file(root.path().join("theme/shared/styles/entries/article.css")).unwrap();
        let theme = Theme::load(root.path(), Path::new("theme")).unwrap();
        assert!(
            theme
                .write_assets(&root.path().join("output"))
                .unwrap_err()
                .to_string()
                .contains("entry point")
        );
        assert!(!root.path().join("output").exists());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_assets_and_layer_roots() {
        let root = tempfile::tempdir().unwrap();
        fixture(root.path());
        let link = root.path().join("theme/shared/static/private.txt");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink("/nonexistent/private", &link).unwrap();
        assert!(
            Theme::load(root.path(), Path::new("theme"))
                .err()
                .unwrap()
                .to_string()
                .contains("symlinks")
        );
        fs::remove_file(link).unwrap();
        fs::rename(
            root.path().join("theme/shared"),
            root.path().join("elsewhere"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            root.path().join("elsewhere"),
            root.path().join("theme/shared"),
        )
        .unwrap();
        assert!(Theme::load(root.path(), Path::new("theme")).is_err());
    }
}
