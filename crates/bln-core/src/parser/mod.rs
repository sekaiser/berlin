use css::DefaultCssParser;
use csv::DefaultCsvParser;
use files::{MediaType, ModuleSpecifier};
use libs::anyhow::Error;
use markdown::DefaultMarkdownParser;
use org::DefaultOrgParser;
use parser::{ParsedSource, ParsedSourceBuilder, ParsedSourceCache, ParsedSourceStore, Parser};
use std::{borrow::BorrowMut, sync::Arc};

pub trait ToCapturingParser {
    /// Creates a parser that will reuse a ParsedSource from the store
    /// if it exists, or else parse.
    fn as_capturing_parser(&self) -> CapturingParser;
}

impl ToCapturingParser for ParsedSourceCache {
    /// Creates a parser that will reuse a ParsedSource from the store
    /// if it exists, or else parse.
    fn as_capturing_parser(&self) -> CapturingParser {
        CapturingParser::new(None, &self.sources)
    }
}

pub struct CapturingParser<'a> {
    _parser: Option<&'a dyn Parser>,
    store: &'a dyn ParsedSourceStore,
}

impl<'a> CapturingParser<'a> {
    pub fn new(parser: Option<&'a dyn Parser>, store: &'a dyn ParsedSourceStore) -> Self {
        Self {
            _parser: parser,
            store,
        }
    }

    fn get_from_store_if_matches(
        &self,
        specifier: &ModuleSpecifier,
        _media_type: MediaType,
    ) -> Option<ParsedSource> {
        let parsed_source = self.store.get_parsed_source(specifier)?;
        Some(parsed_source)
    }
}

impl<'a> Parser for CapturingParser<'a> {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        source: Arc<str>,
        media_type: MediaType,
    ) -> Result<ParsedSource, Error> {
        if let Some(parsed_source) = self.get_from_store_if_matches(specifier, media_type) {
            Ok(parsed_source)
        } else {
            let parsed_source = match media_type {
                MediaType::Org => {
                    DefaultOrgParser::default().parse(specifier, source, media_type)?
                }
                MediaType::Markdown => {
                    DefaultMarkdownParser::default().parse(specifier, source, media_type)?
                }
                MediaType::Tera => ParsedSourceBuilder::new(specifier.to_string(), media_type)
                    .content(source.as_ref().to_string())
                    .build(),
                MediaType::Csv => {
                    DefaultCsvParser::default().parse(specifier, source, media_type)?
                }
                MediaType::Css => {
                    DefaultCssParser::default().parse(specifier, source, media_type)?
                }
                _ => unreachable!("Type not supported."),
            };

            self.store
                .set_parsed_source(specifier.clone(), parsed_source.clone());

            Ok(parsed_source)
        }
    }
}
