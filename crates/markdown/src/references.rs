//! Explicit equation labels and bibliography entries shared by both projections.
use crate::{Error, front_matter::FrontMatter};
use berlin_document::{Block, ComponentId, Document, Equation, Inline};
use comrak::nodes::{AstNode, NodeValue};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reference {
    pub id: String,
    pub text: String,
    pub url: Option<String>,
    pub label: Option<String>,
}

pub(super) struct References {
    equations: BTreeMap<String, String>,
    bibliography: Vec<Reference>,
    cited: Vec<String>,
}

pub(super) fn expression(tex: &str) -> Result<(Option<String>, String), String> {
    let mut label = None;
    let mut cleaned = tex.to_owned();
    let mut replacements = Vec::new();
    for (start, end, name, args) in commands(tex)? {
        if name != "label" {
            continue;
        }
        if label.replace(args.clone()).is_some() {
            return Err("an equation must have at most one label".into());
        }
        identifier(&args)?;
        replacements.push(start..end);
    }
    for range in replacements.into_iter().rev() {
        cleaned.replace_range(range, "");
    }
    if label.is_some() {
        let trimmed = cleaned.trim();
        for env in ["equation", "equation*"] {
            if let Some(body) = trimmed
                .strip_prefix(&format!("\\begin{{{env}}}"))
                .and_then(|body| body.strip_suffix(&format!("\\end{{{env}}}")))
            {
                return Ok((label, body.trim().into()));
            }
        }
    }
    Ok((label, cleaned))
}

