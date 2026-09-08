//! Plain-text projection of semantic blocks and inlines for LinkedIn.

use berlin_document::{Block, CodeBlock, Document, Figure, Inline, List, ListItem, Table};
use std::fmt::Write as _;

pub(super) fn render_document(document: &Document) -> String {
    let mut parts = Vec::new();
    if let Some(title) = document
        .metadata
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
    {
        parts.push(title.to_string());
    }
    parts.extend(render_blocks(&document.blocks));
    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string()
}

fn render_blocks(blocks: &[Block]) -> Vec<String> {
    blocks.iter().filter_map(render_block).collect()
}

fn render_block(block: &Block) -> Option<String> {
    let text = match block {
        Block::Paragraph { content } | Block::Heading { content, .. } => render_inlines(content),
        Block::BlockQuote { blocks } => render_block_quote(blocks),
        Block::List(list) => render_list(list),
        Block::Code(code) => render_code(code),
        Block::Figure(figure) => render_figure(figure),
        Block::Html { value } => strip_html(value),
        Block::Table(table) => render_table(table),
        Block::ThematicBreak => "—".into(),
        Block::FootnoteDefinition { name, blocks } => render_footnote_definition(name, blocks),
        Block::Section { title, blocks, .. } => render_section(title, blocks),
        Block::Component { blocks, .. } => render_blocks(blocks).join("\n\n"),
    };
    non_empty(text)
}

fn render_block_quote(blocks: &[Block]) -> String {
    render_blocks(blocks)
        .join("\n\n")
        .lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_code(code: &CodeBlock) -> String {
    let source = code
        .value
        .trim_end()
        .lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    match code
        .caption
        .as_ref()
        .map(|caption| render_inlines(caption))
        .filter(|caption| !caption.is_empty())
    {
        Some(caption) => format!("{caption}\n\n{source}"),
        None => source,
    }
}

fn render_figure(figure: &Figure) -> String {
    let mut text = figure.caption.clone().unwrap_or_default();
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(&figure.source);
    text
}

fn render_footnote_definition(name: &str, blocks: &[Block]) -> String {
    let body = render_blocks(blocks).join(" ");
    format!("[{name}] {body}")
}

fn render_section(title: &[Inline], blocks: &[Block]) -> String {
    let mut parts = Vec::new();
    let title = render_inlines(title);
    if !title.is_empty() {
        parts.push(title);
    }
    parts.extend(render_blocks(blocks));
    parts.join("\n\n")
}

fn render_list(list: &List) -> String {
    list.items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let marker = if list.ordered {
                format!("{}.", list.start.unwrap_or(1) + index)
            } else {
                "•".into()
            };
            format!("{marker} {}", render_list_item(item))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_list_item(item: &ListItem) -> String {
    let mut text = render_blocks(&item.blocks).join(" ");
    if let Some(checked) = item.checked {
        text = format!("[{}] {text}", if checked { 'x' } else { ' ' });
    }
    text
}

fn render_table(table: &Table) -> String {
    table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| render_inlines(cell))
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_inlines(inlines: &[Inline]) -> String {
    let mut text = String::new();
    for inline in inlines {
        render_inline(&mut text, inline);
    }
    text.trim().to_string()
}

fn render_inline(text: &mut String, inline: &Inline) {
    match inline {
        Inline::Text { value } | Inline::Code { value } => text.push_str(value),
        Inline::Emphasis { content }
        | Inline::Strong { content }
        | Inline::Strikethrough { content } => text.push_str(&render_inlines(content)),
        Inline::Link {
            destination,
            content,
            ..
        } => render_link(text, destination, content),
        Inline::DocumentLink(link) => text.push_str(&render_inlines(&link.content)),
        Inline::Image {
            source,
            description,
            ..
        } => render_image(text, source, description),
        Inline::Html { value } => text.push_str(&strip_html(value)),
        Inline::SoftBreak => text.push(' '),
        Inline::LineBreak => text.push('\n'),
        Inline::FootnoteReference { name } => {
            let _ = write!(text, "[{name}]");
        }
    }
}

fn render_link(text: &mut String, destination: &str, content: &[Inline]) {
    let label = render_inlines(content);
    if destination.starts_with('#') {
        text.push_str(&label);
    } else if label.is_empty() || label == destination {
        text.push_str(destination);
    } else {
        let _ = write!(text, "{label} ({destination})");
    }
}

fn render_image(text: &mut String, source: &str, description: &[Inline]) {
    let description = render_inlines(description);
    if description.is_empty() {
        text.push_str(source);
    } else {
        let _ = write!(text, "{description} ({source})");
    }
}

fn strip_html(value: &str) -> String {
    let mut text = String::new();
    let mut inside_tag = false;
    for character in value.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => text.push(character),
            _ => {}
        }
    }
    text
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}
