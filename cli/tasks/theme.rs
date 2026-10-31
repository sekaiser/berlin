//! Resolves local presentation files without installing into project sources.
//! Project files override shared theme files, which override theme defaults.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Error, bail};

/// Effective source files keyed by their project-relative presentation path.
pub(crate) struct Theme {
    files: BTreeMap<PathBuf, PathBuf>,
}

impl Theme {
    pub(super) fn project_only(project_root: &Path) -> Result<Self, Error> {
        let mut files = BTreeMap::new();
        for directory in ["pages", "styles", "static"] {
            collect_layer(project_root, &project_root.join(directory), &mut files)?;
        }
        Ok(Self { files })
    }
    pub(super) fn snapshot(&self) -> Result<ThemeSnapshot, Error> {
        Ok(ThemeSnapshot {
            files: self
                .files
                .iter()
                .map(|(name, path)| Ok((name.clone(), fs::read(path)?)))
                .collect::<Result<_, Error>>()?,
            paths: self.files.clone(),
        })
    }
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
}

/// Immutable consumed bytes. Presentation checks never materialize a workspace.
pub(super) struct ThemeSnapshot {
    files: BTreeMap<PathBuf, Vec<u8>>,
    paths: BTreeMap<PathBuf, PathBuf>,
}
pub(super) struct PreparedAssets {
    files: BTreeMap<PathBuf, Vec<u8>>,
    inputs: Vec<super::receipt::ConsumedInput>,
}
impl PreparedAssets {
    pub fn consumed_inputs(&self) -> &[super::receipt::ConsumedInput] {
        &self.inputs
    }
    pub fn output_paths(&self) -> BTreeSet<PathBuf> {
        self.files.keys().cloned().collect()
    }
    pub fn write(&self, output: &Path) -> Result<(), Error> {
        for (name, bytes) in &self.files {
            let destination = output.join(name);
            fs::create_dir_all(
                destination
                    .parent()
                    .context("prepared asset has no parent")?,
            )?;
            fs::write(destination, bytes)?;
        }
        Ok(())
    }
}
impl ThemeSnapshot {
    pub fn template_inputs(&self) -> Vec<super::receipt::ConsumedInput> {
        self.consumed_inputs()
            .into_iter()
            .filter(|input| input.logical_name.starts_with("pages/"))
            .collect()
    }
    pub fn prepare_assets(
        &self,
        entries: &BTreeSet<String>,
        cache: Option<&super::cache::ArtifactCache>,
    ) -> Result<PreparedAssets, Error> {
        let mut files = BTreeMap::new();
        let mut read = BTreeSet::new();
        for entry in entries {
            let source = self.publication_entry(entry)?;
            let (css, inputs) =
                super::css::compile_stylesheet_snapshot(&source, &self.files, cache)?;
            files.insert(Path::new("assets/css").join(entry), css.into_bytes());
            read.extend(inputs);
        }
        for (name, bytes) in &self.files {
            if name.starts_with("static") {
                files.insert(name.clone(), bytes.clone());
                read.insert(name.clone());
            }
        }
        let inputs = read
            .into_iter()
            .map(|name| {
                super::receipt::ConsumedInput::new(
                    self.paths[&name].clone(),
                    name.to_string_lossy().replace('\\', "/"),
                    &self.files[&name],
                )
            })
            .collect();
        Ok(PreparedAssets { files, inputs })
    }
    pub fn consumed_inputs(&self) -> Vec<super::receipt::ConsumedInput> {
        self.files
            .iter()
            .map(|(name, bytes)| {
                super::receipt::ConsumedInput::new(
                    self.paths[name].clone(),
                    name.to_string_lossy().replace('\\', "/"),
                    bytes,
                )
            })
            .collect()
    }
    pub fn component_templates(&self) -> Result<BTreeMap<String, String>, Error> {
        self.files
            .iter()
            .filter_map(|(name, bytes)| {
                let relative = name.strip_prefix("pages").ok()?;
                (relative.extension()? == "tera").then(|| {
                    Ok((
                        relative
                            .to_str()
                            .context("template path is not UTF-8")?
                            .replace('\\', "/"),
                        String::from_utf8(bytes.clone())?,
                    ))
                })
            })
            .collect()
    }
    fn publication_entry(&self, entry: &str) -> Result<PathBuf, Error> {
        // Bindings name public filenames, never source paths. Publications have
        // opt-in entries; existing shared entries can still be explicitly used.
        let path = Path::new(entry);
        if path.components().count() != 1
            || path.file_name().and_then(|name| name.to_str()) != Some(entry)
            || path.extension().is_none_or(|extension| extension != "css")
            || entry.contains('\\')
        {
            bail!("component CSS entry must be a CSS filename: '{entry}'");
        }
        for directory in ["styles/publications", "styles/entries"] {
            let source = Path::new(directory).join(entry);
            if self.files.contains_key(&source) {
                return Ok(source);
            }
        }
        bail!("missing required component CSS entry '{entry}'")
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    #[test]
    fn publication_css_is_opt_in_and_only_bound_entries_are_compiled() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let theme = root.path().join("theme");
        for directory in [
            project.clone(),
            theme.join("defaults"),
            theme.join("shared/styles/entries"),
            theme.join("shared/styles/publications"),
        ] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::write(
            theme.join("shared/styles/entries/site.css"),
            "body {color:blue}",
        )
        .unwrap();
        fs::write(
            theme.join("shared/styles/publications/portfolio.css"),
            "body {color:red}",
        )
        .unwrap();
        fs::write(
            theme.join("shared/styles/entries/site.css"),
            "@import '../missing.css';",
        )
        .unwrap();
        let snapshot = Theme::load(&project, &theme).unwrap().snapshot().unwrap();
        let composed = root.path().join("composed");
        let required = BTreeSet::from(["portfolio.css".into()]);
        let prepared = snapshot.prepare_assets(&required, None).unwrap();
        assert_eq!(prepared.consumed_inputs().len(), 1);
        assert_eq!(
            prepared.consumed_inputs()[0].logical_name,
            "styles/publications/portfolio.css"
        );
        prepared.write(&composed).unwrap();
        assert!(composed.join("assets/css/portfolio.css").is_file());
        assert!(!composed.join("assets/css/site.css").exists());
        for invalid in [
            "../portfolio.css",
            "/portfolio.css",
            "nested/file.css",
            "nested\\file.css",
            ".",
            "portfolio.txt",
        ] {
            assert!(
                snapshot
                    .prepare_assets(&BTreeSet::from([invalid.into()]), None)
                    .is_err(),
                "{invalid}"
            );
        }
    }
    #[test]
    fn publication_theme_uses_captured_bytes_after_files_change() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let theme = root.path().join("theme");
        for path in [
            project.clone(),
            theme.join("defaults"),
            theme.join("shared/pages"),
            theme.join("shared/styles/publications"),
            theme.join("shared/static"),
        ] {
            fs::create_dir_all(path).unwrap();
        }
        let template = theme.join("shared/pages/example.tera");
        let css = theme.join("shared/styles/publications/portfolio.css");
        let asset = theme.join("shared/static/example.txt");
        fs::write(&template, "old template").unwrap();
        fs::write(&css, "body {color:red}").unwrap();
        fs::write(&asset, "old asset").unwrap();
        let snapshot = Theme::load(&project, &theme).unwrap().snapshot().unwrap();
        fs::write(&template, "new template").unwrap();
        fs::write(&css, "body {color:blue}").unwrap();
        fs::write(&asset, "new asset").unwrap();
        assert_eq!(
            snapshot.component_templates().unwrap()["example.tera"],
            "old template"
        );
        let inputs = snapshot.consumed_inputs();
        assert_eq!(
            inputs
                .iter()
                .find(|input| input.logical_name == "pages/example.tera")
                .unwrap()
                .sha256,
            berlin_publication::input_digest(b"old template")
        );
        let output = root.path().join("staged");
        snapshot
            .prepare_assets(&BTreeSet::from(["portfolio.css".into()]), None)
            .unwrap()
            .write(&output)
            .unwrap();
        assert_eq!(
            fs::read_to_string(output.join("static/example.txt")).unwrap(),
            "old asset"
        );
        let rendered = fs::read_to_string(output.join("assets/css/portfolio.css")).unwrap();
        assert!(rendered.contains("red"));
        assert!(!rendered.contains("blue"));
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
        let snapshot = theme.snapshot().unwrap();
        let mut templates = tera::Tera::default();
        templates.autoescape_on(vec![".tera"]);
        templates
            .add_raw_templates(snapshot.component_templates().unwrap())
            .unwrap();
        assert_eq!(
            templates
                .render("index.tera", &tera::Context::new())
                .unwrap(),
            "<article>Theme</article>"
        );
        let output = root.path().join("output");
        snapshot
            .prepare_assets(
                &BTreeSet::from(["notebook.css".into(), "article.css".into()]),
                None,
            )
            .unwrap()
            .write(&output)
            .unwrap();
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
            !snapshot
                .paths
                .values()
                .any(|path| path == &root.path().join("theme/shared/styles/tokens.css"))
        );
        assert!(
            snapshot
                .paths
                .values()
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
                .snapshot()
                .unwrap()
                .prepare_assets(&BTreeSet::from(["notebook.css".into()]), None)
                .err()
                .unwrap()
                .to_string()
                .contains("missing required component CSS entry")
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
