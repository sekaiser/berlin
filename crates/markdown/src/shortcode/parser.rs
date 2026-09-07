use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use anyhow::Error;
use anyhow::anyhow;
use anyhow::bail;
use pest::Parser as PestParser;
use pest::Span;
use pest::iterators::Pair;
use pest_derive::Parser as PestParser;
use slugify::slugify;
use url::Url as ModuleSpecifier;

use crate::front_matter::FrontMatter;

#[derive(PestParser)]
#[grammar = "content.pest"]
struct ContentParser;

#[derive(PartialEq, Debug, Eq)]
pub(crate) struct Shortcode {
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

pub(super) fn parse(specifier: &ModuleSpecifier, content: &str) -> Result<Vec<Shortcode>, Error> {
    let mut shortcodes: Vec<Shortcode> = Vec::new();
    let mut pairs = ContentParser::parse(Rule::page, content)
        .map_err(|error| anyhow!("shortcode parsing failed in {specifier}: {error}"))?;

    for p in pairs.next().unwrap().into_inner() {
        match p.as_rule() {
            Rule::inline_shortcode | Rule::ignored_inline_shortcode => {
                let span = p.as_span();
                let (name, args) = parse_shortcode_call(p);

                match name.as_str() {
                    "figure" => {
                        handle_figure(name, args, &span, &mut shortcodes)?;
                    }
                    "relref" => {
                        handle_relref(name, args, &span, specifier, &mut shortcodes)?;
                    }
                    _ => bail!("unsupported shortcode '{name}' in {specifier}"),
                }
            }
            _ => {}
        }
    }

    Ok(shortcodes)
}

fn handle_figure(
    name: String,
    value: tera::Value,
    span: &Span,
    shortcodes: &mut Vec<Shortcode>,
) -> Result<(), Error> {
    let src = get_string("src", &value).context("figure shortcode requires a 'src' argument")?;
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
        body: Some(template),
    });
    Ok(())
}

fn handle_relref(
    name: String,
    value: tera::Value,
    span: &Span,
    specifier: &ModuleSpecifier,
    shortcodes: &mut Vec<Shortcode>,
) -> Result<(), Error> {
    let path = specifier
        .to_file_path()
        .map_err(|_| anyhow!("relref source is not a file URL: {specifier}"))?;
    let file_name =
        get_string("relref", &value).context("relref shortcode requires a target path")?;
    let target = join(path, file_name).context("relref source has no parent directory")?;
    let title = read_title_from_content_of_file(target.clone())
        .with_context(|| format!("unable to resolve relref target {}", target.display()))?;
    let template = format!("/notes/{}.html", slugify!(&title));
    shortcodes.push(Shortcode {
        name,
        args: value,
        span: span.start()..span.end(),
        body: Some(template),
    });
    Ok(())
}

fn replace_string_markers(input: &str) -> String {
    let marker = input.chars().next().unwrap();
    let value = &input[marker.len_utf8()..input.len() - marker.len_utf8()];
    match marker {
        '"' => value.replace("\\\"", "\"").replace("\\\\", "\\"),
        '\'' => value.replace("\\'", "'").replace("\\\\", "\\"),
        '`' => value.to_owned(),
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
        .and_then(|s| serde_saphyr::from_str::<FrontMatter>(&s).ok())
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
