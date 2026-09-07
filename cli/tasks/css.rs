use std::path::Path;

use anyhow::Context as _;
use anyhow::Error;
use anyhow::anyhow;
use lightningcss::bundler::Bundler;
use lightningcss::bundler::FileProvider;
use lightningcss::css_modules::Config;
use lightningcss::css_modules::Pattern;
use lightningcss::stylesheet::MinifyOptions;
use lightningcss::stylesheet::ParserOptions;
use lightningcss::stylesheet::PrinterOptions;

pub(super) fn compile(source: &Path) -> Result<String, Error> {
    let fs = FileProvider::new();
    let mut bundler = Bundler::new(&fs, None, parser_options()?);
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
