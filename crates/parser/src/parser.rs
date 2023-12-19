use files::{MediaType, ModuleSpecifier};
use libs::anyhow::Error;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::ParsedSource;

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

// pub trait Store<K, V> {
//     type Key: K;
//     type Value: V;

//     fn set_value(&self, key: Self::Key, val: Self::Value) -> Option<Self::Value>;

//     fn get_value(&self, key: &Self::Key) -> Option<Self::Value>;
// }

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

// impl<'a> Parser for CapturingParser<'a> {
//     fn parse(
//         &self,
//         specifier: &ModuleSpecifier,
//         source: Arc<str>,
//         media_type: MediaType,
//     ) -> Result<ParsedSource, Error> {
//         if let Some(parsed_source) = self.get_from_store_if_matches(specifier, media_type) {
//             Ok(parsed_source)
//         } else {
//             let parsed_source = match media_type {
//                 MediaType::Org => {
//                     DefaultOrgParser::default().parse(specifier, source, media_type)?
//                 }
//                 MediaType::Markdown => {
//                     DefaultMarkdownParser::default().parse(specifier, source, media_type)?
//                 }
//                 MediaType::Tera => ParsedSourceBuilder::new(specifier.to_string(), media_type)
//                     .content(source.as_ref().to_string())
//                     .build(),
//                 MediaType::Csv => {
//                     DefaultCsvParser::default().parse(specifier, source, media_type)?
//                 }
//                 MediaType::Css => {
//                     DefaultCssParser::default().parse(specifier, source, media_type)?
//                 }
//                 _ => unreachable!("Type not supported."),
//             };

//             self.store
//                 .set_parsed_source(specifier.clone(), parsed_source.clone());

//             Ok(parsed_source)
//         }
//     }
// }

pub trait Parser {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        source: Arc<str>,
        media_type: MediaType,
    ) -> Result<ParsedSource, Error>;
}
