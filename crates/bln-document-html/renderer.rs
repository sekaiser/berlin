//! HTML rendering for Berlin's semantic document model.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::OnceLock;

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
use berlin_document::{ContentId, DocumentLink};
use comrak::plugins::syntect::SyntectAdapter;

use crate::Html;
use crate::code::{CodeListings, InlineNote, RenderedNote};
use crate::math::MathRenderings;
use crate::output::Output;
use crate::{FragmentContext, FragmentError, SemanticComponentPresenter};

/// Projects Berlin semantic documents into HTML.
pub struct Renderer {
    syntax_highlighter: SyntectAdapter,
    line_highlighter: OnceLock<crate::highlight::LineHighlighter>,
    document_routes: HashMap<ContentId, String>,
}

impl Renderer {
    /// Renders borrowed, prevalidated structural blocks with occurrence-specific
    /// anchors and a fallible byte budget, leaving source documents untouched.
    pub fn render_fragment(
        &self,
        blocks: &[Block],
        context: &FragmentContext,
        presenter: Option<&dyn SemanticComponentPresenter>,
        limit: usize,
    ) -> Result<Html, FragmentError> {
        let math = MathRenderings::for_blocks(blocks).map_err(FragmentError::Math)?;
        let mut session = RenderSession::new(
            &self.syntax_highlighter,
            &self.line_highlighter,
            &self.document_routes,
            &math,
        );
        session.output = Output::bounded(limit);
        session.context = context.clone();
        session.presenter = presenter;
        session.code_listings = CodeListings::for_blocks(blocks).with_namespace(&context.namespace);
        if !math.is_empty() {
            let _ = writeln!(session.output, "<style>{}</style>", berlin_math::STYLESHEET);
        }
        session.render_blocks(blocks);
        if let Some(error) = session.error {
            return Err(error);
        }
        if session.output.failed {
            return Err(FragmentError::OutputLimit);
        }
        Ok(Html::new(session.output.into_string()))
    }
    /// Supplies channel-specific URLs; unresolved links render as labels, not broken URLs.
    pub fn with_document_routes(mut self, routes: HashMap<ContentId, String>) -> Self {
        self.document_routes = routes;
        self
    }
    /// Renders a semantic document as typed HTML.
    pub fn render(&self, document: &Document) -> Result<Html, crate::MathError> {
        let math = MathRenderings::for_document(document)?;
        Ok(RenderSession::new(
            &self.syntax_highlighter,
            &self.line_highlighter,
            &self.document_routes,
            &math,
        )
        .render(document))
    }
}

impl Default for Renderer {
    /// Creates a renderer with Berlin's canonical syntax-highlighting theme.
    fn default() -> Self {
        Self {
            syntax_highlighter: SyntectAdapter::new(Some("InspiredGitHub")),
            line_highlighter: OnceLock::new(),
            document_routes: HashMap::new(),
        }
    }
}

struct RenderSession<'a> {
    math: &'a MathRenderings,
    document_routes: &'a HashMap<ContentId, String>,
    syntax_highlighter: &'a SyntectAdapter,
    line_highlighter: &'a OnceLock<crate::highlight::LineHighlighter>,
    output: Output,
    code_listings: CodeListings,
    context: FragmentContext,
    presenter: Option<&'a dyn SemanticComponentPresenter>,
    error: Option<FragmentError>,
}

impl<'a> RenderSession<'a> {
    fn new(
        syntax_highlighter: &'a SyntectAdapter,
        line_highlighter: &'a OnceLock<crate::highlight::LineHighlighter>,
        document_routes: &'a HashMap<ContentId, String>,
        math: &'a MathRenderings,
    ) -> Self {
        Self {
            math,
            document_routes,
            syntax_highlighter,
            line_highlighter,
            output: Output::default(),
            code_listings: CodeListings::default(),
            context: FragmentContext::default(),
            presenter: None,
            error: None,
        }
    }

    fn render(mut self, document: &Document) -> Html {
        if !self.math.is_empty() {
            let _ = writeln!(self.output, "<style>{}</style>", berlin_math::STYLESHEET);
        }
        self.code_listings = CodeListings::for_document(document);
        self.render_blocks(&document.blocks);
        Html::new(self.output.into_string())
    }

    fn render_blocks(&mut self, blocks: &[Block]) {
        for block in blocks {
            if self.output.failed || self.error.is_some() {
                break;
            }
            self.render_block(block);
        }
    }

