use std::path::{Path, PathBuf};

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

pub(super) fn compile(source: &Path) -> Result<String, Error> {
    let fs = FileProvider::new();
    compile_with(source, &fs, parser_options()?)
}

/// Theme styles use ordinary CSS, not CSS modules. Remote imports remain browser
/// imports; local imports must stay within the merged stylesheet workspace.
pub(super) fn compile_stylesheet(source: &Path, root: &Path) -> Result<String, Error> {
    let sources = StylesheetSources {
        files: FileProvider::new(),
        root: root.canonicalize()?,
    };
    compile_with(source, &sources, ParserOptions::default())
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

struct StylesheetSources {
    files: FileProvider,
    root: PathBuf,
}

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
        let compiled = compile_stylesheet(&entry, root).unwrap();
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
        let error = compile_stylesheet(&entry, &root).unwrap_err();
        assert!(format!("{error:#}").contains("leaves the stylesheet directory"));
    }
}
