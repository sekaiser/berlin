use berlin_core::ModuleSpecifier;
use berlin_core::ParsedSource;
use libs::parking_lot::Mutex;

use parser::{CapturingParser, ParsedSourceStore};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Default)]
struct ParsedSourceCacheSources(Arc<Mutex<HashMap<ModuleSpecifier, ParsedSource>>>);

impl ParsedSourceStore for ParsedSourceCacheSources {
    fn set_parsed_source(
        &self,
        specifier: ModuleSpecifier,
        parsed_source: ParsedSource,
    ) -> Option<ParsedSource> {
        self.0.lock().insert(specifier, parsed_source)
    }

    fn get_parsed_source(&self, specifier: &ModuleSpecifier) -> Option<ParsedSource> {
        self.0.lock().get(specifier).cloned()
    }
}

#[derive(Clone)]
pub struct ParsedSourceCache {
    _db_cache_path: Option<PathBuf>,
    sources: ParsedSourceCacheSources,
}

impl ParsedSourceCache {
    pub fn new(sql_cache_path: Option<PathBuf>) -> Self {
        Self {
            _db_cache_path: sql_cache_path,
            sources: Default::default(),
        }
    }

    /// Frees the parsed source from memory.
    pub fn free(&self, specifier: &ModuleSpecifier) {
        self.sources.0.lock().remove(specifier);
    }

    /// Creates a parser that will reuse a ParsedSource from the store
    /// if it exists, or else parse.
    pub fn as_capturing_parser(&self) -> CapturingParser {
        CapturingParser::new(None, &self.sources)
    }
}
