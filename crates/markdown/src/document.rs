//! Pure projection from a Comrak syntax tree into Berlin's semantic document model.

use std::collections::HashMap;
use std::iter::Peekable;

use berlin_document::Block;
use berlin_document::ContentId;
use berlin_document::Document;
use berlin_document::DocumentKind;
use berlin_document::Inline;
use berlin_document::List;
use berlin_document::ListItem;
use berlin_document::Metadata;
use berlin_document::Provenance;
use berlin_document::SourceFormat;
use berlin_document::Table;
use berlin_document::TableAlignment;
use berlin_document::TableRow;
use comrak::nodes::AstNode;
use comrak::nodes::NodeFootnoteDefinition;
use comrak::nodes::NodeList;
use comrak::nodes::NodeTable;
use comrak::nodes::NodeValue;
use comrak::nodes::TableAlignment as ComrakTableAlignment;
use sha2::Digest;
use sha2::Sha256;

use crate::Source;
use crate::front_matter::FrontMatter;

pub(super) fn from_ast<'a>(
    root: &'a AstNode<'a>,
    front_matter: Option<&FrontMatter>,
    source: &Source<'_>,
) -> Result<Document, String> {
    let metadata = front_matter.map_or_else(Metadata::default, |value| Metadata {
        title: value.title.clone(),
        slug: value.slug.clone(),
        previous_slugs: value.previous_slugs.clone(),
        authors: value.author.clone().unwrap_or_default(),
        description: value.description.clone(),
        published: value.published.clone(),
        modified: value.modified.clone(),
        tags: value.tags.clone().unwrap_or_default(),
        draft: value.draft,
        comments: value.comments,
    });
    let source_uri = source.uri().to_string();
    let id = front_matter
        .and_then(|value| value.id.clone())
        .unwrap_or_else(|| source_uri.clone());
    let digest = Sha256::digest(source.text().as_bytes());
    let source_hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    Ok(Document {
        id: ContentId(id),
        kind: front_matter.map_or_else(DocumentKind::default, |value| value.kind.clone()),
        metadata,
        blocks: group_sections(blocks_from_children(root)?),
        relations: Vec::new(),
        provenance: Provenance {
            source: source_uri,
            source_format: SourceFormat::Markdown,
            source_hash,
        },
    })
}

fn group_sections(blocks: Vec<Block>) -> Vec<Block> {
    let mut blocks = blocks.into_iter().peekable();
    let mut ids = HashMap::new();
    sections_below(&mut blocks, 0, &mut ids)
}

fn sections_below<I>(
    blocks: &mut Peekable<I>,
    parent_level: u8,
    ids: &mut HashMap<String, usize>,
) -> Vec<Block>
where
    I: Iterator<Item = Block>,
{
    let mut grouped = Vec::new();
    while let Some(block) = blocks.peek() {
        if matches!(block, Block::Heading { level, .. } if *level <= parent_level) {
            break;
        }
        let block = blocks.next().expect("peeked block must exist");
        match block {
            Block::Heading { level, id, content } => {
                let base_id = id.unwrap_or_else(|| heading_id(&content));
                let occurrence = ids.entry(base_id.clone()).or_default();
                let id = if *occurrence == 0 {
                    base_id
                } else {
                    format!("{base_id}-{occurrence}")
                };
                *occurrence += 1;
                grouped.push(Block::Section {
                    id: berlin_document::ComponentId(id),
                    level,
                    role: None,
                    title: content,
                    blocks: sections_below(blocks, level, ids),
                });
            }
            block => grouped.push(block),
        }
    }
    grouped
}

fn heading_id(content: &[Inline]) -> String {
    let text = inline_plain_text(content).to_lowercase();
    let mut id = String::new();
    let mut needs_separator = false;
    for character in text.chars() {
        if character.is_alphanumeric() {
            if needs_separator && !id.is_empty() {
                id.push('-');
            }
            id.push(character);
            needs_separator = false;
        } else {
            needs_separator = true;
        }
    }
    if id.is_empty() { "section".into() } else { id }
}

fn inline_plain_text(inlines: &[Inline]) -> String {
    let mut output = String::new();
    for inline in inlines {
        match inline {
            Inline::Text { value } | Inline::Code { value } => output.push_str(value),
            Inline::Emphasis { content }
            | Inline::Strong { content }
            | Inline::Strikethrough { content }
            | Inline::Link { content, .. } => output.push_str(&inline_plain_text(content)),
            Inline::DocumentLink(link) => output.push_str(&inline_plain_text(&link.content)),
            Inline::Image { description, .. } => {
                output.push_str(&inline_plain_text(description));
            }
            Inline::SoftBreak | Inline::LineBreak => output.push(' '),
            Inline::Html { .. } | Inline::FootnoteReference { .. } => {}
        }
    }
    output
}

