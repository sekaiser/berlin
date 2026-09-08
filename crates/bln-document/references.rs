//! Authored document links and the passages in which they occur, without URLs.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{Block, ComponentId, ContentId, Document, DocumentValidationError, Inline};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentLink {
    pub target: ContentId,
    pub fragment: Option<String>,
    /// Location of this reference in its source document, not in its target.
    pub anchor: ComponentId,
    pub title: Option<String>,
    pub content: Vec<Inline>,
}

pub struct ReferenceOccurrence<'a> {
    pub link: &'a DocumentLink,
    /// Traversal-local passage identity, used to group mentions without comparing prose.
    pub passage: usize,
    /// The authored containing paragraph, heading, caption or table cell as text.
    pub excerpt: String,
    /// Nearest semantic section, retained when Rhai changes its displayed title.
    pub section: Option<&'a ComponentId>,
}

impl Document {
    pub fn references(&self) -> Vec<ReferenceOccurrence<'_>> {
        let mut references = Vec::new();
        let mut passage = 0;
        visit_passages(&self.blocks, None, &mut |content, section| {
            passage += 1;
            let excerpt = plain_text(content)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            visit_links(content, &mut |link| {
                references.push(ReferenceOccurrence {
                    link,
                    passage,
                    excerpt: excerpt.clone(),
                    section,
                })
            });
        });
        references
    }
}

pub(super) fn validate(
    document: &Document,
    mut anchors: HashSet<String>,
) -> Result<(), DocumentValidationError> {
    for reference in document.references() {
        let link = reference.link;
        super::validate_identifier("document link target", &link.target.0)?;
        if let Some(fragment) = &link.fragment {
            super::validate_identifier("document link fragment", fragment)?;
        }
        if link.anchor.0.is_empty()
            || !link
                .anchor
                .0
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
            || !anchors.insert(link.anchor.0.clone())
        {
            return Err(DocumentValidationError::InvalidDocumentReference(format!(
                "invalid or duplicate source anchor '{}'",
                link.anchor.0
            )));
        }
    }
    Ok(())
}

fn visit_passages<'a>(
    blocks: &'a [Block],
    section: Option<&'a ComponentId>,
    visitor: &mut impl FnMut(&'a [Inline], Option<&'a ComponentId>),
) {
    for block in blocks {
        match block {
            Block::Paragraph { content } | Block::Heading { content, .. } => {
                visitor(content, section)
            }
            Block::Section {
                id, title, blocks, ..
            } => {
                visitor(title, Some(id));
                visit_passages(blocks, Some(id), visitor);
            }
            Block::Component { blocks, .. }
            | Block::BlockQuote { blocks }
            | Block::FootnoteDefinition { blocks, .. } => visit_passages(blocks, section, visitor),
            Block::List(list) => {
                for item in &list.items {
                    visit_passages(&item.blocks, section, visitor);
                }
            }
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        visitor(cell, section);
                    }
                }
            }
            Block::Code(code) => {
                if let Some(caption) = &code.caption {
                    visitor(caption, section);
                }
            }
            Block::Figure(_) | Block::Html { .. } | Block::ThematicBreak => {}
        }
    }
}

fn visit_links<'a>(inlines: &'a [Inline], visitor: &mut impl FnMut(&'a DocumentLink)) {
    for inline in inlines {
        match inline {
            Inline::DocumentLink(link) => visitor(link),
            Inline::Emphasis { content }
            | Inline::Strong { content }
            | Inline::Strikethrough { content }
            | Inline::Link { content, .. } => visit_links(content, visitor),
            _ => {}
        }
    }
}

/// Plain text only: raw HTML is not copied into reference excerpts.
pub fn plain_text(inlines: &[Inline]) -> String {
    let mut text = String::new();
    for inline in inlines {
        match inline {
            Inline::Text { value } | Inline::Code { value } => text.push_str(value),
            Inline::Emphasis { content }
            | Inline::Strong { content }
            | Inline::Strikethrough { content }
            | Inline::Link { content, .. } => text.push_str(&plain_text(content)),
            Inline::DocumentLink(link) => text.push_str(&plain_text(&link.content)),
            Inline::Image { description, .. } => text.push_str(&plain_text(description)),
            Inline::SoftBreak | Inline::LineBreak => text.push(' '),
            Inline::Html { .. } | Inline::FootnoteReference { .. } => {}
        }
    }
    text
}
