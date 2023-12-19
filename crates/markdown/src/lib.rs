mod context;
mod markdown;
mod shortcode;

pub use context::RenderContext;
use files::{MediaType, ModuleSpecifier};
use libs::anyhow::Error;
pub use markdown::{handle_shortcodes, markdown_to_html, string_to_html, MarkdownOptions};
use std::{path::Path, sync::Arc};

use parser::{ParsedSource, ParsedSourceBuilder, Parser};

#[derive(Default, Clone)]
pub struct DefaultMarkdownParser;

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
