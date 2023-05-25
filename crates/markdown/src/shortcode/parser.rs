use berlin_core::{FrontMatter, ModuleSpecifier};
use errors::anyhow::Error;
use errors::error::generic_error;
use slugify::slugify;
use std::ops::Range;
use std::path::{Path, PathBuf};

use pest::iterators::Pair;
use pest::{Parser as PestParser, Span};
use pest_derive::Parser as PestParser;

#[derive(PestParser)]
#[grammar = "content.pest"]
pub struct ContentParser;

#[derive(PartialEq, Debug, Eq)]
pub struct Shortcode {
    pub(crate) name: String,
    pub(crate) args: tera::Value,
    pub(crate) span: Range<usize>,
    pub(crate) body: Option<String>,
}

fn parse_kwarg_value(pair: Pair<Rule>) -> tera::Value {
    let mut val = None;
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::boolean => match p.as_str() {
                "true" => val = Some(tera::Value::Bool(true)),
                "false" => val = Some(tera::Value::Bool(false)),
                _ => unreachable!(),
            },
            Rule::string => val = Some(tera::Value::String(replace_string_markers(p.as_str()))),
            Rule::float => {
                val = Some(tera::to_value(p.as_str().parse::<f64>().unwrap()).unwrap());
            }
            Rule::int => {
                val = Some(tera::to_value(p.as_str().parse::<i64>().unwrap()).unwrap());
            }
            Rule::array => {
                let mut vals = vec![];
                for p2 in p.into_inner() {
                    match p2.as_rule() {
                        Rule::literal => vals.push(parse_kwarg_value(p2)),
                        _ => unreachable!("Got something other than literal in an array: {:?}", p2),
                    }
                }
                val = Some(tera::Value::Array(vals));
            }
            _ => unreachable!("Unknown literal: {:?}", p),
        };
    }

    val.unwrap()
}

/// Returns (shortcode_name, kwargs)
fn parse_shortcode_call(pair: Pair<Rule>) -> (String, tera::Value) {
    let mut name = None;
    let mut args = tera::Map::new();

    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::ident => {
                name = Some(p.as_span().as_str().to_string());
            }
            Rule::kwarg => {
                let mut arg_name = None;
                let mut arg_val = None;
                for p2 in p.into_inner() {
                    match p2.as_rule() {
                        Rule::ident => {
                            arg_name = Some(p2.as_span().as_str().to_string());
                        }
                        Rule::literal => {
                            arg_val = Some(parse_kwarg_value(p2));
                        }
                        Rule::string => {
                            arg_name = name.clone();
                            arg_val =
                                Some(tera::Value::String(replace_string_markers(p2.as_str())));
                        }

                        _ => unreachable!("Got something unexpected in a kwarg: {:?}", p2),
                    }
                }

                if let Some((Some(name), Some(value))) = Some((arg_name, arg_val)) {
                    args.insert(name, value);
                }
            }
            _ => unreachable!("Got something unexpected in a shortcode: {:?}", p),
        }
    }
    (name.unwrap(), tera::Value::Object(args))
}

pub fn parse_for_shortcodes(
    specifier: &ModuleSpecifier,
    content: &str,
) -> Result<(String, Vec<Shortcode>), Error> {
    let mut shortcodes: Vec<Shortcode> = Vec::new();
    let mut output = String::with_capacity(content.len());
    let mut pairs = match ContentParser::parse(Rule::page, content) {
        Ok(p) => p,
        Err(_e) => {
            return Err(generic_error("parsing failed"));
        }
    };

    for p in pairs.next().unwrap().into_inner() {
        match p.as_rule() {
            Rule::inline_shortcode | Rule::ignored_inline_shortcode => {
                let span = p.as_span();
                let (name, args) = parse_shortcode_call(p);

                match name.as_str() {
                    "figure" => {
                        output.push_str(&name);
                        handle_figure(name, args, &span, &mut shortcodes)
                    }
                    "relref" => {
                        output.push_str(&name);
                        handle_relref(name, args, &span, specifier, &mut shortcodes);
                    }
                    _ => println!("Unknown identifier {name}"),
                }
            }
            _ => {}
        }
    }

    Ok((output, shortcodes))
}

fn handle_figure(name: String, value: tera::Value, span: &Span, shortcodes: &mut Vec<Shortcode>) {
    if let Some(src) = get_string("src", &value) {
        let template = if let Some(caption) = get_string("caption", &value) {
            format!(
                r#"<figure><img style="max-width:100%;" src="/static{src}"><figcaption>{caption}</figcaption></figure>"#,
            )
        } else {
            format!(r#"<img style="width:456px;margin-top:5px;margin-bottom:5px;" src="{src}">"#)
        };

        shortcodes.push(Shortcode {
            name,
            args: value,
            span: span.start()..span.end(),
            body: Some(template.to_string()),
        });
    }
}

fn handle_relref(
    name: String,
    value: tera::Value,
    span: &Span,
    specifier: &ModuleSpecifier,
    shortcodes: &mut Vec<Shortcode>,
) {
    let maybe_path = specifier.to_file_path().ok();
    let maybe_relref = get_string("relref", &value);

    if let Some((Some(file_name), Some(path))) = Some((maybe_relref, maybe_path)) {
        if let Some(title) = join(path, file_name).and_then(read_title_from_content_of_file) {
            let template = format!("/notes/{}.html", slugify!(&title));
            shortcodes.push(Shortcode {
                name,
                args: value,
                span: span.start()..span.end(),
                body: Some(template.to_string()),
            });
        }
    };
}

fn replace_string_markers(input: &str) -> String {
    match input.chars().next().unwrap() {
        '"' => input.replace('"', ""),
        '\'' => input.replace('\'', ""),
        '`' => input.replace('`', ""),
        _ => unreachable!("How did you even get there"),
    }
}

fn get_string<'a>(name: &str, value: &'a tera::Value) -> Option<&'a str> {
    value.get(name).and_then(|v| v.as_str())
}

fn join<P: AsRef<Path>>(path: PathBuf, file_name: P) -> Option<PathBuf> {
    path.parent().map(|p| p.join(file_name))
}

fn read_title_from_content_of_file(path: PathBuf) -> Option<String> {
    ModuleSpecifier::from_file_path(path)
        .ok()
        .and_then(|p| std::fs::read_to_string(p.path()).ok())
        .and_then(|s| extract_yaml(&s).ok())
        .and_then(|s| serde_yaml::from_str::<FrontMatter>(&s).ok())
        .and_then(|fm| fm.title)
}

fn extract_yaml(markdown: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut front_matter = String::default();
    let mut sentinel = false;
    let lines = markdown.lines();

    for line in lines {
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

    Ok(front_matter)
}
