use berlin_core::{MediaType, ModuleSpecifier, ParsedSource, ParsedSourceBuilder};
use errors::anyhow::Error;
use errors::error::generic_error;
use markdown::handle_shortcodes;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Stores parsed sources.
///
/// Note: This interface is racy and not thread safe, as it's assumed
/// it will only store the latest changes or that the source text
/// will never change.
pub trait ParsedSourceStore {
    /// Sets the parsed source, potentially returning the previous value.
    fn set_parsed_source(
        &self,
        specifier: ModuleSpecifier,
        parsed_source: ParsedSource,
    ) -> Option<ParsedSource>;

    fn get_parsed_source(&self, specifier: &ModuleSpecifier) -> Option<ParsedSource>;
}

/// Default store that works on a single thread.
#[derive(Default)]
pub struct DefaultParsedSourceStore {
    store: RefCell<HashMap<ModuleSpecifier, ParsedSource>>,
}

impl ParsedSourceStore for DefaultParsedSourceStore {
    fn set_parsed_source(
        &self,
        specifier: ModuleSpecifier,
        parsed_source: ParsedSource,
    ) -> Option<ParsedSource> {
        self.store.borrow_mut().insert(specifier, parsed_source)
    }

    fn get_parsed_source(&self, specifier: &ModuleSpecifier) -> Option<ParsedSource> {
        self.store.borrow().get(specifier).cloned()
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

pub trait Parser {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        source: Arc<str>,
        media_type: MediaType,
    ) -> Result<ParsedSource, Error>;
}

#[derive(Default, Clone)]
pub struct DefaultOrgParser;

#[derive(Default, Clone)]
pub struct DefaultMarkdownParser;

#[derive(Default, Clone)]
pub struct DefaultCssParser;

#[derive(Default, Clone)]
pub struct DefaultCsvParser;

impl Parser for DefaultOrgParser {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        source: Arc<str>,
        media_type: MediaType,
    ) -> Result<ParsedSource, Error> {
        match org::parse(source) {
            Ok(data) => DefaultMarkdownParser {}.parse(specifier, Arc::from(data), media_type),
            Err(e) => Err(generic_error(format!(
                "Cannot convert file {} to {}\nReason: {}",
                specifier,
                MediaType::Markdown,
                e.to_string()
            ))),
        }
    }
}

impl Parser for DefaultMarkdownParser {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        source: Arc<str>,
        _media_type: MediaType,
    ) -> Result<ParsedSource, Error> {
        // preprocess source
        let mut content = source.to_string();
        handle_shortcodes(&specifier, &mut content);

        // process source
        let (maybe_front_matter, data) =
            markdown::markdown_to_html(Arc::from(content), markdown::MarkdownOptions::default());
        let metadata = std::fs::metadata(Path::new(specifier.path()))?;
        let parsed_source = ParsedSourceBuilder::new(specifier.to_string(), MediaType::Html)
            .content(String::from_utf8(data).unwrap())
            .maybe_front_matter(maybe_front_matter)
            .metadata(metadata)
            .build();
        Ok(parsed_source)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let specifier = ModuleSpecifier::parse("file:///a/test.org").expect("bad specifier");
        let source = r#"
        #+TITLE: Test

        This is the example org-mode file used for the Denver Emacs org-mode
        introduction talk. Everything in this talk should work without any custom
        settings or installation with a reasonably recent Emacs version.

        Let's start with a headline, this is kind of like Markdown's # character:

        * This is an example headline

          Text can be put into the headline. You can create another headline at the same
          level with another * character
        "#;
        let parsed_source = DefaultOrgParser::default()
            .parse(&specifier, source.into(), MediaType::Org)
            .unwrap();
        print!("{:?}", parsed_source.data());
    }
}
