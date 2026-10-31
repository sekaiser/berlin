//! Additive, fallible semantic text seam for portable fragments. Legacy drafts
//! continue using their existing projection and byte fixtures unchanged.
use std::collections::HashMap;
use std::io::{self, Write};

use berlin_document::{Block, Inline, PropertyValue};

#[derive(Default)]
pub struct Context {
    pub destinations: HashMap<String, String>,
    pub document_urls: HashMap<String, String>,
}
impl Context {
    fn url<'a>(&'a self, original: &'a str) -> &'a str {
        self.destinations
            .get(original)
            .map_or(original, String::as_str)
    }
}

pub fn write_blocks(
    blocks: &[Block],
    context: &Context,
    output: &mut impl Write,
) -> io::Result<()> {
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            output.write_all(b"\n\n")?;
        }
        match block {
            Block::Paragraph { content } | Block::Heading { content, .. } => {
                write_inlines(content, context, output)?
            }
            Block::Section { title, blocks, .. } => {
                write_inlines(title, context, output)?;
                output.write_all(b"\n\n")?;
                write_blocks(blocks, context, output)?;
            }
            Block::BlockQuote { blocks } => {
                output.write_all(b"> ")?;
                write_blocks(blocks, context, output)?;
            }
            Block::Component {
                name,
                properties,
                blocks,
                ..
            } => {
                if name == "bibliography-entry" {
                    if !properties.is_empty() || blocks.is_empty() {
                        return Err(io::Error::other(
                            "InvalidProperty: invalid bibliography entry",
                        ));
                    }
                    write_blocks(blocks, context, output)?;
                    continue;
                }
                if name != "berlin.callout@1" {
                    return Err(io::Error::other(format!(
                        "UnsupportedChannelComponent: {name}"
                    )));
                }
                let Some(PropertyValue::Text(kind)) = properties.get("kind") else {
                    return Err(io::Error::other(
                        "InvalidProperty: callout kind is required",
                    ));
                };
                if !["note", "warning", "tip"].contains(&kind.as_str()) {
                    return Err(io::Error::other("InvalidProperty: unknown callout kind"));
                }
                write!(output, "[{kind}]")?;
                if let Some(PropertyValue::Text(title)) = properties.get("title") {
                    write!(output, " {title}")?;
                }
                output.write_all(b"\n")?;
                write_blocks(blocks, context, output)?;
            }
            Block::Code(code) => {
                if let Some(caption) = &code.caption {
                    write_inlines(caption, context, output)?;
                    output.write_all(b"\n\n")?;
                }
                output.write_all(code.value.as_bytes())?;
            }
            Block::Figure(figure) => {
                if let Some(caption) = &figure.caption {
                    writeln!(output, "{caption}")?;
                }
                output.write_all(context.url(&figure.source).as_bytes())?;
            }
            Block::List(list) => {
                for (index, item) in list.items.iter().enumerate() {
                    if index > 0 {
                        output.write_all(b"\n")?;
                    }
                    if list.ordered {
                        write!(output, "{}. ", list.start.unwrap_or(1) + index)?;
                    } else {
                        output.write_all(b"- ")?;
                    }
                    if let Some(checked) = item.checked {
                        write!(output, "[{}] ", if checked { 'x' } else { ' ' })?;
                    }
                    write_blocks(&item.blocks, context, output)?;
                }
            }
            Block::Table(table) => {
                for (row_index, row) in table.rows.iter().enumerate() {
                    if row_index > 0 {
                        output.write_all(b"\n")?;
                    }
                    for (cell_index, cell) in row.cells.iter().enumerate() {
                        if cell_index > 0 {
                            output.write_all(b" | ")?;
                        }
                        write_inlines(cell, context, output)?;
                    }
                }
            }
            Block::Math { tex } => write!(output, "\\[{tex}\\]")?,
            Block::Equation(equation) => {
                write!(output, "\\[{}\\] ({})", equation.tex, equation.number)?
            }
            Block::FootnoteDefinition { name, blocks } => {
                write!(output, "[{name}] ")?;
                write_blocks(blocks, context, output)?;
            }
            Block::ThematicBreak => output.write_all(b"---")?,
            Block::Html { .. } => {
                return Err(io::Error::other(
                    "UnsupportedChannelFeature: raw HTML is not portable text",
                ));
            }
        }
    }
    Ok(())
}

fn write_inlines(inlines: &[Inline], context: &Context, output: &mut impl Write) -> io::Result<()> {
    for inline in inlines {
        match inline {
            Inline::Text { value } | Inline::Code { value } => {
                output.write_all(value.as_bytes())?
            }
            Inline::Emphasis { content }
            | Inline::Strong { content }
            | Inline::Strikethrough { content } => write_inlines(content, context, output)?,
            Inline::Link {
                destination,
                content,
                ..
            } => {
                write_inlines(content, context, output)?;
                write!(output, " ({})", context.url(destination))?;
            }
            Inline::CrossReference { target, content } => {
                write_inlines(content, context, output)?;
                let destination = format!("#{}", target.0);
                write!(output, " ({})", context.url(&destination))?;
            }
            Inline::DocumentLink(link) => {
                write_inlines(&link.content, context, output)?;
                let url = context.document_urls.get(&link.target.0).ok_or_else(|| {
                    io::Error::other("UnresolvedReference: missing original document URL")
                })?;
                write!(output, " ({url}")?;
                if let Some(fragment) = &link.fragment {
                    write!(output, "#{fragment}")?;
                }
                output.write_all(b")")?;
            }
            Inline::Image {
                source,
                description,
                ..
            } => {
                write_inlines(description, context, output)?;
                write!(output, " ({})", context.url(source))?;
            }
            Inline::SoftBreak | Inline::LineBreak => output.write_all(b"\n")?,
            Inline::FootnoteReference { name } => write!(output, "[{name}]")?,
            Inline::Math { tex } => write!(output, "\\({tex}\\)")?,
            Inline::Html { .. } => {
                return Err(io::Error::other(
                    "UnsupportedChannelFeature: raw inline HTML is not portable text",
                ));
            }
        }
    }
    Ok(())
}
