mod css;

use std::path::Path;
use std::sync::Arc;

use files::{MediaType, ModuleSpecifier};
use libs::anyhow::Error;
use parser::{ParsedSource, ParsedSourceBuilder, Parser};

pub use libs::lightningcss::error::PrinterErrorKind;
pub use libs::lightningcss::stylesheet::ToCssResult;

pub use css::to_css;

#[derive(Default, Clone)]
pub struct DefaultCssParser;

impl Parser for DefaultCssParser {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        _source: Arc<str>,
        _media_type: MediaType,
    ) -> Result<ParsedSource, Error> {
        let specifier_string = specifier.to_string();
        let specifier_path_string = specifier.path().to_string();
        let path = Path::new(&specifier_path_string);
        let res = css::to_css(path)?;
        let parsed_source = ParsedSourceBuilder::new(specifier_string, MediaType::Css)
            .content(res.code)
            .build();

        Ok(parsed_source)
    }
}
