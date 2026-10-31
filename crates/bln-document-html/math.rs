//! Prepare each distinct equation once before writing any document HTML.
use berlin_document::{Block, Document, Inline};
use berlin_math::{DisplayMode, Error, MathMl};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct MathRenderings {
    inline: HashMap<String, MathMl>,
    display: HashMap<String, MathMl>,
}

impl MathRenderings {
    pub fn for_document(document: &Document) -> Result<Self, Error> {
        Self::for_blocks(&document.blocks)
    }

    pub fn for_blocks(blocks: &[Block]) -> Result<Self, Error> {
        let mut rendered = Self::default();
        rendered.blocks(blocks)?;
        Ok(rendered)
    }

    pub fn is_empty(&self) -> bool {
        self.inline.is_empty() && self.display.is_empty()
    }

    pub fn get(&self, tex: &str, display: bool) -> &str {
        let map = if display { &self.display } else { &self.inline };
        map.get(tex)
            .expect("document equations were prepared before HTML rendering")
            .as_str()
    }

    fn insert(&mut self, tex: &str, display: bool) -> Result<(), Error> {
        let map = if display {
            &mut self.display
        } else {
            &mut self.inline
        };
        if !map.contains_key(tex) {
            let mode = if display {
                DisplayMode::Block
            } else {
                DisplayMode::Inline
            };
            map.insert(tex.to_owned(), berlin_math::render(tex, mode)?);
        }
        Ok(())
    }

    fn blocks(&mut self, blocks: &[Block]) -> Result<(), Error> {
        for block in blocks {
            match block {
                Block::Math { tex } => self.insert(tex, true)?,
                Block::Equation(equation) => self.insert(&equation.tex, true)?,
                Block::Paragraph { content } | Block::Heading { content, .. } => {
                    self.inlines(content)?
                }
                Block::Section { title, blocks, .. } => {
                    self.inlines(title)?;
                    self.blocks(blocks)?;
                }
                Block::BlockQuote { blocks }
                | Block::Component { blocks, .. }
                | Block::FootnoteDefinition { blocks, .. } => self.blocks(blocks)?,
                Block::List(list) => {
                    for item in &list.items {
                        self.blocks(&item.blocks)?;
                    }
                }
                Block::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            self.inlines(cell)?;
                        }
                    }
                }
                Block::Code(code) => {
                    if let Some(caption) = &code.caption {
                        self.inlines(caption)?;
                    }
                }
                Block::Figure(_) | Block::Html { .. } | Block::ThematicBreak => {}
            }
        }
        Ok(())
    }

    fn inlines(&mut self, inlines: &[Inline]) -> Result<(), Error> {
        for inline in inlines {
            match inline {
                Inline::Math { tex } => self.insert(tex, false)?,
                Inline::Emphasis { content }
                | Inline::Strong { content }
                | Inline::Strikethrough { content }
                | Inline::CrossReference { content, .. }
                | Inline::Link { content, .. } => self.inlines(content)?,
                Inline::DocumentLink(link) => self.inlines(&link.content)?,
                // Image descriptions are plain text, not typeset mathematics.
                Inline::Text { .. }
                | Inline::Code { .. }
                | Inline::Image { .. }
                | Inline::Html { .. }
                | Inline::SoftBreak
                | Inline::LineBreak
                | Inline::FootnoteReference { .. } => {}
            }
        }
        Ok(())
    }
}
