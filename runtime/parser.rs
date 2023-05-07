use berlin_core::anyhow::Error;
use berlin_core::error::generic_error;
use berlin_core::{FrontMatter, MediaType, ModuleSpecifier, ParsedSource};
use comrak::nodes::{AstNode, NodeValue};
use comrak::plugins::syntect::SyntectAdapter;
use comrak::{format_html_with_plugins, parse_document, Arena, ComrakOptions, ComrakPlugins};
use lightningcss::bundler::{Bundler, FileProvider};
use lightningcss::css_modules::{Config, Pattern};
use lightningcss::stylesheet::{MinifyOptions, ParserOptions, PrinterOptions};
use pandoc::{InputFormat, InputKind, OutputFormat, OutputKind, PandocOption, PandocOutput};
use regex::{Captures, Regex};
use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

lazy_static::lazy_static! {
    static ref RELREF_RE: Regex = Regex::new(r#"\[(?P<label>.+)\]\(\{\{< relref "(?P<name>.*)" >\}\}\)"#).unwrap();
}

/// Stores parsed sources.
///
/// Note: This interface is racy and not thread safe, as it's assumed
/// it will only store the latest changes or that the source text
/// will never change.
pub trait ParsedSourceStore {
    /// Sets the parsed source, potentially returning the previous value.
    fn set_parsed_source(
        &self,
        specifier: ModuleSpecifier,
        parsed_source: ParsedSource,
    ) -> Option<ParsedSource>;

    fn get_parsed_source(&self, specifier: &ModuleSpecifier) -> Option<ParsedSource>;
}

/// Default store that works on a single thread.
#[derive(Default)]
pub struct DefaultParsedSourceStore {
    store: RefCell<HashMap<ModuleSpecifier, ParsedSource>>,
}

impl ParsedSourceStore for DefaultParsedSourceStore {
    fn set_parsed_source(
        &self,
        specifier: ModuleSpecifier,
        parsed_source: ParsedSource,
    ) -> Option<ParsedSource> {
        self.store.borrow_mut().insert(specifier, parsed_source)
    }

    fn get_parsed_source(&self, specifier: &ModuleSpecifier) -> Option<ParsedSource> {
        self.store.borrow().get(specifier).cloned()
    }
}

pub struct CapturingParser<'a> {
    _parser: Option<&'a dyn Parser>,
    store: &'a dyn ParsedSourceStore,
}

impl<'a> CapturingParser<'a> {
    pub fn new(parser: Option<&'a dyn Parser>, store: &'a dyn ParsedSourceStore) -> Self {
        Self {
            _parser: parser,
            store,
        }
    }

    fn get_from_store_if_matches(
        &self,
        specifier: &ModuleSpecifier,
        _media_type: MediaType,
    ) -> Option<ParsedSource> {
        let parsed_source = self.store.get_parsed_source(specifier)?;
        Some(parsed_source)
    }
}

impl<'a> Parser for CapturingParser<'a> {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        source: Arc<str>,
        media_type: MediaType,
    ) -> Result<ParsedSource, Error> {
        if let Some(parsed_source) = self.get_from_store_if_matches(specifier, media_type) {
            Ok(parsed_source)
        } else {
            let parsed_source = match media_type {
                MediaType::Org => {
                    let parser = DefaultOrgParser::default();
                    parser.parse(specifier, source, media_type)?
                }
                MediaType::Markdown => {
                    let parser = DefaultMarkdownParser::default();
                    parser.parse(specifier, source, media_type)?
                }
                MediaType::Tera => ParsedSource::new(
                    specifier.to_string(),
                    media_type,
                    Some(source.as_ref().to_string()),
                    None,
                ),
                MediaType::Csv => {
                    let parser = DefaultCsvParser::default();
                    parser.parse(specifier, source, media_type)?
                }
                MediaType::Css => {
                    let parser = DefaultCssParser::default();
                    parser.parse(specifier, source, media_type)?
                }
                _ => unreachable!("Type not supported."),
            };

            self.store
                .set_parsed_source(specifier.clone(), parsed_source.clone());

            Ok(parsed_source)
        }
    }
}

pub trait Parser {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        source: Arc<str>,
        media_type: MediaType,
    ) -> Result<ParsedSource, Error>;
}