impl References {
    pub fn collect<'a>(
        root: &'a AstNode<'a>,
        matter: Option<&FrontMatter>,
        uri: &str,
    ) -> Result<Self, Error> {
        let mut result = Self {
            equations: BTreeMap::new(),
            bibliography: matter.map(|m| m.references.clone()).unwrap_or_default(),
            cited: Vec::new(),
        };
        let mut ids = std::collections::HashSet::new();
        for entry in &result.bibliography {
            identifier(&entry.id).map_err(|message| invalid(uri, message))?;
            if !ids.insert(&entry.id)
                || entry.text.trim().is_empty()
                || entry.label.as_ref().is_some_and(|l| l.trim().is_empty())
            {
                return Err(invalid(
                    uri,
                    "invalid or duplicate bibliography entry".into(),
                ));
            }
            if let Some(url) = &entry.url
                && !url::Url::parse(url).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
            {
                return Err(invalid(
                    uri,
                    "bibliography URLs must use HTTP or HTTPS".into(),
                ));
            }
        }
        for node in root.descendants() {
            let data = node.data.borrow();
            if let Some((tex, berlin_math::DisplayMode::Block)) =
                crate::math::expression(&data.value)
                && let Some(id) = expression(tex).map_err(|m| invalid(uri, m))?.0
            {
                let number = (result.equations.len() + 1).to_string();
                if result.equations.insert(id, number).is_some() {
                    return Err(invalid(uri, "duplicate equation label".into()));
                }
            }
            if let NodeValue::Text(text) = &data.value {
                if !commands(text).map_err(|m| invalid(uri, m))?.is_empty()
                    && node.ancestors().skip(1).any(|ancestor| {
                        matches!(
                            ancestor.data.borrow().value,
                            NodeValue::Link(_) | NodeValue::Image(_) | NodeValue::Heading(_)
                        )
                    })
                {
                    return Err(invalid(uri, "equation references and citations belong in prose, not headings, links or image descriptions".into()));
                }
                for (_, _, name, args) in commands(text).map_err(|m| invalid(uri, m))? {
                    if name == "cite" {
                        for id in args.split(',').map(str::trim) {
                            if !result.bibliography.iter().any(|entry| entry.id == id) {
                                return Err(invalid(uri, format!("unknown citation '{id}'")));
                            }
                            if !result.cited.iter().any(|seen| seen == id) {
                                result.cited.push(id.into());
                            }
                        }
                    }
                }
            }
        }
        Ok(result)
    }

    pub fn number(&self, id: &str) -> Result<&str, String> {
        self.equations
            .get(id)
            .map(String::as_str)
            .ok_or_else(|| format!("unknown equation reference '{id}'"))
    }

    fn inlines(&self, value: &str) -> Result<Vec<Inline>, String> {
        let mut output = Vec::new();
        let mut offset = 0;
        for (start, end, name, args) in commands(value)? {
            if name == "label" {
                return Err("labels must be inside display equations".into());
            }
            output.push(Inline::Text {
                value: value[offset..start].into(),
            });
            if name == "cite" {
                output.push(Inline::Text { value: "[".into() });
                for (index, id) in args.split(',').map(str::trim).enumerate() {
                    if index != 0 {
                        output.push(Inline::Text { value: ", ".into() });
                    }
                    let label = self.citation_label(id)?;
                    output.push(Inline::CrossReference {
                        target: ComponentId(format!("ref-{id}")),
                        content: vec![Inline::Text { value: label }],
                    });
                }
                output.push(Inline::Text { value: "]".into() });
            } else {
                let number = self.number(&args)?.to_owned();
                output.push(Inline::CrossReference {
                    target: ComponentId(args),
                    content: vec![Inline::Text {
                        value: if name == "eqref" {
                            format!("({number})")
                        } else {
                            number
                        },
                    }],
                });
            }
            offset = end;
        }
        output.push(Inline::Text {
            value: value[offset..].into(),
        });
        Ok(output)
    }

    fn citation_label(&self, id: &str) -> Result<String, String> {
        let index = self
            .cited
            .iter()
            .position(|key| key == id)
            .ok_or_else(|| format!("unknown citation '{id}'"))?;
        let entry = self
            .bibliography
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| format!("missing bibliography entry '{id}'"))?;
        Ok(entry
            .label
            .clone()
            .unwrap_or_else(|| (index + 1).to_string()))
    }

    pub fn resolve(&self, document: &mut Document) -> Result<(), String> {
        self.blocks(&mut document.blocks)?;
        if !self.cited.is_empty() {
            document.blocks.push(Block::Section {
                id: ComponentId("references".into()),
                level: 2,
                role: None,
                title: vec![Inline::Text {
                    value: "References".into(),
                }],
                blocks: self.bibliography_blocks()?,
            });
        }
        Ok(())
    }

    fn blocks(&self, blocks: &mut [Block]) -> Result<(), String> {
        for block in blocks {
            match block {
                Block::Math { tex } => {
                    if let (Some(id), body) = expression(tex)? {
                        *block = Block::Equation(Equation {
                            number: self.number(&id)?.into(),
                            id: ComponentId(id),
                            tex: body,
                        });
                    }
                }
                Block::Paragraph { content } | Block::Heading { content, .. } => {
                    self.map_inlines(content)?
                }
                Block::Section { title, blocks, .. } => {
                    self.map_inlines(title)?;
                    self.blocks(blocks)?;
                }
                Block::BlockQuote { blocks }
                | Block::Component { blocks, .. }
                | Block::FootnoteDefinition { blocks, .. } => self.blocks(blocks)?,
                Block::List(list) => {
                    for item in &mut list.items {
                        self.blocks(&mut item.blocks)?;
                    }
                }
                Block::Table(table) => {
                    for row in &mut table.rows {
                        for cell in &mut row.cells {
                            self.map_inlines(cell)?;
                        }
                    }
                }
                Block::Code(_)
                | Block::Figure(_)
                | Block::Html { .. }
                | Block::ThematicBreak
                | Block::Equation(_) => {}
            }
        }
        Ok(())
    }

    fn map_inlines(&self, inlines: &mut Vec<Inline>) -> Result<(), String> {
        let mut output = Vec::new();
        for mut inline in std::mem::take(inlines) {
            match &mut inline {
                Inline::Text { value } => {
                    output.extend(self.inlines(value)?);
                    continue;
                }
                Inline::Emphasis { content }
                | Inline::Strong { content }
                | Inline::Strikethrough { content }
                | Inline::Link { content, .. }
                | Inline::CrossReference { content, .. } => self.map_inlines(content)?,
                Inline::DocumentLink(link) => self.map_inlines(&mut link.content)?,
                _ => {}
            }
            output.push(inline);
        }
        *inlines = output;
        Ok(())
    }

    fn bibliography_blocks(&self) -> Result<Vec<Block>, String> {
        self.cited
            .iter()
            .map(|id| {
                let entry = self
                    .bibliography
                    .iter()
                    .find(|entry| &entry.id == id)
                    .unwrap();
                let mut content = vec![Inline::Text {
                    value: format!("[{}] {}", self.citation_label(id)?, entry.text),
                }];
                if let Some(url) = &entry.url {
                    content.push(Inline::Text { value: " ".into() });
                    content.push(Inline::Link {
                        destination: url.clone(),
                        title: None,
                        content: vec![Inline::Text {
                            value: "Source".into(),
                        }],
                    });
                }
                Ok(Block::Component {
                    id: ComponentId(format!("ref-{id}")),
                    name: "bibliography-entry".into(),
                    properties: BTreeMap::new(),
                    blocks: vec![Block::Paragraph { content }],
                })
            })
            .collect()
    }

    pub fn render_text(&self, value: &str) -> Result<Option<String>, String> {
        if commands(value)?.is_empty() {
            return Ok(None);
        }
        let mut output = String::new();
        for inline in self.inlines(value)? {
            match inline {
                Inline::Text { value } => output.push_str(&escape(&value)),
                Inline::CrossReference { target, content } => output.push_str(&format!(
                    "<a href=\"#{}\">{}</a>",
                    escape(&target.0),
                    escape(&berlin_document::plain_text(&content))
                )),
                _ => unreachable!(),
            }
        }
        Ok(Some(output))
    }

    pub fn bibliography_html(&self) -> Result<String, String> {
        if self.cited.is_empty() {
            return Ok(String::new());
        }
        let mut html = String::from("<section id=\"references\"><h2>References</h2>\n");
        for id in &self.cited {
            let entry = self
                .bibliography
                .iter()
                .find(|entry| &entry.id == id)
                .unwrap();
            html.push_str(&format!(
                "<p id=\"ref-{}\">[{}] {}",
                escape(id),
                escape(&self.citation_label(id)?),
                escape(&entry.text)
            ));
            if let Some(url) = &entry.url {
                html.push_str(&format!(" <a href=\"{}\">Source</a>", escape(url)));
            }
            html.push_str("</p>\n");
        }
        html.push_str("</section>\n");
        Ok(html)
    }
}

