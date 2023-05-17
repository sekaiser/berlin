use berlin_core::anyhow::Error;
use berlin_core::error::generic_error;
use std::ops::Range;

use pest::iterators::Pair;
use pest::Parser as PestParser;
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

fn replace_string_markers(input: &str) -> String {
    match input.chars().next().unwrap() {
        '"' => input.replace('"', ""),
        '\'' => input.replace('\'', ""),
        '`' => input.replace('`', ""),
        _ => unreachable!("How did you even get there"),
    }
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
                        _ => unreachable!("Got something unexpected in a kwarg: {:?}", p2),
                    }
                }

                args.insert(arg_name.unwrap(), arg_val.unwrap());
            }
            _ => unreachable!("Got something unexpected in a shortcode: {:?}", p),
        }
    }
    (name.unwrap(), tera::Value::Object(args))
}

pub fn parse_for_shortcodes(content: &str) -> Result<(String, Vec<Shortcode>), Error> {
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
                output.push_str(&name);
                let mut args_copy = args.clone();
                let src = args_copy
                    .get_mut("src")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
                    .replace("\"", "");

                let template = if let Some(caption) = args_copy.get_mut("caption") {
                    let caption = caption.as_str().unwrap().to_string().replace("\"", "");

                    format!(
                        r#"<figure><img style="max-width:100%;" src="/static{src}"><figcaption>{caption}</figcaption></figure>"#,
                    )
                } else {
                    format!(
                        r#"<img style="width:456px;margin-top:5px;margin-bottom:5px;" src="{src}">"#
                    )
                };

                shortcodes.push(Shortcode {
                    name,
                    args,
                    span: span.start()..span.end(),
                    body: Some(template.to_string()),
                });
            }
            _ => {}
        }
    }

    Ok((output, shortcodes))
}
