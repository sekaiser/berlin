//! HTML rendering for Berlin's semantic document model.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use berlin_document::Block;
use berlin_document::CodeBlock;
use berlin_document::ComponentId;
use berlin_document::Document;
use berlin_document::Figure;
use berlin_document::Inline;
use berlin_document::List;
use berlin_document::ListItem;
use berlin_document::PropertyValue;
use berlin_document::Table;
use berlin_document::TableAlignment;
use comrak::plugins::syntect::SyntectAdapter;

use crate::Html;
use crate::code::CodeListings;

/// Projects Berlin semantic documents into HTML.
pub struct Renderer {
    syntax_highlighter: SyntectAdapter,
}

impl Renderer {
    /// Renders a semantic document as typed HTML.
    pub fn render(&self, document: &Document) -> Html {
        RenderSession::new(&self.syntax_highlighter).render(document)
    }
}

impl Default for Renderer {
    /// Creates a renderer with Berlin's canonical syntax-highlighting theme.
    fn default() -> Self {
        Self {
            syntax_highlighter: SyntectAdapter::new(Some("InspiredGitHub")),
        }
    }
}

struct RenderSession<'a> {
    syntax_highlighter: &'a SyntectAdapter,
    output: String,
    code_listings: CodeListings,
}

impl<'a> RenderSession<'a> {
    fn new(syntax_highlighter: &'a SyntectAdapter) -> Self {
        Self {
            syntax_highlighter,
            output: String::new(),
            code_listings: CodeListings::default(),
        }
    }

    fn render(mut self, document: &Document) -> Html {
        self.code_listings = CodeListings::for_document(document);
        self.render_blocks(&document.blocks);
        Html::new(self.output)
    }

    fn render_blocks(&mut self, blocks: &[Block]) {
        for block in blocks {
            self.render_block(block);
        }
    }

    fn render_block(&mut self, block: &Block) {
        match block {
            Block::Paragraph { content } => self.render_paragraph(content),
            Block::Heading { level, id, content } => {
                self.render_heading(*level, id.as_deref(), content)
            }
            Block::BlockQuote { blocks } => self.render_block_quote(blocks),
            Block::List(list) => self.render_list(list),
            Block::Code(code) => self.render_code(code),
            Block::Figure(figure) => self.render_figure(figure),
            Block::Html { value } => self.render_html(value),
            Block::Table(table) => self.render_table(table),
            Block::ThematicBreak => self.output.push_str("<hr />\n"),
            Block::FootnoteDefinition { name, blocks } => {
                self.render_footnote_definition(name, blocks)
            }
            Block::Section {
                id,
                level,
                role,
                title,
                blocks,
            } => self.render_section(id, *level, role.as_deref(), title, blocks),
            Block::Component {
                id,
                name,
                properties,
                blocks,
            } => self.render_component(id, name, properties, blocks),
        }
    }

    fn render_paragraph(&mut self, content: &[Inline]) {
        self.output.push_str("<p>");
        self.render_inlines(content);
        self.output.push_str("</p>\n");
    }

    fn render_block_quote(&mut self, blocks: &[Block]) {
        self.output.push_str("<blockquote>\n");
        self.render_blocks(blocks);
        self.output.push_str("</blockquote>\n");
    }

    fn render_code(&mut self, code: &CodeBlock) {
        let caption = code
            .caption
            .as_ref()
            .map(|content| self.render_fragment(content));
        self.code_listings
            .write(
                self.syntax_highlighter,
                &mut self.output,
                code,
                caption.as_deref(),
            )
            .expect("writing highlighted code to a String cannot fail");
    }

    /// Captures inline markup while retaining this document's reference context.
    fn render_fragment(&mut self, content: &[Inline]) -> String {
        let document_output = std::mem::take(&mut self.output);
        self.render_inlines(content);
        std::mem::replace(&mut self.output, document_output)
    }

    fn render_html(&mut self, value: &str) {
        self.output.push_str(value);
        if !value.ends_with('\n') {
            self.output.push('\n');
        }
    }

    fn render_footnote_definition(&mut self, name: &str, blocks: &[Block]) {
        let name = escape_attribute(name);
        let _ = writeln!(
            self.output,
            "<section class=\"footnote-definition\" id=\"fn-{name}\">"
        );
        self.render_blocks(blocks);
        self.output.push_str("</section>\n");
    }