pub(super) fn invalid(uri: &str, message: String) -> Error {
    Error::InvalidDocument {
        source_uri: uri.into(),
        message,
    }
}
pub(super) fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
    {
        return Err("reference IDs use ASCII letters, digits, '.', '_', ':' and '-'".into());
    }
    Ok(())
}

fn commands(value: &str) -> Result<Vec<(usize, usize, &str, String)>, String> {
    let mut output = Vec::new();
    let mut offset = 0;
    while let Some(relative) = value[offset..].find('\\') {
        let start = offset + relative;
        let tail = &value[start + 1..];
        let length = tail.bytes().take_while(u8::is_ascii_alphabetic).count();
        if length == 0 {
            offset = (start + 2).min(value.len());
            while !value.is_char_boundary(offset) {
                offset += 1;
            }
            continue;
        }
        let name = &tail[..length];
        offset = start + 1 + length;
        if !matches!(name, "label" | "ref" | "eqref" | "cite") {
            continue;
        }
        let rest = value[offset..].trim_start();
        let rest = rest
            .strip_prefix('{')
            .ok_or_else(|| format!("\\{name} needs a braced identifier"))?;
        let (args, tail) = rest
            .split_once('}')
            .ok_or_else(|| format!("unclosed \\{name}"))?;
        if name == "cite" {
            for id in args.split(',').map(str::trim) {
                identifier(id)?;
            }
        } else {
            identifier(args)?;
        }
        offset = value.len() - tail.len();
        output.push((start, offset, name, args.to_owned()));
    }
    Ok(output)
}