    fn render_block(&mut self, block: &Block) {
        match block {
            Block::Math { tex } => self.render_math(tex, true),
            Block::Equation(equation) => self.render_equation(equation),
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

    fn render_equation(&mut self, equation: &berlin_document::Equation) {
        let id = escape_attribute(&self.context.qualify(&equation.id.0));
        let number = escape_text(&equation.number);
        let _ = write!(self.output, "<div class=\"berlin-equation\" id=\"{id}\">");
        self.render_math(&equation.tex, true);
        let _ = writeln!(
            self.output,
            "<a class=\"berlin-equation-number\" href=\"#{id}\" aria-label=\"Equation {}\">({number})</a></div>",
            escape_attribute(&equation.number)
        );
    }

    fn render_math(&mut self, tex: &str, display: bool) {
        if display {
            self.output.push_str("<div class=\"berlin-math-display\" tabindex=\"0\" role=\"region\" aria-label=\"Equation\">");
        }
        self.output.push_str(self.math.get(tex, display));
        if display {
            self.output.push_str("</div>\n");
        }
    }

    fn render_block_quote(&mut self, blocks: &[Block]) {
        self.output.push_str("<blockquote>\n");
        self.render_blocks(blocks);
        self.output.push_str("</blockquote>\n");
    }

    fn render_code(&mut self, code: &CodeBlock) {
        let notes = self.render_code_notes(code);
        let caption = code
            .caption
            .as_ref()
            .map(|content| self.render_fragment(content));
        if self
            .code_listings
            .write(
                self.syntax_highlighter,
                &mut self.output,
                code,
                caption.as_deref(),
                (!notes.is_empty()).then(|| self.line_highlighter.get_or_init(Default::default)),
                &notes,
            )
            .is_err()
        {
            self.output.failed = true;
        }
    }

    fn render_code_notes(&mut self, code: &CodeBlock) -> HashMap<String, Vec<RenderedNote>> {
        self.code_listings
            .inline_notes(code)
            .into_iter()
            .map(|(target, notes)| {
                let notes = notes
                    .into_iter()
                    .map(|note| self.render_code_note(note))
                    .collect();
                (target, notes)
            })
            .collect()
    }

    fn render_code_note(&mut self, note: InlineNote) -> RenderedNote {
        let Inline::Link { content: label, .. } = &note.content[0] else {
            unreachable!("notes start with a reference");
        };
        let html = format!(
            "<p><strong>{}</strong>{}</p>",
            self.render_fragment(label),
            self.render_fragment(&note.content[1..])
        );
        RenderedNote {
            anchor: note.anchor,
            html,
        }
    }

    /// Captures inline markup while retaining this document's reference context.
    fn render_fragment(&mut self, content: &[Inline]) -> String {
        let fragment_output = self.output.fork();
        let document_output = std::mem::replace(&mut self.output, fragment_output);
        self.render_inlines(content);
        let fragment = std::mem::replace(&mut self.output, document_output);
        if fragment.failed {
            self.output.failed = true;
        }
        fragment.into_string()
    }

    fn render_html(&mut self, value: &str) {
        self.output.push_str(value);
        if !value.ends_with('\n') {
            self.output.push('\n');
        }
    }

    fn render_footnote_definition(&mut self, name: &str, blocks: &[Block]) {
        let label = escape_text(name);
        let name = escape_attribute(&self.context.qualify(&format!("fn-{name}")));
        let _ = writeln!(
            self.output,
            "<section class=\"footnote-definition\" id=\"{name}\">"
        );
        if let [Block::Paragraph { content }, remaining @ ..] = blocks {
            let _ = write!(
                self.output,
                "<p><span class=\"footnote-label\">[{label}]</span> "
            );
            self.render_inlines(content);
            self.output.push_str("</p>\n");
            self.render_blocks(remaining);
        } else {
            let _ = write!(
                self.output,
                "<span class=\"footnote-label\">[{label}]</span> "
            );
            self.render_blocks(blocks);
        }
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
        if let Some(presenter) = self.presenter {
            let fragment_output = self.output.fork();
            let parent = std::mem::replace(&mut self.output, fragment_output);
            self.render_blocks(blocks);
            let children = std::mem::replace(&mut self.output, parent);
            if children.failed {
                self.output.failed = true;
                return;
            }
            match presenter.present(
                &ComponentId(self.context.qualify(&id.0)),
                name,
                properties,
                Html::new(children.into_string()),
            ) {
                Ok(html) => self.output.push_str(&html),
                Err(detail) => self.error = Some(FragmentError::Presentation(detail)),
            }
            return;
        }
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
        let shifted = i16::from(level) + i16::from(self.context.heading_offset);
        if !(1..=6).contains(&shifted) {
            self.error = Some(FragmentError::HeadingDepth);
            return;
        }
        let level = shifted as u8;
        let id = id.map(str::to_owned).unwrap_or_else(|| heading_id(content));
        let _ = write!(self.output, "<h{level}>");
        if !id.is_empty() {
            let escaped = escape_attribute(&self.context.qualify(&id));
            let _ = write!(
                self.output,
                "<a href=\"#{escaped}\" aria-hidden=\"true\" class=\"anchor\" id=\"{escaped}\"></a>"
            );
        }
        self.render_inlines(content);
        let _ = writeln!(self.output, "</h{level}>");
    }

    fn render_list(&mut self, list: &List) {
        if list
            .items
            .iter()
            .all(|item| item.checked.is_none() && self.code_listings.is_inline_note(&item.blocks))
        {
            return;
        }
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
        let source = escape_attribute(&self.context.destination(&figure.source));
        let alt = figure
            .alt
            .as_deref()
            .map(|alt| format!(" alt=\"{}\"", escape_attribute(alt)))
            .unwrap_or_default();
        match &figure.caption {
            Some(caption) => {
                let _ = writeln!(
                    self.output,
                    "<figure><img style=\"max-width:100%;\" \
                     src=\"{source}\"{alt}><figcaption>{caption}</figcaption></figure>"
                );
            }
            None => {
                let _ = writeln!(
                    self.output,
                    "<img style=\"width:456px;margin-top:5px;margin-bottom:5px;\" src=\"{source}\"{alt}>"
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
            Inline::Math { tex } => self.render_math(tex, false),
            Inline::CrossReference { target, content } => {
                self.render_link(&format!("#{}", target.0), None, content)
            }
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
            Inline::DocumentLink(link) => self.render_document_link(link),
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

    fn render_document_link(&mut self, link: &DocumentLink) {
        let _ = write!(
            self.output,
            "<span id=\"{}\" class=\"document-reference\">",
            escape_attribute(&self.context.qualify(&link.anchor.0))
        );
        if let Some(route) = self.document_routes.get(&link.target) {
            let destination = match &link.fragment {
                Some(fragment) => format!("{route}#{fragment}"),
                None => route.clone(),
            };
            self.render_link(&destination, link.title.as_deref(), &link.content);
        } else {
            self.render_inlines(&link.content);
        }
        self.output.push_str("</span>");
    }

    fn render_link(&mut self, destination: &str, title: Option<&str>, content: &[Inline]) {
        let explanation = self.code_listings.explanation_anchor(destination);
        let destination = escape_attribute(&self.context.destination(destination));
        let _ = write!(self.output, "<a href=\"{destination}\"");
        if let Some(id) = explanation {
            let _ = write!(
                self.output,
                " class=\"code-reference\" id=\"{}\"",
                escape_attribute(&self.context.qualify(&id))
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
            escape_attribute(&self.context.destination(source)),
            escape_attribute(&plain_text(description))
        );
        if let Some(title) = title {
            let _ = write!(self.output, " title=\"{}\"", escape_attribute(title));
        }
        self.output.push_str(" />");
    }

    fn render_footnote_reference(&mut self, name: &str) {
        let destination = escape_attribute(&self.context.destination(&format!("#fn-{name}")));
        let name = escape_attribute(name);
        let _ = write!(
            self.output,
            "<sup class=\"footnote-ref\"><a href=\"{destination}\">{name}</a></sup>"
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
            Inline::Math { tex } => output.push_str(tex),
            Inline::Text { value } | Inline::Code { value } => output.push_str(value),
            Inline::Emphasis { content }
            | Inline::Strong { content }
            | Inline::Strikethrough { content }
            | Inline::CrossReference { content, .. }
            | Inline::Link { content, .. } => output.push_str(&plain_text(content)),
            Inline::DocumentLink(link) => output.push_str(&plain_text(&link.content)),
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
