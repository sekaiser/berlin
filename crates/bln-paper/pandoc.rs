//! Explicit projections of supported Pandoc nodes; no raw-TeX or HTML fallback.
mod tables;
use crate::{BibliographyEntry, Error, Labels, Publication, aux::group, unsupported};
use berlin_document::{
    Block, CodeBlock, ComponentId, Document, DocumentKind, Equation, Inline, List, ListItem,
    plain_text,
};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn import(
    root: Value,
    labels: &Labels,
    bibliography: &[BibliographyEntry],
    mut publication: Publication,
) -> Result<Document, Error> {
    if root["pandoc-api-version"] != serde_json::json!([1, 23, 1, 1]) {
        return Err(unsupported(
            "expected Pandoc API 1.23.1.1 (the pinned paper toolchain)",
        ));
    }
    let mut importer = Importer {
        labels,
        footnotes: Vec::new(),
    };
    let meta = root["meta"]
        .as_object()
        .ok_or_else(|| unsupported("missing paper metadata"))?;
    for key in meta.keys() {
        if !matches!(
            key.as_str(),
            "title"
                | "author"
                | "date"
                | "abstract"
                | "bibliography"
                | "link-citations"
                | "references"
        ) {
            return Err(unsupported(format!("unsupported paper metadata '{key}'")));
        }
    }
    publication.metadata.title = Some(importer.meta_text(&root["meta"]["title"])?);
    publication.metadata.authors = array(&root["meta"]["author"]["c"])?
        .iter()
        .map(|author| importer.meta_text(author))
        .collect::<Result<_, _>>()?;
    let mut blocks = vec![Block::Paragraph {
        content: vec![Inline::Link {
            destination: publication.pdf_url,
            title: None,
            content: text("Download the paper (PDF)"),
        }],
    }];
    if let Some(abstract_value) = meta.get("abstract") {
        if tag(abstract_value)? != "MetaBlocks" {
            return Err(unsupported("unsupported abstract metadata"));
        }
        let abstract_blocks = importer.blocks(array(&abstract_value["c"])?)?;
        if publication.metadata.description.is_none() {
            let summary = abstract_blocks
                .iter()
                .find_map(|block| match block {
                    Block::Paragraph { content } => Some(plain_text(content)),
                    _ => None,
                })
                .unwrap_or_default();
            publication.metadata.description =
                Some(summary.split_whitespace().collect::<Vec<_>>().join(" "));
        }
        blocks.extend(abstract_blocks);
    }
    publication.metadata.description.get_or_insert_default();
    blocks.extend(importer.blocks(array(&root["blocks"])?)?);
    let mut bibliography_blocks = Vec::new();
    for entry in bibliography {
        let number = labels.citation(&entry.key)?;
        let ast: Value = serde_json::from_str(&entry.pandoc_json)?;
        if ast["pandoc-api-version"] != root["pandoc-api-version"] {
            return Err(unsupported("bibliography API version differs from paper"));
        }
        let mut item = importer.blocks(array(&ast["blocks"])?)?;
        let Some(Block::Paragraph { content }) = item.first_mut() else {
            return Err(unsupported("bibliography entry must start with prose"));
        };
        content.splice(0..0, text(&format!("[{number}] ")));
        bibliography_blocks.push(Block::Component {
            id: ComponentId(format!("ref-{}", entry.key)),
            name: "bibliography-entry".into(),
            properties: BTreeMap::new(),
            blocks: item,
        });
    }
    // These are siblings of the article's sections, not children of its last section.
    blocks = nest_sections(blocks);
    if !bibliography_blocks.is_empty() {
        blocks.push(Block::Section {
            id: ComponentId("refs".into()),
            level: 2,
            role: None,
            title: text("References"),
            blocks: bibliography_blocks,
        });
    }
    if !importer.footnotes.is_empty() {
        blocks.push(Block::Section {
            id: ComponentId("paper-footnotes".into()),
            level: 2,
            role: None,
            title: text("Notes"),
            blocks: std::mem::take(&mut importer.footnotes),
        });
    }
    let document = Document {
        id: publication.id,
        kind: DocumentKind::Article,
        metadata: publication.metadata,
        provenance: publication.provenance,
        relations: vec![],
        blocks,
    };
    document.validate()?;
    // Every compiled label must survive projection, not just labels that happen
    // to be referenced by another passage in this paper.
    for id in labels.references.keys() {
        if !has_id(&document.blocks, id) {
            return Err(unsupported(format!(
                "compiled label '{id}' was not preserved"
            )));
        }
    }
    for id in labels.citations.keys() {
        if !has_id(&document.blocks, &format!("ref-{id}")) {
            return Err(unsupported(format!(
                "bibliography entry '{id}' was not preserved"
            )));
        }
    }
    Ok(document)
}

