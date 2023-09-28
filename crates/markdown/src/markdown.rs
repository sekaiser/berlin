use crate::shortcode::parse_for_shortcodes;
use berlin_core::{FrontMatter, ModuleSpecifier};
use libs::comrak::nodes::{AstNode, NodeValue};
use libs::comrak::plugins::syntect::SyntectAdapter;
use libs::comrak::{format_html_with_plugins, parse_document, Arena, ComrakOptions, ComrakPlugins};
use libs::lazy_static;
pub use libs::regex::Regex;
use serde::Deserialize;
use std::borrow::BorrowMut;
use std::sync::Arc;

lazy_static::lazy_static! {
    static ref RELREF_RE: Regex = Regex::new(r#"\[(?P<label>.+)\]\(\{\{< relref "(?P<name>.*)" >\}\}\)"#).unwrap();
}

#[derive(Clone)]
pub struct MarkdownOptions;

impl MarkdownOptions {
    pub fn default() -> ComrakOptions {
        let mut options = ComrakOptions::default();
        options.extension.front_matter_delimiter = Some("---".to_owned());
        options.extension.table = true;
        options.extension.strikethrough = true;
        options.extension.tagfilter = true;
        options.render.unsafe_ = true;
        options.extension.autolink = true;
        options.extension.tasklist = true;
        options.extension.header_ids = Some("".to_string());
        options.extension.footnotes = true;
        options.extension.description_lists = true;

        options
    }
}

pub fn handle_shortcodes(specifier: &ModuleSpecifier, content: &mut String) {
    if let Ok((_name, mut shortcodes)) = parse_for_shortcodes(&specifier, &content) {
        // the ranges of the shortcodes are computed based on the original file
        // and differences in ranges after a rendering step of a short code
        // are not considered.
        // So applying the shortcodes sequentially without taking the
        // change of ranges into considerations leads to malformed output.
        // We could add logic to update the ranges by an offset that gets updated
        // after the application of each shortcode
        // OR
        // we simply reverse the array and update the file bottom-up instead of
        // top-down.
        shortcodes.reverse();
        for sc in shortcodes {
            if let Some(ref body) = sc.body {
                content.replace_range(sc.span, body);
            }
        }
    }
}

pub fn string_to_html(source: &String, options: &ComrakOptions) -> String {
    let arena = Arena::new();
    let mut html = Vec::new();
    let plugins = ComrakPlugins::default();
    let root = parse_document(&arena, &source, &options);
    format_html_with_plugins(root, &options, &mut html, &plugins).unwrap();
    String::from_utf8(html).unwrap()
}

pub fn markdown_to_html(
    source: Arc<str>,
    options: ComrakOptions,
) -> (Option<FrontMatter>, Vec<u8>) {
    // let preprocessed_source = RELREF_RE.replace_all(&content, |caps: &Captures| {
    //     format!("[{}](/notes/{}.html)", &caps["label"], &caps["name"])
    // });

    let mut maybe_front_matter = None;
    let arena = Arena::new();
    let root = parse_document(&arena, &source, &options);

    fn iter_nodes<'a, F>(node: &'a AstNode<'a>, f: &mut F)
    where
        F: FnMut(&'a AstNode<'a>),
    {
        f(node);
        for ref mut c in node.children() {
            iter_nodes(c, f);
        }
    }

    iter_nodes(root, &mut |node| {
        if let NodeValue::FrontMatter(ref mut text) = node.data.borrow_mut().value {
            let mut documents = libs::serde_yaml::Deserializer::from_slice(text);
            if let Some(document) = documents.nth(1) {
                maybe_front_matter = FrontMatter::deserialize(document).ok();
            }
        }
        if let NodeValue::Text(ref mut text) = node.data.borrow_mut().value {
            if let Some(parent) = node.parent().borrow_mut() {
                // ox-hugo generates anchor links of the form `# Header {#header},
                // comrak interprets the whole line as text, which is not wanted here.
                // So we need to remove it`
                if let NodeValue::Heading(_) = parent.data.borrow().value {
                    for i in 0..text.len() {
                        let j = match i {
                            0 => 0,
                            n => n - 1,
                        };
                        if text[i] == 123 && text[j] == 32 {
                            *text = text[..j].to_vec();
                            break;
                        }
                    }
                }
            }
        }
    });

    let mut html = Vec::new();
    let mut plugins = ComrakPlugins::default();
    let adapter = SyntectAdapter::new("base16-ocean.dark");
    plugins.render.codefence_syntax_highlighter = Some(&adapter);
    format_html_with_plugins(root, &options, &mut html, &plugins).unwrap();

    (maybe_front_matter, html)
}
