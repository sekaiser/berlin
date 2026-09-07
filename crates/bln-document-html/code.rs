//! Static code listings. Browser controls enhance, but never replace, the source text.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{self, Write};

use berlin_document::{Block, CodeBlock, Document, Inline};
use comrak::adapters::SyntaxHighlighterAdapter;
use comrak::plugins::syntect::SyntectAdapter;
use sha2::{Digest, Sha256};

/// Resolves author-assigned listing anchors and explanation return destinations.
/// Unnamed listings fall back to content fingerprints scoped to this document.
#[derive(Default)]
pub(super) struct CodeListings {
    occurrences: HashMap<String, usize>,
    explanations: HashMap<String, Vec<String>>,
    rendered_explanations: HashMap<String, usize>,
}

impl CodeListings {
    pub(super) fn for_document(document: &Document) -> Self {
        let mut targets = Vec::new();
        let mut links = Vec::new();
        collect_references(&document.blocks, &mut targets, &mut links);
        let mut listings = Self {
            explanations: targets
                .into_iter()
                .map(|target| (target, Vec::new()))
                .collect(),
            ..Self::default()
        };
        for link in links {
            if let Some(target) = link.strip_prefix('#')
                && let Some(notes) = listings.explanations.get_mut(target)
            {
                notes.push(format!("{target}-note-{}", notes.len() + 1));
            }
        }
        listings
    }

    /// Gives each prose reference its own return destination, including repeated references.
    pub(super) fn explanation_anchor(&mut self, destination: &str) -> Option<String> {
        let target = destination.strip_prefix('#')?;
        let explanations = self.explanations.get(target)?;
        let index = self.rendered_explanations.entry(target.into()).or_default();
        let anchor = explanations.get(*index)?.clone();
        *index += 1;
        Some(anchor)
    }

    pub(super) fn write(
        &mut self,
        highlighter: &SyntectAdapter,
        output: &mut String,
        code: &CodeBlock,
        caption: Option<&str>,
    ) -> fmt::Result {
        let id = escape_attribute(&self.anchor(code));
        let language = code.language.as_deref().filter(|value| !value.is_empty());
        let label = match language {
            None | Some("nil" | "text" | "plaintext") => "Text",
            Some(language) => language,
        };
        write!(output, "<figure class=\"code-listing\" id=\"{id}\">")?;
        write!(
            output,
            "<figcaption class=\"code-toolbar\"><span class=\"code-language\">{}</span>",
            escape(label)
        )?;
        if let Some(caption) = caption {
            write!(output, "<span class=\"code-caption\">{caption}</span>")?;
        }
        write!(
            output,
            "<span class=\"code-actions\"><a href=\"#{id}\" aria-label=\"Link to code listing\">Link</a><button type=\"button\" class=\"code-copy\" hidden>Copy</button></span><span class=\"code-status\" role=\"status\"></span></figcaption>"
        )?;
        output.push_str("<div class=\"code-content\">");
        let lines = code.value.lines().count();
        if lines > 1 || code.references.line_anchor_prefix.is_some() || !code.highlights.is_empty()
        {
            self.write_line_links(output, &id, code)?;
        }
        output.push_str("<pre tabindex=\"0\" aria-label=\"Code listing\">");
        let mut attributes = HashMap::new();
        if let Some(language) = language {
            attributes.insert("class", Cow::Owned(format!("language-{language}")));
        }
        highlighter.write_code_tag(output, attributes)?;
        highlighter.write_highlighted(output, language, &code.value)?;
        output.push_str("</code></pre></div></figure>\n");
        Ok(())
    }

    fn anchor(&mut self, code: &CodeBlock) -> String {
        if let Some(id) = &code.references.id {
            return id.clone();
        }
        let mut digest = Sha256::new();
        digest.update(code.language.as_deref().unwrap_or_default());
        digest.update([0]);
        digest.update(&code.value);
        let hash = digest.finalize();
        let prefix = u64::from_be_bytes(hash[..8].try_into().expect("SHA-256 has 32 bytes"));
        let base = format!("code-{prefix:016x}");
        let occurrence = self.occurrences.entry(base.clone()).or_default();
        *occurrence += 1;
        if *occurrence == 1 {
            base
        } else {
            format!("{base}-{occurrence}")
        }
    }

    fn write_line_links(&self, output: &mut String, id: &str, code: &CodeBlock) -> fmt::Result {
        output.push_str("<div class=\"code-gutter\" aria-label=\"Code line links\">");
        for offset in 0..code.value.lines().count() {
            let highlighted = code
                .highlights
                .iter()
                .any(|range| range.start <= offset + 1 && offset < range.end);
            let class = if highlighted {
                "code-line-reference is-highlighted"
            } else {
                "code-line-reference"
            };
            let line = code.references.first_line.saturating_add(offset);
            let target = code
                .references
                .line_anchor_prefix
                .as_ref()
                .map(|prefix| format!("{prefix}-{line}"));
            if let Some(target) = &target {
                write!(
                    output,
                    "<div class=\"{class}\" id=\"{}\" style=\"--line-offset:{offset}\">",
                    escape_attribute(target)
                )?;
            } else {
                write!(
                    output,
                    "<div class=\"{class}\" style=\"--line-offset:{offset}\">"
                )?;
            }
            write!(
                output,
                "<a id=\"{id}-L{line}\" href=\"#{id}-L{line}\" aria-label=\"Link to line {line}\" data-line=\"{line}\"></a>"
            )?;
            if let Some(notes) = target
                .as_ref()
                .and_then(|target| self.explanations.get(target))
            {
                for (index, note) in notes.iter().enumerate() {
                    write!(
                        output,
                        "<a class=\"code-note-link\" href=\"#{}\" aria-label=\"Explanation {} for line {line}\">{}</a>",
                        escape_attribute(note),
                        index + 1,
                        if notes.len() == 1 {
                            "Note".into()
                        } else {
                            format!("Note {}", index + 1)
                        }
                    )?;
                }
            }
            output.push_str("</div>");
        }
        output.push_str("</div>");
        Ok(())
    }
}

fn escape_attribute(value: &str) -> String {
    escape(value).replace('"', "&quot;")
}

fn collect_references(blocks: &[Block], targets: &mut Vec<String>, links: &mut Vec<String>) {
    for block in blocks {
        match block {
            Block::Code(code) => {
                if let Some(caption) = &code.caption {
                    collect_links(caption, links);
                }
                if let Some(prefix) = &code.references.line_anchor_prefix {
                    targets.extend((0..code.value.lines().count()).map(|offset| {
                        format!(
                            "{prefix}-{}",
                            code.references.first_line.saturating_add(offset)
                        )
                    }));
                }
            }
            Block::Paragraph { content } | Block::Heading { content, .. } => {
                collect_links(content, links)
            }
            Block::Section { title, blocks, .. } => {
                collect_links(title, links);
                collect_references(blocks, targets, links);
            }
            Block::Component { blocks, .. }
            | Block::BlockQuote { blocks }
            | Block::FootnoteDefinition { blocks, .. } => {
                collect_references(blocks, targets, links)
            }
            Block::List(list) => {
                for item in &list.items {
                    collect_references(&item.blocks, targets, links);
                }
            }
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_links(cell, links);
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_links(inlines: &[Inline], links: &mut Vec<String>) {
    for inline in inlines {
        match inline {
            Inline::Link {
                destination,
                content,
                ..
            } => {
                links.push(destination.clone());
                collect_links(content, links);
            }
            Inline::Emphasis { content }
            | Inline::Strong { content }
            | Inline::Strikethrough { content } => collect_links(content, links),
            _ => {}
        }
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
