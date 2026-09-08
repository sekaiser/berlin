//! Orchestration of Markdown normalization, parsing, and semantic validation.

use std::collections::HashMap;

use berlin_document::Block;
use berlin_document::Document;
use berlin_document::Figure;
use berlin_document::Inline;
use comrak::Arena;
use comrak::Options as ComrakOptions;
use comrak::parse_document;

use crate::Error;
use crate::Source;
use crate::document;
use crate::front_matter;
use crate::options::berlin_markdown_options;
use crate::shortcode;

/// Parser for Berlin's Ox-Hugo-compatible Markdown dialect.
pub struct Parser {
    options: ComrakOptions<'static>,
}

impl Parser {
    /// Creates a parser with Berlin's canonical Markdown configuration.
    pub fn new() -> Self {
        Self {
            options: berlin_markdown_options(),
        }
    }

    /// Parses a validated source into Berlin's semantic document model.
    pub fn parse(&self, source: &Source<'_>) -> Result<Document, Error> {
        let (normalized, figures) = normalize_shortcodes(source)?;
        let arena = Arena::new();
        let root = parse_document(&arena, &normalized, &self.options);
        let front_matter = front_matter::parse(root, source.uri().as_str())?;
        let mut document =
            document::from_ast(root, front_matter.as_ref(), source).map_err(|message| {
                Error::InvalidDocument {
                    source_uri: source.uri().to_string(),
                    message,
                }
            })?;
        restore_figures(&mut document.blocks, &figures);
        document::assign_reference_anchors(&mut document.blocks);
        validate(document, source)
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_shortcodes(source: &Source<'_>) -> Result<(String, HashMap<String, Figure>), Error> {
    let mut normalized = source.text().to_owned();
    let mut figures = HashMap::new();
    let mut shortcodes = shortcode::parse(source)?;
    shortcodes.reverse();

    for (index, shortcode) in shortcodes.into_iter().enumerate() {
        let original = &source.text()[shortcode.span.clone()];
        if original.starts_with("{{</*") {
            continue;
        }

        match shortcode.name.as_str() {
            "relref" => {
                if let Some(destination) = shortcode.document_link.or(shortcode.body) {
                    normalized.replace_range(shortcode.span, &destination);
                }
            }
            "figure" => {
                let Some(image_source) = shortcode.args.get("src").and_then(|value| value.as_str())
                else {
                    continue;
                };
                let token = format!("BERLINFIGURETOKEN{index}");
                figures.insert(
                    token.clone(),
                    Figure {
                        source: image_source.to_owned(),
                        caption: shortcode
                            .args
                            .get("caption")
                            .and_then(|caption| caption.as_str())
                            .map(str::to_owned),
                    },
                );
                normalized.replace_range(shortcode.span, &token);
            }
            _ => {}
        }
    }

    Ok((normalized, figures))
}

fn restore_figures(blocks: &mut [Block], figures: &HashMap<String, Figure>) {
    for block in blocks {
        match block {
            Block::Paragraph { content } => {
                let Some(Inline::Text { value }) = content.first() else {
                    continue;
                };
                if content.len() == 1
                    && let Some(figure) = figures.get(value)
                {
                    *block = Block::Figure(figure.clone());
                }
            }
            Block::BlockQuote { blocks }
            | Block::FootnoteDefinition { blocks, .. }
            | Block::Section { blocks, .. }
            | Block::Component { blocks, .. } => {
                restore_figures(blocks, figures);
            }
            Block::List(list) => {
                for item in &mut list.items {
                    restore_figures(&mut item.blocks, figures);
                }
            }
            _ => {}
        }
    }
}

fn validate(document: Document, source: &Source<'_>) -> Result<Document, Error> {
    document
        .validate()
        .map_err(|error| Error::InvalidDocument {
            source_uri: source.uri().to_string(),
            message: error.to_string(),
        })?;
    Ok(document)
}
