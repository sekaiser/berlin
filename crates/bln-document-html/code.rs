//! Static code listings and native inline explanations. Complete reference-led
//! lists become disclosures; ordinary prose and mixed lists keep their place.
//! Browser controls enhance, but never replace, the source text.

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
    inline_notes: HashMap<String, Vec<InlineNote>>,
}

#[derive(Clone)]
pub(super) struct InlineNote {
    pub anchor: String,
    pub content: Vec<Inline>,
}

pub(super) struct RenderedNote {
    pub anchor: String,
    pub html: String,
}

struct NoteCandidate {
    content: Vec<Inline>,
    list_start: usize,
    list_len: usize,
}

impl CodeListings {
    pub(super) fn for_document(document: &Document) -> Self {
        let mut targets = Vec::new();
        let mut links = Vec::new();
        let mut candidates = HashMap::new();
        collect_references(&document.blocks, &mut targets, &mut links, &mut candidates);
        let mut listings = Self {
            explanations: targets
                .into_iter()
                .map(|target| (target, Vec::new()))
                .collect(),
            ..Self::default()
        };
        // Relocate complete, unambiguous note lists only. A mixed list must
        // retain its original ordering, numbering, and surrounding context.
        let mut eligible_counts = HashMap::<usize, usize>::new();
        for candidate in candidates.values() {
            let mut references = Vec::new();
            collect_links(&candidate.content, &mut references);
            let is_code_reference = |link: &String| {
                link.strip_prefix('#')
                    .is_some_and(|target| listings.explanations.contains_key(target))
            };
            if references.first().is_some_and(is_code_reference)
                && references
                    .iter()
                    .filter(|link| is_code_reference(link))
                    .count()
                    == 1
            {
                *eligible_counts.entry(candidate.list_start).or_default() += 1;
            }
        }
        candidates.retain(|_, candidate| {
            eligible_counts.get(&candidate.list_start) == Some(&candidate.list_len)
        });
        for (index, link) in links.into_iter().enumerate() {
            let candidate = candidates.remove(&index).map(|candidate| candidate.content);
            if let Some(target) = link.strip_prefix('#')
                && let Some(notes) = listings.explanations.get_mut(target)
            {
                let anchor = format!("{target}-note-{}", notes.len() + 1);
                notes.push(anchor.clone());
                if let Some(content) = candidate {
                    listings
                        .inline_notes
                        .entry(target.into())
                        .or_default()
                        .push(InlineNote { anchor, content });
                }
            }
        }
        listings
    }

    pub(super) fn inline_notes(&self, code: &CodeBlock) -> HashMap<String, Vec<InlineNote>> {
        let Some(prefix) = &code.references.line_anchor_prefix else {
            return HashMap::new();
        };
        (0..code.value.lines().count())
            .filter_map(|offset| {
                let target = format!(
                    "{prefix}-{}",
                    code.references.first_line.saturating_add(offset)
                );
                self.inline_notes
                    .get(&target)
                    .map(|notes| (target, notes.clone()))
            })
            .collect()
    }

    pub(super) fn is_inline_note(&self, blocks: &[Block]) -> bool {
        let [Block::Paragraph { content }] = blocks else {
            return false;
        };
        let Some(Inline::Link { destination, .. }) = content.first() else {
            return false;
        };
        destination
            .strip_prefix('#')
            .and_then(|target| self.inline_notes.get(target))
            .is_some_and(|notes| notes.iter().any(|note| note.content == *content))
    }

