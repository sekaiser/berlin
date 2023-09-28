use std::path::Path;

use errors::error::generic_error;
use libs::anyhow::Error;

use libs::lightningcss::{
    bundler::{Bundler, FileProvider},
    css_modules::{Config, Pattern},
    stylesheet::{MinifyOptions, ParserOptions, PrinterOptions, ToCssResult},
};

pub fn to_css<P: AsRef<Path>>(path: P) -> Result<ToCssResult, Error> {
    let fs = FileProvider::new();
    let parser_options = ParserOptions {
        css_modules: Some(Config {
            pattern: Pattern::parse("[local]")?,
            dashed_idents: true,
        }),
        ..ParserOptions::default()
    };
    let mut bundler = Bundler::new(&fs, None, parser_options);
    let mut stylesheet = bundler.bundle(path.as_ref()).unwrap();
    stylesheet.minify(MinifyOptions::default())?;
    stylesheet
        .to_css(PrinterOptions {
            minify: true,
            ..PrinterOptions::default()
        })
        .map_err(|e| generic_error(e.to_string()))
}