fn blocks_from_children<'a>(node: &'a AstNode<'a>) -> Result<Vec<Block>, String> {
    let blocks = node
        .children()
        .map(blocks_from_node)
        .collect::<Result<Vec<_>, _>>()?;
    crate::code::associate_named_listings(blocks.into_iter().flatten().collect())
}

fn blocks_from_node<'a>(node: &'a AstNode<'a>) -> Result<Vec<Block>, String> {
    Ok(match &node.data.borrow().value {
        NodeValue::FrontMatter(_) => Vec::new(),
        NodeValue::Paragraph => vec![paragraph_block(node)],
        NodeValue::Heading(heading) => vec![heading_block(node, heading.level)],
        NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => vec![block_quote(node)?],
        NodeValue::List(list) => vec![list_block(node, list)?],
        NodeValue::CodeBlock(code) => vec![crate::code::from_node(code)?],
        NodeValue::HtmlBlock(html) => vec![Block::Html {
            value: html.literal.clone(),
        }],
        NodeValue::Table(table) => vec![table_block(node, table)],
        NodeValue::ThematicBreak => vec![Block::ThematicBreak],
        NodeValue::FootnoteDefinition(footnote) => vec![footnote_definition(node, footnote)?],
        value if is_transparent_container(value) => blocks_from_children(node)?,
        _ => Vec::new(),
    })
}

fn paragraph_block<'a>(node: &'a AstNode<'a>) -> Block {
    Block::Paragraph {
        content: inlines_from_children(node),
    }
}

fn heading_block<'a>(node: &'a AstNode<'a>, level: u8) -> Block {
    let mut content = inlines_from_children(node);
    let id = take_heading_id(&mut content);
    Block::Heading { level, id, content }
}

fn block_quote<'a>(node: &'a AstNode<'a>) -> Result<Block, String> {
    Ok(Block::BlockQuote {
        blocks: blocks_from_children(node)?,
    })
}

fn list_block<'a>(node: &'a AstNode<'a>, list: &NodeList) -> Result<Block, String> {
    let ordered = list.list_type == comrak::nodes::ListType::Ordered;
    Ok(Block::List(List {
        ordered,
        start: ordered.then_some(list.start),
        tight: list.tight,
        items: node
            .children()
            .filter(|child| {
                matches!(
                    child.data.borrow().value,
                    NodeValue::Item(_) | NodeValue::TaskItem(_)
                )
            })
            .map(list_item)
            .collect::<Result<_, _>>()?,
    }))
}

fn list_item<'a>(item: &'a AstNode<'a>) -> Result<ListItem, String> {
    Ok(ListItem {
        checked: match item.data.borrow().value {
            NodeValue::TaskItem(marker) => Some(marker.symbol.is_some()),
            _ => task_state(item),
        },
        blocks: blocks_from_children(item)?,
    })
}

fn table_block<'a>(node: &'a AstNode<'a>, table: &NodeTable) -> Block {
    Block::Table(Table {
        alignments: table.alignments.iter().map(table_alignment).collect(),
        rows: node.children().filter_map(table_row).collect(),
    })
}

fn table_row<'a>(row: &'a AstNode<'a>) -> Option<TableRow> {
    match row.data.borrow().value {
        NodeValue::TableRow(header) => Some(TableRow {
            header,
            cells: row.children().map(inlines_from_children).collect(),
        }),
        _ => None,
    }
}

fn footnote_definition<'a>(
    node: &'a AstNode<'a>,
    footnote: &NodeFootnoteDefinition,
) -> Result<Block, String> {
    Ok(Block::FootnoteDefinition {
        name: footnote.name.clone(),
        blocks: blocks_from_children(node)?,
    })
}

fn is_transparent_container(value: &NodeValue) -> bool {
    matches!(
        value,
        NodeValue::Document
            | NodeValue::Item(_)
            | NodeValue::DescriptionList
            | NodeValue::DescriptionItem(_)
            | NodeValue::DescriptionTerm
            | NodeValue::DescriptionDetails
            | NodeValue::TaskItem(_)
            | NodeValue::TableRow(_)
            | NodeValue::TableCell
    )
}

fn task_state<'a>(node: &'a AstNode<'a>) -> Option<bool> {
    for descendant in node.descendants() {
        if let NodeValue::TaskItem(marker) = descendant.data.borrow().value {
            return Some(marker.symbol.is_some());
        }
    }
    None
}

fn inlines_from_children<'a>(node: &'a AstNode<'a>) -> Vec<Inline> {
    node.children().flat_map(inline_from_node).collect()
}

