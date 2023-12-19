use files::{MediaType, ModuleSpecifier};
use libs::anyhow::Error;
use parser::{ParsedSource, ParsedSourceBuilder, Parser};
use std::sync::Arc;

#[derive(Default, Clone)]
pub struct DefaultCsvParser;

impl Parser for DefaultCsvParser {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        source: Arc<str>,
        _media_type: MediaType,
    ) -> Result<ParsedSource, Error> {
        let parsed_source = ParsedSourceBuilder::new(specifier.to_string(), MediaType::Csv)
            .content(source.to_string())
            .build();

        Ok(parsed_source)
    }
}
