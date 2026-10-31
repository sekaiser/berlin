use std::path::{Path, PathBuf};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

use anyhow::Context as _;
use anyhow::Error;
use anyhow::anyhow;
use lightningcss::bundler::Bundler;
use lightningcss::bundler::FileProvider;
use lightningcss::bundler::{ResolveResult, SourceProvider};
use lightningcss::css_modules::Config;
use lightningcss::css_modules::Pattern;
use lightningcss::stylesheet::MinifyOptions;
use lightningcss::stylesheet::ParserOptions;
use lightningcss::stylesheet::PrinterOptions;

mod cache;

#[derive(Clone, Copy, serde::Serialize)]
enum Mode {
    Modules,
    Stylesheet,
}

impl Mode {
    fn options<'a>(self) -> Result<ParserOptions<'a>, Error> {
        match self {
            Self::Modules => parser_options(),
            Self::Stylesheet => Ok(ParserOptions::default()),
        }
    }
}

pub(super) fn compile(
    source: &Path,
    root: &Path,
    cache: Option<&super::cache::ArtifactCache>,
) -> Result<String, Error> {
    let fs = FileProvider::new();
    cache::compile(source, root, &fs, Mode::Modules, cache)
}

/// Theme styles use ordinary CSS, not CSS modules. Remote imports remain browser
/// imports; local imports must stay within the merged stylesheet workspace.
#[cfg(test)]
fn compile_stylesheet(
    source: &Path,
    root: &Path,
    cache: Option<&super::cache::ArtifactCache>,
) -> Result<String, Error> {
    let sources = StylesheetSources {
        files: FileProvider::new(),
        root: root.canonicalize()?,
    };
    cache::compile(source, root, &sources, Mode::Stylesheet, cache)
}

/// Bundle only captured bytes, with a recorded local import closure. No working
/// directory, temporary stylesheet tree, network request or source reread.
pub(super) fn compile_stylesheet_snapshot(
    source: &Path,
    files: &BTreeMap<PathBuf, Vec<u8>>,
    cache: Option<&super::cache::ArtifactCache>,
) -> Result<(String, BTreeSet<PathBuf>), Error> {
    struct Sources<'a> {
        files: &'a BTreeMap<PathBuf, Vec<u8>>,
        read: Mutex<BTreeSet<PathBuf>>,
    }
    impl SourceProvider for Sources<'_> {
        type Error = std::io::Error;
        fn read<'a>(&'a self, file: &Path) -> Result<&'a str, Self::Error> {
            let bytes = self.files.get(file).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("missing captured stylesheet: {}", file.display()),
                )
            })?;
            let text = std::str::from_utf8(bytes)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            self.read
                .lock()
                .map_err(|_| std::io::Error::other("stylesheet input tracking lock poisoned"))?
                .insert(file.to_owned());
            Ok(text)
        }
        fn resolve(
            &self,
            specifier: &str,
            originating_file: &Path,
        ) -> Result<ResolveResult, Self::Error> {
            if specifier.starts_with('/') || url::Url::parse(specifier).is_ok() {
                return Ok(ResolveResult::External(specifier.into()));
            }
            let denied = || {
                std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "CSS import leaves captured stylesheet directory",
                )
            };
            if specifier.contains('\\') {
                return Err(denied());
            }
            let joined = originating_file
                .parent()
                .ok_or_else(denied)?
                .join(specifier);
            let mut path = PathBuf::new();
            for component in joined.components() {
                match component {
                    std::path::Component::Normal(part) => path.push(part),
                    std::path::Component::CurDir => {}
                    std::path::Component::ParentDir => {
                        if !path.pop() {
                            return Err(denied());
                        }
                    }
                    _ => return Err(denied()),
                }
            }
            if !path.starts_with("styles") {
                return Err(denied());
            }
            Ok(ResolveResult::File(path))
        }
    }
    let source_provider = Sources {
        files,
        read: Mutex::new(BTreeSet::new()),
    };
    let css = cache::compile_snapshot(source, &source_provider, cache, || {
        // Failed dependency-hint probes are not part of the new import closure.
        if let Ok(mut read) = source_provider.read.lock() {
            read.clear();
        }
    })?;
    let read = source_provider
        .read
        .into_inner()
        .map_err(|_| anyhow!("stylesheet input tracking lock poisoned"))?;
    Ok((css, read))
}