pub(super) fn inline_fragment(source: &str) -> Vec<Inline> {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, source, &crate::options::berlin_markdown_options());
    inlines_from_children(root)
}

fn inline_from_node<'a>(node: &'a AstNode<'a>) -> Vec<Inline> {
    match &node.data.borrow().value {
        NodeValue::Text(value) => vec![Inline::Text {
            value: value.to_string(),
        }],
        NodeValue::Emph => vec![Inline::Emphasis {
            content: inlines_from_children(node),
        }],
        NodeValue::Strong => vec![Inline::Strong {
            content: inlines_from_children(node),
        }],
        NodeValue::Strikethrough => vec![Inline::Strikethrough {
            content: inlines_from_children(node),
        }],
        NodeValue::Code(code) => vec![Inline::Code {
            value: code.literal.clone(),
        }],
        NodeValue::Link(link) => vec![link_from_node(node, link)],
        NodeValue::Image(image) => vec![Inline::Image {
            source: image.url.clone(),
            title: nonempty(&image.title),
            description: inlines_from_children(node),
        }],
        NodeValue::HtmlInline(value) | NodeValue::Raw(value) => vec![Inline::Html {
            value: value.clone(),
        }],
        NodeValue::SoftBreak => vec![Inline::SoftBreak],
        NodeValue::LineBreak => vec![Inline::LineBreak],
        NodeValue::FootnoteReference(reference) => vec![Inline::FootnoteReference {
            name: reference.name.clone(),
        }],
        _ => inlines_from_children(node),
    }
}

fn link_from_node<'a>(node: &'a AstNode<'a>, link: &comrak::nodes::NodeLink) -> Inline {
    let content = inlines_from_children(node);
    let title = nonempty(&link.title);
    if let Some(destination) = link.url.strip_prefix("id:") {
        let (target, fragment) = destination
            .split_once('#')
            .map_or((destination, None), |(target, fragment)| {
                (target, Some(fragment.to_owned()))
            });
        Inline::DocumentLink(berlin_document::DocumentLink {
            target: ContentId(target.into()),
            fragment,
            anchor: berlin_document::ComponentId(String::new()),
            title,
            content,
        })
    } else {
        Inline::Link {
            destination: link.url.clone(),
            title,
            content,
        }
    }
}

/// Generated occurrence anchors are rebuilt with backlinks, after caption parsing.
pub(super) fn assign_reference_anchors(blocks: &mut [Block]) {
    fn inlines(content: &mut [Inline], next: &mut usize) {
        for inline in content {
            match inline {
                Inline::DocumentLink(link) => {
                    *next += 1;
                    link.anchor.0 = format!("bln-ref-{next}");
                }
                Inline::Emphasis { content }
                | Inline::Strong { content }
                | Inline::Strikethrough { content }
                | Inline::Link { content, .. } => inlines(content, next),
                _ => {}
            }
        }
    }
    fn visit(blocks: &mut [Block], next: &mut usize) {
        for block in blocks {
            match block {
                Block::Paragraph { content } | Block::Heading { content, .. } => {
                    inlines(content, next)
                }
                Block::Section { title, blocks, .. } => {
                    inlines(title, next);
                    visit(blocks, next);
                }
                Block::Component { blocks, .. }
                | Block::BlockQuote { blocks }
                | Block::FootnoteDefinition { blocks, .. } => visit(blocks, next),
                Block::List(list) => {
                    for item in &mut list.items {
                        visit(&mut item.blocks, next);
                    }
                }
                Block::Table(table) => {
                    for row in &mut table.rows {
                        for cell in &mut row.cells {
                            inlines(cell, next);
                        }
                    }
                }
                Block::Code(code) => {
                    if let Some(caption) = &mut code.caption {
                        inlines(caption, next);
                    }
                }
                _ => {}
            }
        }
    }
    visit(blocks, &mut 0);
}

fn take_heading_id(content: &mut [Inline]) -> Option<String> {
    for inline in content.iter_mut().rev() {
        let Inline::Text { value } = inline else {
            continue;
        };
        let Some(start) = value.rfind(" {#") else {
            continue;
        };
        if !value.ends_with('}') {
            continue;
        }
        let id = value[start + 3..value.len() - 1].to_owned();
        value.truncate(start);
        return Some(id);
    }
    None
}

fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn table_alignment(value: &ComrakTableAlignment) -> TableAlignment {
    match value {
        ComrakTableAlignment::None => TableAlignment::None,
        ComrakTableAlignment::Left => TableAlignment::Left,
        ComrakTableAlignment::Center => TableAlignment::Center,
        ComrakTableAlignment::Right => TableAlignment::Right,
    }
}
