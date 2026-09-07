//! Reference integrity for named code listings and their exported line anchors.

use crate::{Block, DocumentValidationError, Inline};
use std::collections::HashSet;

pub(super) fn validate(blocks: &[Block]) -> Result<(), DocumentValidationError> {
    let mut ids = HashSet::new();
    let mut prefixes = Vec::new();
    let mut links = Vec::new();
    collect(blocks, &mut ids, &mut prefixes, &mut links)?;
    for link in links {
        if let Some(target) = link.strip_prefix('#')
            && (target.starts_with("org-coderef--")
                || prefixes
                    .iter()
                    .any(|prefix| target.starts_with(&format!("{prefix}-"))))
            && !ids.contains(target)
        {
            return Err(invalid(format!("unknown line anchor '{target}'")));
        }
    }
    Ok(())
}

fn invalid(message: String) -> DocumentValidationError {
    DocumentValidationError::InvalidCodeReference(message)
}

fn insert(ids: &mut HashSet<String>, id: &str) -> Result<(), DocumentValidationError> {
    if !ids.insert(id.into()) {
        return Err(invalid(format!("duplicate anchor '{id}'")));
    }
    Ok(())
}

fn identifier(id: &str) -> Result<(), DocumentValidationError> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
    {
        return Err(invalid(format!(
            "anchor '{id}' must use letters, digits, '-', '_', '.', or ':'"
        )));
    }
    Ok(())
}

fn collect<'a>(
    blocks: &'a [Block],
    ids: &mut HashSet<String>,
    prefixes: &mut Vec<&'a str>,
    links: &mut Vec<&'a str>,
) -> Result<(), DocumentValidationError> {
    for block in blocks {
        match block {
            Block::Code(code) => {
                if let Some(caption) = &code.caption {
                    inline_links(caption, links);
                }
                let references = &code.references;
                if let Some(id) = &references.id {
                    identifier(id)?;
                    insert(ids, id)?;
                }
                let count = code.value.lines().count();
                for range in &code.highlights {
                    if range.start == 0 || range.start > range.end || range.end > count {
                        return Err(invalid(format!(
                            "highlight range {}-{} is outside this {count}-line listing",
                            range.start, range.end
                        )));
                    }
                }
                if references.first_line == 0 || references.first_line.checked_add(count).is_none()
                {
                    return Err(invalid("line numbering is zero or overflows".into()));
                }
                if let Some(prefix) = &references.line_anchor_prefix {
                    identifier(prefix)?;
                    prefixes.push(prefix);
                    for line in references.first_line..references.first_line + count {
                        insert(ids, &format!("{prefix}-{line}"))?;
                    }
                }
            }
            Block::Section {
                id, title, blocks, ..
            } => {
                insert(ids, &id.0)?;
                inline_links(title, links);
                collect(blocks, ids, prefixes, links)?;
            }
            Block::Component { id, blocks, .. } => {
                insert(ids, &id.0)?;
                collect(blocks, ids, prefixes, links)?;
            }
            Block::Heading { id, content, .. } => {
                if let Some(id) = id {
                    insert(ids, id)?;
                }
                inline_links(content, links);
            }
            Block::Paragraph { content } => inline_links(content, links),
            Block::BlockQuote { blocks } | Block::FootnoteDefinition { blocks, .. } => {
                collect(blocks, ids, prefixes, links)?
            }
            Block::List(list) => {
                for item in &list.items {
                    collect(&item.blocks, ids, prefixes, links)?;
                }
            }
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        inline_links(cell, links);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn inline_links<'a>(inlines: &'a [Inline], links: &mut Vec<&'a str>) {
    for inline in inlines {
        match inline {
            Inline::Link {
                destination,
                content,
                ..
            } => {
                links.push(destination);
                inline_links(content, links);
            }
            Inline::Emphasis { content }
            | Inline::Strong { content }
            | Inline::Strikethrough { content } => inline_links(content, links),
            Inline::Image { description, .. } => inline_links(description, links),
            _ => {}
        }
    }
}