fn compile_with<'a, P: SourceProvider>(
    source: &Path,
    fs: &'a P,
    options: ParserOptions<'a>,
) -> Result<String, Error> {
    let mut bundler = Bundler::new(fs, None, options);
    let mut stylesheet = bundler
        .bundle(source)
        // Bundler errors may borrow the file provider; retain their diagnostic text.
        .map_err(|error| anyhow!(error.to_string()))
        .with_context(|| format!("Unable to bundle CSS {}", source.display()))?;
    stylesheet
        .minify(MinifyOptions::default())
        .with_context(|| format!("Unable to minify CSS {}", source.display()))?;
    let compiled = stylesheet
        .to_css(PrinterOptions {
            minify: true,
            ..PrinterOptions::default()
        })
        .with_context(|| format!("Unable to serialize CSS {}", source.display()))?;
    Ok(compiled.code)
}

#[cfg(test)]
struct StylesheetSources {
    files: FileProvider,
    root: PathBuf,
}

#[cfg(test)]
impl SourceProvider for StylesheetSources {
    type Error = std::io::Error;

    fn read<'a>(&'a self, file: &Path) -> Result<&'a str, Self::Error> {
        self.files.read(file)
    }

    fn resolve(
        &self,
        specifier: &str,
        originating_file: &Path,
    ) -> Result<ResolveResult, Self::Error> {
        if specifier.starts_with('/') || url::Url::parse(specifier).is_ok() {
            return Ok(ResolveResult::External(specifier.to_owned()));
        }
        let path = originating_file.with_file_name(specifier).canonicalize()?;
        if !path.starts_with(&self.root) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("CSS import leaves the stylesheet directory: {specifier}"),
            ));
        }
        Ok(ResolveResult::File(path))
    }
}

