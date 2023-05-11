use crate::shortcode::parse_for_shortcodes;
use berlin_core::anyhow::Error;
use berlin_core::error::generic_error;
use berlin_core::{FrontMatter, MediaType, ModuleSpecifier, ParsedSource};
use comrak::nodes::{AstNode, NodeValue};
use comrak::plugins::syntect::SyntectAdapter;
use comrak::{format_html_with_plugins, parse_document, Arena, ComrakOptions, ComrakPlugins};
use regex::{Captures, Regex};
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

pub fn handle_shortcodes(content: &mut String) {
    if let Ok((name, shortcodes)) = parse_for_shortcodes(&content) {
        for sc in shortcodes {
            if let Some(ref body) = sc.body {
                content.replace_range(sc.span, body);
            }
        }
    }
}

fn extract_front_matter(markdown: &str) -> Option<FrontMatter> {
    let mut front_matter = String::default();
    let mut sentinel = false;
    let lines = markdown.lines();
    for line in lines.clone() {
        if line.trim() == "---" {
            if sentinel {
                break;
            }

            sentinel = true;
            continue;
        }

        if sentinel {
            front_matter.push_str(line);
            front_matter.push('\n');
        }
    }

    if front_matter.is_empty() {
        None
    } else {
        Some(serde_yaml::from_str::<FrontMatter>(&front_matter).unwrap())
    }
}

pub fn markdown_to_html(
    source: Arc<str>,
    options: ComrakOptions,
) -> (Option<FrontMatter>, Vec<u8>) {
    let mut content = source.to_string();
    handle_shortcodes(&mut content);

    let preprocessed_source = RELREF_RE.replace_all(&content, |caps: &Captures| {
        format!("[{}](/notes/{}.html)", &caps["label"], &caps["name"])
    });

    let maybe_front_matter = extract_front_matter(&preprocessed_source);
    let arena = Arena::new();
    let root = parse_document(&arena, &preprocessed_source, &options);

    fn iter_nodes<'a, F>(node: &'a AstNode<'a>, f: &F)
    where
        F: Fn(&'a AstNode<'a>),
    {
        f(node);
        for c in node.children() {
            iter_nodes(c, f);
        }
    }

    iter_nodes(root, &|node| {
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

pub fn to_parsed_source(
    specifier: &ModuleSpecifier,
    media_type: MediaType,
    maybe_front_matter: Option<FrontMatter>,
    data: Vec<u8>,
) -> Result<ParsedSource, Error> {
    Ok(ParsedSource::new(
        specifier.to_string(),
        media_type,
        Some(String::from_utf8(data).unwrap()),
        maybe_front_matter,
    ))
}