#[derive(Default, Clone)]
pub struct DefaultOrgParser;

#[derive(Default, Clone)]
pub struct DefaultMarkdownParser;

#[derive(Default, Clone)]
pub struct DefaultCssParser;

#[derive(Default, Clone)]
pub struct DefaultCsvParser;

impl Parser for DefaultOrgParser {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        source: Arc<str>,
        media_type: MediaType,
    ) -> Result<ParsedSource, Error> {
        let mut pandoc = pandoc::new();
        pandoc.set_input(InputKind::Pipe(source.as_ref().to_owned()));
        pandoc.add_option(PandocOption::Standalone);
        let filter = std::env::current_dir()?
            .parent()
            .unwrap()
            .join("filters")
            .join("test.lua");
        pandoc.add_option(PandocOption::LuaFilter(filter));
        pandoc.set_input_format(InputFormat::Org, vec![]);
        pandoc.set_output_format(OutputFormat::Other("gfm".to_string()), vec![]);
        pandoc.set_output(OutputKind::Pipe);

        if let PandocOutput::ToBuffer(data) = pandoc.execute()? {
            let parser = DefaultMarkdownParser {};
            return parser.parse(specifier, Arc::from(data), media_type);
        } else {
            return Err(generic_error(format!(
                "Cannot convert file {} to {}",
                specifier,
                MediaType::Markdown
            )));
        }
    }
}

impl Parser for DefaultMarkdownParser {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        source: Arc<str>,
        _media_type: MediaType,
    ) -> Result<ParsedSource, Error> {
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

        //let syntect_adapter = SyntectAdapter::new("Solarized (dark)");
        let syntect_adapter = SyntectAdapter::new("base16-ocean.dark");
        let mut plugins = ComrakPlugins::default();
        plugins.render.codefence_syntax_highlighter = Some(&syntect_adapter);

        let preprocessed_source = RELREF_RE.replace_all(&source, |caps: &Captures| {
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
        format_html_with_plugins(root, &options, &mut html, &plugins).unwrap();
        let parsed_source = ParsedSource::new(
            specifier.to_string(),
            MediaType::Html,
            Some(String::from_utf8(html).unwrap()),
            maybe_front_matter,
        );
        Ok(parsed_source)
    }
}

impl Parser for DefaultCsvParser {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        source: Arc<str>,
        _media_type: MediaType,
    ) -> Result<ParsedSource, Error> {
        let parsed_source = ParsedSource::new(
            specifier.to_string(),
            MediaType::Csv,
            Some(source.to_string()),
            None,
        );
        Ok(parsed_source)
    }
}

impl Parser for DefaultCssParser {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        _source: Arc<str>,
        _media_type: MediaType,
    ) -> Result<ParsedSource, Error> {
        let fs = FileProvider::new();
        let parser_options = ParserOptions {
            css_modules: Some(Config {
                pattern: Pattern::parse("[local]")?,
                dashed_idents: true,
            }),
            ..ParserOptions::default()
        };
        let mut bundler = Bundler::new(&fs, None, parser_options);
        let mut stylesheet = bundler
            .bundle(&Path::new(&specifier.path().to_string()))
            .unwrap();
        stylesheet.minify(MinifyOptions::default())?;
        let res = stylesheet.to_css(PrinterOptions {
            minify: true,
            ..PrinterOptions::default()
        })?;

        let parsed_source =
            ParsedSource::new(specifier.to_string(), MediaType::Css, Some(res.code), None);
        Ok(parsed_source)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let specifier = ModuleSpecifier::parse("file:///a/test.org").expect("bad specifier");
        let source = r#"
        #+TITLE: Test

        This is the example org-mode file used for the Denver Emacs org-mode
        introduction talk. Everything in this talk should work without any custom
        settings or installation with a reasonably recent Emacs version.

        Let's start with a headline, this is kind of like Markdown's # character:

        * This is an example headline

          Text can be put into the headline. You can create another headline at the same
          level with another * character
        "#;
        let parsed_source = DefaultOrgParser::default()
            .parse(&specifier, source.into(), MediaType::Org)
            .unwrap();
        print!("{:?}", parsed_source.data());
    }
}
