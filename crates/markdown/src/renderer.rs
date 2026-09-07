//! Direct HTML projection for Markdown that has not entered the semantic model.

use std::fmt;

use comrak::Arena;
use comrak::Options as ComrakOptions;
use comrak::format_html_with_plugins;
use comrak::nodes::AstNode;
use comrak::nodes::NodeValue;
use comrak::options::Plugins as ComrakPlugins;
use comrak::parse_document;
use comrak::plugins::syntect::SyntectAdapter;

use crate::Error;
use crate::Source;
use crate::front_matter;
use crate::options::berlin_markdown_options;
use crate::shortcode;

/// Rendered HTML produced from Berlin's Markdown dialect.
///
/// Raw HTML from the source is preserved; this type does not imply sanitization.
#[derive(Clone, Debug, Eq, PartialEq)]
#[must_use]
pub struct Html(String);

impl Html {
    /// Borrows the rendered HTML.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the value and returns its rendered HTML.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for Html {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for Html {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Direct HTML renderer for Berlin's Ox-Hugo-compatible Markdown dialect.
pub struct Renderer {
    options: ComrakOptions<'static>,
    syntax_highlighter: SyntectAdapter,
}

impl Renderer {
    /// Renders a validated source, including shortcode expansion and syntax highlighting.
    pub fn render(&self, source: &Source<'_>) -> Result<Html, Error> {
        let normalized = normalize_shortcodes(source)?;
        let arena = Arena::new();
        let root = parse_document(&arena, &normalized, &self.options);
        front_matter::parse(root, source.uri().as_str())?;
        remove_ox_hugo_heading_markers(root);

        let mut plugins = ComrakPlugins::default();
        plugins.render.codefence_syntax_highlighter = Some(&self.syntax_highlighter);
        Ok(Html(render_html(root, &self.options, &plugins)))
    }
}

impl Default for Renderer {
    /// Creates a renderer with Berlin's canonical Markdown configuration.
    fn default() -> Self {
        Self {
            options: berlin_markdown_options(),
            syntax_highlighter: SyntectAdapter::new(Some("base16-ocean.dark")),
        }
    }
}

fn normalize_shortcodes(source: &Source<'_>) -> Result<String, Error> {
    let mut normalized = source.text().to_owned();
    let mut shortcodes = shortcode::parse(source)?;
    // Apply replacements bottom-up so earlier source spans remain valid.
    shortcodes.reverse();
    for shortcode in shortcodes {
        if let Some(body) = shortcode.body {
            normalized.replace_range(shortcode.span, &body);
        }
    }
    Ok(normalized)
}

fn remove_ox_hugo_heading_markers<'a>(root: &'a AstNode<'a>) {
    for node in root.descendants() {
        if let NodeValue::Text(ref mut text) = node.data.borrow_mut().value
            && let Some(parent) = node.parent()
            && matches!(parent.data.borrow().value, NodeValue::Heading(_))
            && let Some(pos) = text.find(" {")
        {
            // Ox-Hugo emits headings as `Header {#header}`. Strip the explicit
            // marker after Comrak has used it to derive the heading ID.
            text.to_mut().truncate(pos);
        }
    }
}

fn render_html<'a>(
    root: &'a AstNode<'a>,
    options: &ComrakOptions,
    plugins: &ComrakPlugins,
) -> String {
    let mut output = String::new();
    format_html_with_plugins(root, options, &mut output, plugins)
        .expect("rendering Markdown to a String cannot fail");
    output
}