    /// Gives each prose reference its own return destination, including repeated references.
    pub(super) fn explanation_anchor(&mut self, destination: &str) -> Option<String> {
        let target = destination.strip_prefix('#')?;
        let explanations = self.explanations.get(target)?;
        let index = self.rendered_explanations.entry(target.into()).or_default();
        while explanations.get(*index).is_some_and(|anchor| {
            self.inline_notes
                .get(target)
                .is_some_and(|notes| notes.iter().any(|note| &note.anchor == anchor))
        }) {
            *index += 1;
        }
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
        line_highlighter: Option<&super::highlight::LineHighlighter>,
        notes: &HashMap<String, Vec<RenderedNote>>,
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
        if !notes.is_empty() {
            self.write_annotated(
                output,
                &id,
                code,
                line_highlighter.expect("annotated code has a line highlighter"),
                notes,
            )?;
            output.push_str("</figure>\n");
            return Ok(());
        }
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

    fn write_annotated(
        &self,
        output: &mut String,
        id: &str,
        code: &CodeBlock,
        highlighter: &super::highlight::LineHighlighter,
        notes: &HashMap<String, Vec<RenderedNote>>,
    ) -> fmt::Result {
        // JSON preserves CRLFs and exact trailing newlines. Escape '<' so source
        // text cannot terminate the inert script element.
        let source = serde_json::to_string(&code.value)
            .expect("strings serialize")
            .replace('<', "\\u003c")
            .replace('>', "\\u003e")
            .replace('&', "\\u0026");
        write!(
            output,
            "<script type=\"application/json\" class=\"code-source\">{source}</script><div class=\"code-lines\" tabindex=\"0\" aria-label=\"Annotated code listing\"><div class=\"code-rows\">"
        )?;
        for (offset, html) in highlighter
            .lines(code.language.as_deref(), &code.value)
            .iter()
            .enumerate()
        {
            let line = code.references.first_line.saturating_add(offset);
            let target = format!(
                "{}-{line}",
                code.references
                    .line_anchor_prefix
                    .as_deref()
                    .expect("annotated code has a prefix")
            );
            let highlighted = code
                .highlights
                .iter()
                .any(|range| range.start <= offset + 1 && offset < range.end);
            write!(
                output,
                "<div class=\"code-row{}\" id=\"{}\"><span class=\"code-line-anchor\" id=\"{id}-L{line}\"></span><pre><code>{html}</code></pre>",
                if highlighted { " is-highlighted" } else { "" },
                escape_attribute(&target)
            )?;
            if let Some(notes) = notes.get(&target) {
                write!(
                    output,
                    "<details class=\"code-note\"><summary aria-label=\"Notes for line {line}\">Note</summary><div class=\"code-explanations\">"
                )?;
                for note in notes {
                    write!(
                        output,
                        "<div class=\"code-explanation\" id=\"{}\">{}</div>",
                        escape_attribute(&note.anchor),
                        note.html
                    )?;
                }
                output.push_str("</div></details>");
            }
            // Keep return links for references that remain ordinary prose.
            if let Some(anchors) = self.explanations.get(&target) {
                for anchor in anchors.iter().filter(|anchor| {
                    !self
                        .inline_notes
                        .get(&target)
                        .is_some_and(|notes| notes.iter().any(|note| &note.anchor == *anchor))
                }) {
                    write!(
                        output,
                        "<a class=\"code-context-link\" href=\"#{}\">Context</a>",
                        escape_attribute(anchor)
                    )?;
                }
            }
            output.push_str("</div>");
        }
        output.push_str("</div></div>");
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

fn collect_references(
    blocks: &[Block],
    targets: &mut Vec<String>,
    links: &mut Vec<String>,
    candidates: &mut HashMap<usize, NoteCandidate>,
) {
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
                collect_references(blocks, targets, links, candidates);
            }
            Block::Component { blocks, .. }
            | Block::BlockQuote { blocks }
            | Block::FootnoteDefinition { blocks, .. } => {
                collect_references(blocks, targets, links, candidates)
            }
            Block::List(list) => {
                let list_start = links.len();
                let eligible = list.items.iter().all(|item| {
                    item.checked.is_none()
                        && matches!(item.blocks.as_slice(), [Block::Paragraph { content }]
                        if matches!(content.first(), Some(Inline::Link { .. })))
                });
                for item in &list.items {
                    let index = links.len();
                    collect_references(&item.blocks, targets, links, candidates);
                    if eligible
                        && let [Block::Paragraph { content }] = item.blocks.as_slice()
                        && matches!(content.first(), Some(Inline::Link { .. }))
                    {
                        candidates.insert(
                            index,
                            NoteCandidate {
                                content: content.clone(),
                                list_start,
                                list_len: list.items.len(),
                            },
                        );
                    }
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

pub(super) fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