    fn render_section(
        &mut self,
        id: &ComponentId,
        level: u8,
        role: Option<&str>,
        title: &[Inline],
        blocks: &[Block],
    ) {
        if let Some(role) = role {
            let _ = writeln!(
                self.output,
                "<section data-role=\"{}\">",
                escape_attribute(role)
            );
        }
        self.render_heading(level, Some(&id.0), title);
        self.render_blocks(blocks);
        if role.is_some() {
            self.output.push_str("</section>\n");
        }
    }

    fn render_component(
        &mut self,
        id: &ComponentId,
        name: &str,
        properties: &BTreeMap<String, PropertyValue>,
        blocks: &[Block],
    ) {
        let _ = write!(
            self.output,
            "<div id=\"{}\" data-component=\"{}\"",
            escape_attribute(&id.0),
            escape_attribute(name)
        );
        for (key, value) in properties {
            if let Some(value) = property_attribute(value) {
                let _ = write!(
                    self.output,
                    " data-{}=\"{}\"",
                    escape_attribute(key),
                    escape_attribute(value)
                );
            }
        }
        self.output.push_str(">\n");
        self.render_blocks(blocks);
        self.output.push_str("</div>\n");
    }

    fn render_heading(&mut self, level: u8, id: Option<&str>, content: &[Inline]) {
        let id = id.map(str::to_owned).unwrap_or_else(|| heading_id(content));
        let _ = write!(self.output, "<h{level}>");
        if !id.is_empty() {
            let escaped = escape_attribute(&id);
            let _ = write!(
                self.output,
                "<a href=\"#{escaped}\" aria-hidden=\"true\" class=\"anchor\" id=\"{escaped}\"></a>"
            );
        }
        self.render_inlines(content);
        let _ = writeln!(self.output, "</h{level}>");
    }

    fn render_list(&mut self, list: &List) {
        let tag = if list.ordered { "ol" } else { "ul" };
        let _ = write!(self.output, "<{tag}");
        if let (true, Some(start)) = (list.ordered, list.start)
            && start != 1
        {
            let _ = write!(self.output, " start=\"{start}\"");
        }
        self.output.push_str(">\n");
        for item in &list.items {
            self.render_list_item(item, list.tight);
        }
        let _ = writeln!(self.output, "</{tag}>");
    }

    fn render_list_item(&mut self, item: &ListItem, tight: bool) {
        self.output.push_str("<li>");
        if let Some(checked) = item.checked {
            self.output
                .push_str("<input type=\"checkbox\" disabled=\"\"");
            if checked {
                self.output.push_str(" checked=\"\"");
            }
            self.output.push_str(" /> ");
        }
        for block in &item.blocks {
            if tight && let Block::Paragraph { content } = block {
                self.render_inlines(content);
                continue;
            }
            self.render_block(block);
        }
        self.output.push_str("</li>\n");
    }

    fn render_figure(&mut self, figure: &Figure) {
        let source = escape_attribute(&figure.source);
        match &figure.caption {
            Some(caption) => {
                let _ = writeln!(
                    self.output,
                    "<figure><img style=\"max-width:100%;\" \
                     src=\"/static{source}\"><figcaption>{caption}</figcaption></figure>"
                );
            }
            None => {
                let _ = writeln!(
                    self.output,
                    "<img style=\"width:456px;margin-top:5px;margin-bottom:5px;\" src=\"{source}\">"
                );
            }
        }
    }

    fn render_table(&mut self, table: &Table) {
        self.output.push_str("<table>\n");
        for row in &table.rows {
            self.output.push_str("<tr>\n");
            let cell_tag = if row.header { "th" } else { "td" };
            for (index, cell) in row.cells.iter().enumerate() {
                let _ = write!(self.output, "<{cell_tag}");
                if let Some(alignment) = table.alignments.get(index).and_then(table_alignment) {
                    let _ = write!(self.output, " align=\"{alignment}\"");
                }
                self.output.push('>');
                self.render_inlines(cell);
                let _ = writeln!(self.output, "</{cell_tag}>");
            }
            self.output.push_str("</tr>\n");
        }
        self.output.push_str("</table>\n");
    }