fn parser_options<'i>() -> Result<ParserOptions<'i>, Error> {
    Ok(ParserOptions {
        css_modules: Some(Config {
            pattern: Pattern::parse("[local]")?,
            dashed_idents: true,
            ..Config::default()
        }),
        ..ParserOptions::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn captured_cache_reuses_bytes_and_discards_failed_hint_reads() {
        let directory = tempfile::tempdir().unwrap();
        let cache = super::super::cache::ArtifactCache::new(directory.path());
        let mut files = BTreeMap::from([
            (
                PathBuf::from("styles/entry.css"),
                b"@import 'a.css'; body {color:green}".to_vec(),
            ),
            (PathBuf::from("styles/a.css"), b".a {color:red}".to_vec()),
        ]);
        let cold = compile_stylesheet_snapshot(Path::new("styles/entry.css"), &files, Some(&cache))
            .unwrap();
        assert_eq!(
            cold,
            compile_stylesheet_snapshot(Path::new("styles/entry.css"), &files, Some(&cache))
                .unwrap()
        );
        files.insert(
            PathBuf::from("styles/entry.css"),
            b"body {--revision:fresh-snapshot;color:blue}".to_vec(),
        );
        let (css, read) =
            compile_stylesheet_snapshot(Path::new("styles/entry.css"), &files, Some(&cache))
                .unwrap();
        assert!(css.contains("fresh-snapshot"));
        assert_eq!(read, BTreeSet::from([PathBuf::from("styles/entry.css")]));
        assert_eq!(
            (css, read),
            compile_stylesheet_snapshot(Path::new("styles/entry.css"), &files, Some(&cache))
                .unwrap()
        );
    }

    #[test]
    fn snapshot_bundles_only_captured_imports_and_records_the_closure() {
        let files = BTreeMap::from([
            (
                PathBuf::from("styles/entries/project.css"),
                b"@import '../tokens.css'; .example { color: var(--ink); }".to_vec(),
            ),
            (
                PathBuf::from("styles/tokens.css"),
                b":root { --ink: green; }".to_vec(),
            ),
            (
                PathBuf::from("styles/unused.css"),
                b"invalid unused stylesheet {{{".to_vec(),
            ),
        ]);
        let (css, read) =
            compile_stylesheet_snapshot(Path::new("styles/entries/project.css"), &files, None)
                .unwrap();
        assert!(css.contains("--ink:green"));
        assert!(css.contains(".example{color:var(--ink)}"));
        assert_eq!(
            read,
            BTreeSet::from([
                PathBuf::from("styles/entries/project.css"),
                PathBuf::from("styles/tokens.css")
            ])
        );
    }

    #[test]
    fn snapshot_rejects_missing_imports_and_directory_escape() {
        for import in ["missing.css", "../../secret.css", "..\\secret.css"] {
            let files = BTreeMap::from([(
                PathBuf::from("styles/project.css"),
                format!("@import '{import}';").into_bytes(),
            )]);
            assert!(
                compile_stylesheet_snapshot(Path::new("styles/project.css"), &files, None).is_err(),
                "{import}"
            );
        }
    }

    #[test]
    fn snapshot_matches_filesystem_compilation_without_rereading_it() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("project.css");
        let original = b"@import url('https://example.test/base.css'); body { color: green; }";
        fs::write(&source, original).unwrap();
        let expected = compile_stylesheet(&source, directory.path(), None).unwrap();
        let files = BTreeMap::from([(PathBuf::from("styles/project.css"), original.to_vec())]);
        fs::write(&source, "body { color: red; }").unwrap();
        let (actual, _) =
            compile_stylesheet_snapshot(Path::new("styles/project.css"), &files, None).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn theme_entries_bundle_local_imports_and_preserve_browser_urls() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir(root.join("entries")).unwrap();
        fs::write(root.join("tokens.css"), ":root { --ink: green; }").unwrap();
        let entry = root.join("entries/project.css");
        fs::write(
            &entry,
            r#"
            @import url("https://example.invalid/base.css");
            @import "../tokens.css";
            @font-face { font-family: Test; src: url("../../static/fonts/font.woff2"); }
            :root { --art: url("../../static/pics/art.webp?v=1#image"); }
            .example { color: var(--ink); }
        "#,
        )
        .unwrap();
        let compiled = compile_stylesheet(&entry, root, None).unwrap();
        assert!(compiled.starts_with("@import"));
        assert!(compiled.contains("https://example.invalid/base.css"));
        assert!(compiled.contains("--ink:green"));
        assert!(!compiled.contains("../tokens.css"));
        assert!(compiled.contains("../../static/fonts/font.woff2"));
        assert!(compiled.contains("../../static/pics/art.webp?v=1#image"));
        assert!(compiled.contains(".example{color:var(--ink)}"));
    }

    #[test]
    fn theme_imports_cannot_escape_the_merged_styles_directory() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("styles");
        fs::create_dir(&root).unwrap();
        fs::write(directory.path().join("outside.css"), "body { color: red; }").unwrap();
        let entry = root.join("entry.css");
        fs::write(&entry, "@import '../outside.css';").unwrap();
        let error = compile_stylesheet(&entry, &root, None).unwrap_err();
        assert!(format!("{error:#}").contains("leaves the stylesheet directory"));
    }

    #[test]
    #[cfg(unix)]
    fn cache_hits_recheck_theme_import_confinement() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("styles");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("a.css"), ".a { color: red }").unwrap();
        fs::write(
            directory.path().join("outside.css"),
            ".private { color: blue }",
        )
        .unwrap();
        let alias = root.join("alias.css");
        std::os::unix::fs::symlink("a.css", &alias).unwrap();
        let entry = root.join("entry.css");
        fs::write(&entry, "@import 'alias.css';").unwrap();
        let cache = super::super::cache::ArtifactCache::new(directory.path());
        let cold = compile_stylesheet(&entry, &root, Some(&cache)).unwrap();
        assert_eq!(
            compile_stylesheet(&entry, &root, Some(&cache)).unwrap(),
            cold
        );
        fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink("../outside.css", alias).unwrap();
        let error = compile_stylesheet(&entry, &root, Some(&cache)).unwrap_err();
        assert!(format!("{error:#}").contains("leaves the stylesheet directory"));
    }
}
