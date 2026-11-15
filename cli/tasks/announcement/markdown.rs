//! A static Markdown edition of a semantic document, independent of theme HTML.
use anyhow::{Error, bail};
use berlin_document::{Block, ContentId, Document, Inline};
use std::collections::HashMap;
use url::Url;

pub(super) struct Edition<'a> {
    pub original: &'a Url,
    pub mount: &'a Url,
    pub documents: &'a HashMap<ContentId, String>,
    pub warnings: Vec<String>,
}
impl Edition<'_> {
    fn url(&self, value: &str) -> Result<String, Error> {
        if value.starts_with("mailto:") || value.starts_with("tel:") {
            return Ok(value.into());
        }
        let url = berlin_publication::resolve_source_url(value, self.original, Some(self.mount))?;
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            bail!("distribution requires public HTTPS links and images: {value}");
        }
        Ok(url.into())
    }
    pub fn render(&mut self, document: &Document) -> Result<String, Error> {
        let mut output = self.blocks(&document.blocks)?;
        output.push_str(&format!("\n\nOriginally published at {}.\n", self.original));
        Ok(output)
    }
    fn blocks(&mut self, blocks: &[Block]) -> Result<String, Error> {
        let mut out = String::new();
        for block in blocks {
            let text = match block {
                Block::Paragraph { content } => self.inlines(content)?,
                Block::Heading { level, id, content } => {
                    let anchor = id
                        .as_ref()
                        .map(|id| {
                            format!(
                                "<a id=\"{}\"></a>\n\n",
                                html_escape::encode_double_quoted_attribute(id)
                            )
                        })
                        .unwrap_or_default();
                    format!(
                        "{anchor}{} {}",
                        "#".repeat((*level).clamp(1, 6) as usize),
                        self.inlines(content)?
                    )
                }
                Block::Code(code) => {
                    let fence = "`".repeat(
                        code.value
                            .split('\n')
                            .map(|l| l.chars().take_while(|&c| c == '`').count())
                            .max()
                            .unwrap_or(0)
                            .max(2)
                            + 1,
                    );
                    let language = code.language.as_deref().unwrap_or("");
                    if language.contains(['\n', '\r', '`']) {
                        bail!("invalid code language");
                    }
                    let caption = code
                        .caption
                        .as_ref()
                        .map(|c| self.inlines(c))
                        .transpose()?
                        .unwrap_or_default();
                    format!(
                        "{caption}\n\n{fence}{language}\n{}\n{fence}",
                        code.value.trim_end_matches('\n')
                    )
                }
                Block::BlockQuote { blocks } => self
                    .blocks(blocks)?
                    .lines()
                    .map(|l| format!("> {l}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                Block::List(list) => {
                    let mut items = Vec::new();
                    for (index, item) in list.items.iter().enumerate() {
                        let prefix = if list.ordered {
                            format!("{}. ", list.start.unwrap_or(1) + index)
                        } else {
                            "- ".into()
                        };
                        let checked = item
                            .checked
                            .map(|v| if v { "[x] " } else { "[ ] " })
                            .unwrap_or("");
                        let body = self.blocks(&item.blocks)?;
                        let mut lines = body.lines();
                        let first = lines.next().unwrap_or("");
                        let rest = lines
                            .map(|l| format!("{}{}", " ".repeat(prefix.len()), l))
                            .collect::<Vec<_>>()
                            .join("\n");
                        items.push(format!(
                            "{prefix}{checked}{first}{}",
                            if rest.is_empty() {
                                String::new()
                            } else {
                                format!("\n{rest}")
                            }
                        ));
                    }
                    items.join("\n")
                }
                Block::Figure(figure) => {
                    let alt = figure.alt.as_deref().unwrap_or("");
                    let caption = figure
                        .caption
                        .as_deref()
                        .map(plain_html)
                        .transpose()?
                        .unwrap_or_default();
                    format!(
                        "![{}](<{}>)\n\n{}",
                        escape(alt),
                        self.url(&figure.source)?,
                        escape(&caption)
                    )
                }
                Block::Table(table) => {
                    let mut rows = Vec::new();
                    for (i, row) in table.rows.iter().enumerate() {
                        let cells = row
                            .cells
                            .iter()
                            .map(|c| {
                                self.inlines(c)
                                    .map(|v| table_cell(&v).replace('\n', "<br>"))
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        rows.push(format!("| {} |", cells.join(" | ")));
                        if i == 0 {
                            rows.push(format!("| {} |", vec!["---"; cells.len()].join(" | ")));
                        }
                    }
                    rows.join("\n")
                }
                Block::FootnoteDefinition { name, blocks } => format!(
                    "[^{}]: {}",
                    name,
                    self.blocks(blocks)?.replace('\n', "\n    ")
                ),
                Block::ThematicBreak => "---".into(),
                Block::Math { tex } => {
                    self.warnings.push(
                        "Display mathematics retained as TeX; review platform rendering.".into(),
                    );
                    format!("$$\n{tex}\n$$")
                }
                Block::Equation(equation) => {
                    self.warnings.push(
                        "Numbered equation retained as TeX; review platform rendering.".into(),
                    );
                    format!(
                        "<a id=\"{}\"></a>\n\n$$\n{}\n$$\n\n({})",
                        html_escape::encode_double_quoted_attribute(equation.id.0.as_str()),
                        equation.tex,
                        equation.number
                    )
                }
                Block::Html { value } => self.html(value)?,
                Block::Section {
                    id,
                    level,
                    title,
                    blocks,
                    ..
                } => format!(
                    "<a id=\"{}\"></a>\n\n{} {}\n\n{}",
                    html_escape::encode_double_quoted_attribute(id.0.as_str()),
                    "#".repeat((*level).clamp(1, 6) as usize),
                    self.inlines(title)?,
                    self.blocks(blocks)?
                ),
                Block::Component { name, blocks, .. } => {
                    self.warnings.push(format!("Component '{name}' exported as static content; review the original for interaction."));
                    self.blocks(blocks)?
                }
            };
            if !text.trim().is_empty() {
                if out.len() + text.len() + 2 > 2 * 1024 * 1024 {
                    bail!("Markdown edition exceeds 2 MiB");
                }
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(&text);
            }
        }
        Ok(out)
    }
    fn html(&mut self, value: &str) -> Result<String, Error> {
        if [
            "<img", "<svg", "<iframe", "<video", "<audio", "<object", "<script",
        ]
        .iter()
        .any(|s| value.to_ascii_lowercase().contains(s))
        {
            self.warnings.push("Embedded HTML media or executable content replaced with a link to the original article.".into());
            return Ok(format!(
                "[View embedded content on the original article](<{}>)",
                self.original
            ));
        }
        // Notebook enclosures and captions need no theme classes on another site.
        if value.contains('<') {
            self.warnings.push("HTML formatting is simplified to text; review captions and embedded links in the draft.".into());
        }
        Ok(escape(&plain_html(value)?))
    }
    fn inlines(&mut self, values: &[Inline]) -> Result<String, Error> {
        let mut out = String::new();
        for value in values {
            out.push_str(&match value {
                Inline::Text { value } => escape(value),
                Inline::Code { value } => {
                    let fence = "`".repeat(value.split('`').count());
                    format!("{fence} {value} {fence}")
                }
                Inline::Emphasis { content } => format!("*{}*", self.inlines(content)?),
                Inline::Strong { content } => format!("**{}**", self.inlines(content)?),
                Inline::Strikethrough { content } => format!("~~{}~~", self.inlines(content)?),
                Inline::Link {
                    destination,
                    content,
                    ..
                } => format!("[{}](<{}>)", self.inlines(content)?, self.url(destination)?),
                Inline::Image {
                    source,
                    description,
                    ..
                } => format!("![{}](<{}>)", self.inlines(description)?, self.url(source)?),
                Inline::DocumentLink(link) => {
                    let url = self.documents.get(&link.target).ok_or_else(|| {
                        anyhow::anyhow!(
                            "unpublished or unresolved linked document: {}",
                            link.target.0
                        )
                    })?;
                    {
                        let mut destination = Url::parse(url)?;
                        destination.set_fragment(link.fragment.as_deref());
                        format!("[{}](<{}>)", self.inlines(&link.content)?, destination)
                    }
                }
                Inline::CrossReference { target, content } => {
                    let mut url = self.original.clone();
                    url.set_fragment(Some(target.0.as_str()));
                    format!("[{}](<{}>)", self.inlines(content)?, url)
                }
                Inline::Math { tex } => format!("${tex}$"),
                Inline::Html { value } => self.html(value)?,
                Inline::SoftBreak => "\n".into(),
                Inline::LineBreak => "  \n".into(),
                Inline::FootnoteReference { name } => format!("[^{name}]"),
            });
        }
        Ok(out)
    }
}
pub(super) fn plain_html(value: &str) -> Result<String, Error> {
    use std::{cell::RefCell, rc::Rc};
    let out = Rc::new(RefCell::new(String::new()));
    let text = out.clone();
    let boundary = out.clone();
    lol_html::rewrite_str(
        value,
        lol_html::RewriteStrSettings::new()
            .append_element_content_handler(lol_html::element!(
                "p,div,li,br,h1,h2,h3,h4,h5,h6,blockquote,pre",
                move |element| {
                    boundary.borrow_mut().push('\n');
                    let end = boundary.clone();
                    element.on_end_tag(lol_html::end_tag!(move |_| {
                        end.borrow_mut().push('\n');
                        Ok(())
                    }))?;
                    Ok(())
                }
            ))
            .append_document_content_handler(lol_html::doc_text!(move |t| {
                text.borrow_mut().push_str(t.as_str());
                Ok(())
            })),
    )?;
    let result = html_escape::decode_html_entities(out.borrow().as_str()).into_owned();
    Ok(result.trim().to_owned())
}
fn table_cell(value: &str) -> String {
    let mut out = String::new();
    let mut backslashes = 0;
    for c in value.chars() {
        if c == '|' && backslashes % 2 == 0 {
            out.push('\\');
        }
        out.push(c);
        backslashes = if c == '\\' { backslashes + 1 } else { 0 };
    }
    out
}
fn escape(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if matches!(
            ch,
            '\\' | '`'
                | '*'
                | '_'
                | '['
                | ']'
                | '<'
                | '>'
                | '#'
                | '.'
                | '!'
                | '('
                | ')'
                | '+'
                | '-'
                | '|'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_export_preserves_formatting_and_mount_relative_assets() {
        let original = Url::parse("https://example.test/notebook/notes/one.html").unwrap();
        let mount = Url::parse("https://example.test/notebook/").unwrap();
        let documents = HashMap::new();
        let mut edition = Edition {
            original: &original,
            mount: &mount,
            documents: &documents,
            warnings: vec![],
        };
        let document = ::markdown::Parser::new().parse(&::markdown::Source::in_memory("## An example\n\nA **useful** [reference](two.html).\n\n![Alt](/static/figure.png)\n\n````rust\nlet fence = \"```\";\n````\n\n| First | Second |\n| --- | --- |\n| a\\|b | `x\\|y` |\n")).unwrap();
        let output = edition.render(&document).unwrap();
        assert!(output.contains("**useful**"));
        assert!(output.contains("https://example.test/notebook/notes/two.html"));
        assert!(output.contains("https://example.test/notebook/static/figure.png"));
        assert!(output.contains("let fence = \"```\";"));
        assert!(output.contains("a\\|b"));
        assert!(output.contains("x\\|y"));
        assert!(
            output.ends_with(
                "Originally published at https://example.test/notebook/notes/one.html.\n"
            )
        );
    }

    #[test]
    fn document_references_use_target_fragment_and_never_the_source_mention_anchor() {
        let original = Url::parse("https://example.test/one.html").unwrap();
        let documents = HashMap::from([(
            ContentId("two".into()),
            "https://example.test/two.html".into(),
        )]);
        let mut edition = Edition {
            original: &original,
            mount: &original,
            documents: &documents,
            warnings: vec![],
        };
        let output = edition
            .inlines(&[Inline::DocumentLink(berlin_document::DocumentLink {
                target: ContentId("two".into()),
                fragment: Some("target-section".into()),
                anchor: berlin_document::ComponentId("source-mention".into()),
                title: None,
                content: vec![Inline::Text {
                    value: "Other article".into(),
                }],
            })])
            .unwrap();
        assert!(output.contains("two.html#target-section"));
        assert!(!output.contains("source-mention"));
        assert!(edition.url("file:///private/notes.org").is_err());
    }

    #[test]
    fn html_fallback_preserves_word_boundaries_and_discloses_missing_interaction() {
        assert_eq!(
            plain_html("<p>First &amp; second.</p><p>Third.</p>").unwrap(),
            "First & second.\n\nThird."
        );
        let original = Url::parse("https://example.test/one.html").unwrap();
        let documents = HashMap::new();
        let mut edition = Edition {
            original: &original,
            mount: &original,
            documents: &documents,
            warnings: vec![],
        };
        assert!(
            edition
                .html("<iframe src=\"https://example.test/deck\"></iframe>")
                .unwrap()
                .contains("original article")
        );
        assert_eq!(edition.warnings.len(), 1);
        assert!(
            !edition
                .html("<script>secret()</script>")
                .unwrap()
                .contains("secret()")
        );
    }
}