fn has_id(blocks: &[Block], target: &str) -> bool {
    blocks.iter().any(|block| match block {
        Block::Equation(equation) => equation.id.0 == target,
        Block::Section { id, blocks, .. } | Block::Component { id, blocks, .. } => {
            id.0 == target || has_id(blocks, target)
        }
        Block::BlockQuote { blocks } | Block::FootnoteDefinition { blocks, .. } => {
            has_id(blocks, target)
        }
        Block::List(list) => list.items.iter().any(|item| has_id(&item.blocks, target)),
        _ => false,
    })
}

struct Importer<'a> {
    labels: &'a Labels,
    footnotes: Vec<Block>,
}

impl Importer<'_> {
    fn meta_text(&mut self, node: &Value) -> Result<String, Error> {
        let value = match tag(node)? {
            "MetaString" => string(&node["c"])?.to_owned(),
            "MetaInlines" => plain_text(&self.inlines(array(&node["c"])?)?),
            other => return Err(unsupported(format!("metadata node '{other}'"))),
        };
        if value.trim().is_empty() {
            return Err(unsupported("empty title or author"));
        }
        Ok(value)
    }

    fn blocks(&mut self, nodes: &[Value]) -> Result<Vec<Block>, Error> {
        let mut blocks = Vec::new();
        for node in nodes {
            blocks.extend(self.block(node)?);
        }
        Ok(blocks)
    }

    fn block(&mut self, node: &Value) -> Result<Vec<Block>, Error> {
        let c = &node["c"];
        Ok(match tag(node)? {
            "Para" | "Plain" => self.paragraph(array(c)?)?,
            "Header" => {
                let [level, attr, title] = parts(c)?;
                let level = level
                    .as_u64()
                    .filter(|n| (1..=4).contains(n))
                    .ok_or_else(|| {
                        unsupported("only chapter through subsubsection are supported")
                    })? as u8;
                let id = attr_id(attr, &[])?;
                let label = self.labels.reference(id)?;
                if !matches!(
                    label.kind.as_str(),
                    "chapter" | "section" | "subsection" | "subsubsection"
                ) {
                    return Err(unsupported("section label has the wrong kind"));
                }
                let mut content = text(&format!("{}. ", label.number));
                content.extend(self.inlines(array(title)?)?);
                vec![Block::Heading {
                    level: level + 1,
                    id: Some(id.into()),
                    content,
                }]
            }
            "BlockQuote" => vec![Block::BlockQuote {
                blocks: self.blocks(array(c)?)?,
            }],
            "BulletList" => vec![Block::List(self.list(array(c)?, None)?)],
            "OrderedList" => {
                let [attrs, items] = parts(c)?;
                let [start, style, delimiter] = parts(attrs)?;
                if !matches!(tag(style)?, "Decimal" | "DefaultStyle")
                    || !matches!(tag(delimiter)?, "Period" | "DefaultDelim")
                {
                    return Err(unsupported("non-decimal ordered list"));
                }
                let start = start
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .filter(|n| *n > 0)
                    .ok_or_else(|| unsupported("invalid list start"))?;
                vec![Block::List(self.list(array(items)?, Some(start))?)]
            }
            "CodeBlock" => {
                let [attrs, value] = parts(c)?;
                let id = attr_id(attrs, &[])?;
                vec![Block::Code(CodeBlock {
                    language: None,
                    value: string(value)?.into(),
                    caption: None,
                    highlights: vec![],
                    references: berlin_document::CodeReferences {
                        id: (!id.is_empty()).then(|| id.into()),
                        ..Default::default()
                    },
                })]
            }
            "HorizontalRule" => vec![Block::ThematicBreak],
            "RawBlock" => {
                let [format, value] = parts(c)?;
                if string(format)? != "latex"
                    || !matches!(
                        string(value)?.trim(),
                        r"\maketitle"
                            | r"\centering"
                            | r"\clearpage"
                            | r"\newpage"
                            | r"\bibliographystyle{plain}"
                            | r"\bibliographystyle{unsrt}"
                            | r"\bibliographystyle{abbrv}"
                            | r"\bibliographystyle{alpha}"
                    )
                {
                    return Err(unsupported(format!("raw block: {c}")));
                }
                vec![]
            }
            "Div" => self.div(c)?,
            "Table" => self.table(c)?,
            "Figure" => self.figure(c)?,
            other => return Err(unsupported(format!("block '{other}'"))),
        })
    }

    fn list(&mut self, items: &[Value], start: Option<usize>) -> Result<List, Error> {
        Ok(List {
            ordered: start.is_some(),
            start,
            tight: false,
            items: items
                .iter()
                .map(|item| {
                    Ok(ListItem {
                        checked: None,
                        blocks: self.blocks(array(item)?)?,
                    })
                })
                .collect::<Result<_, Error>>()?,
        })
    }

    fn paragraph(&mut self, nodes: &[Value]) -> Result<Vec<Block>, Error> {
        let mut blocks = Vec::new();
        let mut content = Vec::new();
        for node in nodes {
            if tag(node)? == "Math" && tag(&node["c"][0])? == "DisplayMath" {
                flush_paragraph(&mut content, &mut blocks);
                blocks.push(self.equation(string(&node["c"][1])?)?);
            } else {
                content.extend(self.inline(node)?);
            }
        }
        flush_paragraph(&mut content, &mut blocks);
        Ok(blocks)
    }

    fn equation(&self, tex: &str) -> Result<Block, Error> {
        if let Some((before, after)) = tex.split_once(r"\label") {
            let (id, after) = group(after)?;
            if after.contains(r"\label") {
                return Err(unsupported("multiple labels in one equation"));
            }
            let label = self.labels.reference(id)?;
            if label.kind != "equation" {
                return Err(unsupported("equation label has the wrong kind"));
            }
            let tex = format!("{before}{after}").trim().to_owned();
            berlin_math::render(&tex, berlin_math::DisplayMode::Block)?;
            Ok(Block::Equation(Equation {
                id: ComponentId(id.into()),
                number: label.number.clone(),
                tex,
            }))
        } else {
            berlin_math::render(tex, berlin_math::DisplayMode::Block)?;
            Ok(Block::Math { tex: tex.into() })
        }
    }

    fn inlines(&mut self, nodes: &[Value]) -> Result<Vec<Inline>, Error> {
        let mut inlines = Vec::new();
        for node in nodes {
            inlines.extend(self.inline(node)?);
        }
        Ok(inlines)
    }

    fn inline(&mut self, node: &Value) -> Result<Vec<Inline>, Error> {
        let c = &node["c"];
        Ok(match tag(node)? {
            "Str" => text(string(c)?),
            "Space" => text(" "),
            "SoftBreak" => vec![Inline::SoftBreak],
            "LineBreak" => vec![Inline::LineBreak],
            "Emph" => vec![Inline::Emphasis {
                content: self.inlines(array(c)?)?,
            }],
            "Strong" => vec![Inline::Strong {
                content: self.inlines(array(c)?)?,
            }],
            "Strikeout" => vec![Inline::Strikethrough {
                content: self.inlines(array(c)?)?,
            }],
            "Math" => {
                let [kind, tex] = parts(c)?;
                if tag(kind)? != "InlineMath" {
                    return Err(unsupported("display math inside inline formatting"));
                }
                let tex = string(tex)?;
                berlin_math::render(tex, berlin_math::DisplayMode::Inline)?;
                vec![Inline::Math { tex: tex.into() }]
            }
            "Code" => {
                let [attr, value] = parts(c)?;
                empty_attr(attr)?;
                vec![Inline::Code {
                    value: string(value)?.into(),
                }]
            }
            "Image" => {
                let [attr, description, target] = parts(c)?;
                let [id, classes, attributes] = parts(attr)?;
                if !string(id)?.is_empty() || !array(classes)?.is_empty() {
                    return Err(unsupported("inline image identity/classes"));
                }
                for attribute in array(attributes)? {
                    let [key, _] = parts(attribute)?;
                    if !matches!(string(key)?, "width" | "height") {
                        return Err(unsupported("unsupported image attribute"));
                    }
                }
                let [source, title] = parts(target)?;
                let source = string(source)?;
                if !source.starts_with('/') || source.starts_with("//") || source.contains("..") {
                    return Err(unsupported(
                        "image must be a prepared public paper resource",
                    ));
                }
                vec![Inline::Image {
                    source: source.into(),
                    title: (!string(title)?.is_empty()).then(|| string(title).unwrap().into()),
                    description: self.inlines(array(description)?)?,
                }]
            }
            "Span" => {
                let [attr, content] = parts(c)?;
                empty_attr(attr)?;
                self.inlines(array(content)?)?
            }
            "Link" => {
                let [attr, content, target] = parts(c)?;
                empty_attr(attr)?;
                let [destination, title] = parts(target)?;
                let destination = string(destination)?;
                let content = self.inlines(array(content)?)?;
                if let Some(target) = destination.strip_prefix('#') {
                    vec![Inline::CrossReference {
                        target: ComponentId(target.into()),
                        content,
                    }]
                } else {
                    if !destination.starts_with("https://")
                        && !destination.starts_with("http://")
                        && !destination.starts_with("mailto:")
                    {
                        return Err(unsupported(
                            "only web/mail links and local references are supported",
                        ));
                    }
                    let title = string(title)?;
                    vec![Inline::Link {
                        destination: destination.into(),
                        title: (!title.is_empty()).then(|| title.into()),
                        content,
                    }]
                }
            }
            "RawInline" => {
                let [format, value] = parts(c)?;
                let value = string(value)?;
                if string(format)? != "latex" {
                    return Err(unsupported("non-TeX raw inline"));
                }
                if matches!(value.trim(), r"\clearpage" | r"\newpage") {
                    return Ok(vec![Inline::SoftBreak]);
                }
                let (rest, equation) = if let Some(rest) = value.strip_prefix(r"\eqref") {
                    (rest, true)
                } else if let Some(rest) = value.strip_prefix(r"\ref") {
                    (rest, false)
                } else {
                    return Err(unsupported(format!("raw inline '{value}'")));
                };
                let (id, rest) = group(rest)?;
                if !rest.trim().is_empty() {
                    return Err(unsupported(format!("trailing reference data '{value}'")));
                }
                let label = self.labels.reference(id)?;
                if equation && label.kind != "equation" {
                    return Err(unsupported("eqref does not target an equation"));
                }
                vec![Inline::CrossReference {
                    target: ComponentId(id.into()),
                    content: text(&if equation {
                        format!("({})", label.number)
                    } else {
                        label.number.clone()
                    }),
                }]
            }
            "Cite" => self.citations(c)?,
            "Note" => {
                let name = (self.footnotes.len() + 1).to_string();
                // Reserve before recursion so nested notes cannot reuse an ID.
                self.footnotes.push(Block::FootnoteDefinition {
                    name: name.clone(),
                    blocks: vec![],
                });
                let index = self.footnotes.len() - 1;
                let blocks = self.blocks(array(c)?)?;
                self.footnotes[index] = Block::FootnoteDefinition {
                    name: name.clone(),
                    blocks,
                };
                vec![Inline::FootnoteReference { name }]
            }
            other => return Err(unsupported(format!("inline '{other}'"))),
        })
    }

    fn citations(&mut self, value: &Value) -> Result<Vec<Inline>, Error> {
        let [citations, _formatted] = parts(value)?;
        let mut content = Vec::new();
        for (index, citation) in array(citations)?.iter().enumerate() {
            if tag(&citation["citationMode"])? != "NormalCitation" {
                return Err(unsupported("only normal numeric citations are supported"));
            }
            if index > 0 {
                content.extend(text(", "));
            }
            content.extend(self.inlines(array(&citation["citationPrefix"])?)?);
            let id = string(&citation["citationId"])?;
            let number = self.labels.citation(id)?;
            content.push(Inline::CrossReference {
                target: ComponentId(format!("ref-{id}")),
                content: text(&format!("[{number}]")),
            });
            let suffix = self.inlines(array(&citation["citationSuffix"])?)?;
            if !suffix.is_empty() {
                content.extend(text(", "));
                content.extend(suffix);
            }
        }
        if content.is_empty() {
            return Err(unsupported("empty citation"));
        }
        Ok(content)
    }

    fn div(&mut self, value: &Value) -> Result<Vec<Block>, Error> {
        let [attr, children] = parts(value)?;
        let id = attr_id(attr, &[])?;
        let label = self.labels.reference(id)?;
        if label.kind != "table" {
            return Err(unsupported(format!("unsupported block container '{id}'")));
        }
        let mut blocks = self.blocks(array(children)?)?;
        let caption = text(&format!("Table {}. ", label.number));
        if let Some(Block::Paragraph { content }) = blocks.first_mut() {
            content.splice(0..0, caption);
        } else {
            blocks.insert(0, Block::Paragraph { content: caption });
        }
        Ok(vec![Block::Component {
            id: ComponentId(id.into()),
            name: "paper-table".into(),
            properties: BTreeMap::new(),
            blocks,
        }])
    }

    fn figure(&mut self, value: &Value) -> Result<Vec<Block>, Error> {
        let [attr, caption, body] = parts(value)?;
        let id = attr_id(attr, &[])?;
        let label = self.labels.reference(id)?;
        if label.kind != "figure" {
            return Err(unsupported("figure label has wrong kind"));
        }
        let number = label.number.clone();
        let [short, caption] = parts(caption)?;
        if !short.is_null() {
            return Err(unsupported("short figure captions"));
        }
        let mut caption = self.blocks(array(caption)?)?;
        let description = caption
            .iter()
            .filter_map(|block| match block {
                Block::Paragraph { content } => Some(plain_text(content)),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        if description.trim().is_empty() {
            return Err(unsupported("figures require descriptive captions"));
        }
        let blocks = self.blocks(array(body)?)?;
        let [Block::Paragraph { content }] = blocks.as_slice() else {
            return Err(unsupported("figures currently require one image"));
        };
        let images = content
            .iter()
            .filter(|inline| {
                !matches!(inline, Inline::SoftBreak | Inline::LineBreak)
                    && !matches!(inline, Inline::Text { value } if value.trim().is_empty())
            })
            .collect::<Vec<_>>();
        let [
            Inline::Image {
                source,
                description: alt,
                ..
            },
        ] = images.as_slice()
        else {
            return Err(unsupported(
                "figures currently require one image without additional body prose",
            ));
        };
        let alt = plain_text(alt);
        let mut output = vec![Block::Figure(berlin_document::Figure {
            source: source.clone(),
            alt: Some(if alt.trim().is_empty() {
                description
            } else {
                alt
            }),
            caption: None,
        })];
        if let Some(Block::Paragraph { content }) = caption.first_mut() {
            content.splice(0..0, text(&format!("Figure {number}. ")));
        }
        output.extend(caption);
        Ok(vec![Block::Component {
            id: ComponentId(id.into()),
            name: "paper-figure".into(),
            properties: BTreeMap::new(),
            blocks: output,
        }])
    }
}

fn flush_paragraph(content: &mut Vec<Inline>, blocks: &mut Vec<Block>) {
    if !plain_text(content).trim().is_empty()
        || content
            .iter()
            .any(|inline| matches!(inline, Inline::Image { .. }))
    {
        blocks.push(Block::Paragraph {
            content: std::mem::take(content),
        });
    }
    content.clear();
}

fn nest_sections(blocks: Vec<Block>) -> Vec<Block> {
    fn consume(
        input: &mut std::iter::Peekable<std::vec::IntoIter<Block>>,
        parent: u8,
    ) -> Vec<Block> {
        let mut output = Vec::new();
        while let Some(block) = input.peek() {
            if matches!(block, Block::Heading { level, .. } if *level <= parent) {
                break;
            }
            match input.next().expect("peeked block") {
                Block::Heading {
                    level,
                    id: Some(id),
                    content,
                } => output.push(Block::Section {
                    id: ComponentId(id),
                    level,
                    role: None,
                    title: content,
                    blocks: consume(input, level),
                }),
                block => output.push(block),
            }
        }
        output
    }
    consume(&mut blocks.into_iter().peekable(), 0)
}

fn tag(value: &Value) -> Result<&str, Error> {
    string(&value["t"])
}
fn string(value: &Value) -> Result<&str, Error> {
    value
        .as_str()
        .ok_or_else(|| unsupported("expected a Pandoc string"))
}
fn array(value: &Value) -> Result<&[Value], Error> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| unsupported("expected a Pandoc array"))
}
fn parts<const N: usize>(value: &Value) -> Result<&[Value; N], Error> {
    array(value)?
        .try_into()
        .map_err(|_| unsupported(format!("expected {N} Pandoc fields")))
}
fn text(value: &str) -> Vec<Inline> {
    vec![Inline::Text {
        value: value.into(),
    }]
}
fn empty_attr(value: &Value) -> Result<(), Error> {
    if !attr_id(value, &[])?.is_empty() {
        return Err(unsupported("inline IDs are not supported"));
    }
    Ok(())
}
fn attr_id<'a>(value: &'a Value, allowed_classes: &[&str]) -> Result<&'a str, Error> {
    let [id, classes, attrs] = parts(value)?;
    for class in array(classes)? {
        if !allowed_classes.contains(&string(class)?) {
            return Err(unsupported(format!("unsupported class '{class}'")));
        }
    }
    for attr in array(attrs)? {
        let [key, value] = parts(attr)?;
        if string(key)? != "entry-spacing" || string(value)? != "0" {
            return Err(unsupported(format!("unsupported attribute '{attr}'")));
        }
    }
    string(id)
}