    fn render_inlines(&mut self, inlines: &[Inline]) {
        for inline in inlines {
            self.render_inline(inline);
        }
    }

    fn render_inline(&mut self, inline: &Inline) {
        match inline {
            Inline::Text { value } => self.output.push_str(&escape_text(value)),
            Inline::Emphasis { content } => self.render_wrapped("em", content),
            Inline::Strong { content } => self.render_wrapped("strong", content),
            Inline::Strikethrough { content } => self.render_wrapped("del", content),
            Inline::Code { value } => self.render_inline_code(value),
            Inline::Link {
                destination,
                title,
                content,
            } => self.render_link(destination, title.as_deref(), content),
            Inline::Image {
                source,
                title,
                description,
            } => self.render_image(source, title.as_deref(), description),
            Inline::Html { value } => self.output.push_str(value),
            Inline::SoftBreak => self.output.push('\n'),
            Inline::LineBreak => self.output.push_str("<br />\n"),
            Inline::FootnoteReference { name } => self.render_footnote_reference(name),
        }
    }

    fn render_inline_code(&mut self, value: &str) {
        self.output.push_str("<code>");
        self.output.push_str(&escape_text(value));
        self.output.push_str("</code>");
    }

    fn render_link(&mut self, destination: &str, title: Option<&str>, content: &[Inline]) {
        let explanation = self.code_listings.explanation_anchor(destination);
        let destination = escape_attribute(destination);
        let _ = write!(self.output, "<a href=\"{destination}\"");
        if let Some(id) = explanation {
            let _ = write!(
                self.output,
                " class=\"code-reference\" id=\"{}\"",
                escape_attribute(&id)
            );
        }
        if let Some(title) = title {
            let _ = write!(self.output, " title=\"{}\"", escape_attribute(title));
        }
        self.output.push('>');
        self.render_inlines(content);
        self.output.push_str("</a>");
    }

    fn render_image(&mut self, source: &str, title: Option<&str>, description: &[Inline]) {
        let _ = write!(
            self.output,
            "<img src=\"{}\" alt=\"{}\"",
            escape_attribute(source),
            escape_attribute(&plain_text(description))
        );
        if let Some(title) = title {
            let _ = write!(self.output, " title=\"{}\"", escape_attribute(title));
        }
        self.output.push_str(" />");
    }

    fn render_footnote_reference(&mut self, name: &str) {
        let name = escape_attribute(name);
        let _ = write!(
            self.output,
            "<sup class=\"footnote-ref\"><a href=\"#fn-{name}\">{name}</a></sup>"
        );
    }

    fn render_wrapped(&mut self, tag: &str, content: &[Inline]) {
        let _ = write!(self.output, "<{tag}>");
        self.render_inlines(content);
        let _ = write!(self.output, "</{tag}>");
    }
}

fn property_attribute(value: &PropertyValue) -> Option<&str> {
    match value {
        PropertyValue::Text(value) => Some(value),
        _ => None,
    }
}

fn table_alignment(alignment: &TableAlignment) -> Option<&'static str> {
    match alignment {
        TableAlignment::None => None,
        TableAlignment::Left => Some("left"),
        TableAlignment::Center => Some("center"),
        TableAlignment::Right => Some("right"),
    }
}

fn heading_id(content: &[Inline]) -> String {
    let text = plain_text(content).to_lowercase();
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
    id
}

fn plain_text(inlines: &[Inline]) -> String {
    let mut output = String::new();
    for inline in inlines {
        match inline {
            Inline::Text { value } | Inline::Code { value } => output.push_str(value),
            Inline::Emphasis { content }
            | Inline::Strong { content }
            | Inline::Strikethrough { content }
            | Inline::Link { content, .. } => output.push_str(&plain_text(content)),
            Inline::Image { description, .. } => output.push_str(&plain_text(description)),
            Inline::SoftBreak | Inline::LineBreak => output.push(' '),
            Inline::Html { .. } | Inline::FootnoteReference { .. } => {}
        }
    }
    output
}

fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attribute(value: &str) -> String {
    escape_text(value).replace('"', "&quot;")
}
