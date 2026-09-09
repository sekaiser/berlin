//! Explicit paper constructs checked before compilation, after resource expansion.
use crate::{Error, aux::group, unsupported};

pub fn validate_source(source: &str, bibliography_stem: Option<&str>) -> Result<(), Error> {
    let details = inspect_source(source)?;
    if details.bibliography.as_deref() != bibliography_stem {
        return Err(unsupported(
            "the supplied bibliography must match the source's bibliography argument exactly",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct SourceDetails {
    pub bibliography: Option<String>,
    pub bibliography_style: String,
    pub date: Option<String>,
    pub document_class: String,
    pub macros: String,
    pub tikz_libraries: Vec<String>,
}

pub fn inspect_source(source: &str) -> Result<SourceDetails, Error> {
    if source.len() > 1024 * 1024 {
        return Err(unsupported("LaTeX source exceeds 1 MiB"));
    }
    let source = without_comments(source);
    let (preamble, _) = source
        .split_once(r"\begin{document}")
        .ok_or_else(|| unsupported("expected one expanded LaTeX document"))?;
    let mut rest = preamble.trim();
    let mut class = false;
    let mut hyperref = false;
    let mut date = None;
    let mut document_class = String::new();
    let mut macros = String::new();
    let mut tikz_libraries = Vec::new();
    while !rest.is_empty() {
        let (command, tail) = command(rest)?;
        if matches!(command, "newcommand" | "providecommand" | "renewcommand") {
            let definition = crate::tex::macro_definition(rest)?;
            macros.push_str(definition);
            macros.push('\n');
            rest = rest[definition.len()..].trim();
            continue;
        }
        let (options, tail) = options(tail)?;
        let (value, tail) = group(tail)?;
        match command {
            "documentclass" if !class && crate::tex::simple_name(value) => {
                if !options.split(',').all(|v| {
                    matches!(
                        v.trim(),
                        "" | "10pt"
                            | "11pt"
                            | "12pt"
                            | "a4paper"
                            | "letterpaper"
                            | "oneside"
                            | "twoside"
                            | "openany"
                            | "openright"
                    )
                }) {
                    return Err(unsupported("unsupported article class options"));
                }
                class = true;
                document_class = value.into();
            }
            "usepackage" => {
                for package in value.split(',').map(str::trim) {
                    match package {
                        "amsmath" | "amssymb" | "amsfonts" | "graphicx" | "tikz"
                            if options.is_empty() => {}
                        "hyperref"
                            if options
                                .split(',')
                                .all(|v| matches!(v.trim(), "" | "hidelinks" | "pdfusetitle")) =>
                        {
                            hyperref = true
                        }
                        _ => {
                            return Err(unsupported(format!(
                                "unsupported package or options: '{package}'"
                            )));
                        }
                    }
                }
            }
            "date" if options.is_empty() => {
                if date.replace(value.to_owned()).is_some() {
                    return Err(unsupported("duplicate date command"));
                }
            }
            "title" | "author" if options.is_empty() => {}
            "usetikzlibrary" if options.is_empty() => {
                for library in value.split(',').map(str::trim) {
                    if library.is_empty()
                        || !library
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
                    {
                        return Err(unsupported("TikZ library must be a literal name"));
                    }
                    tikz_libraries.push(library.into());
                }
            }
            _ => {
                return Err(unsupported(format!(
                    "unsupported preamble command '\\{command}'"
                )));
            }
        }
        rest = tail.trim();
    }
    if !class || !hyperref {
        return Err(unsupported(
            "the expanded paper requires a document class and hyperref",
        ));
    }
    let mut rest = source.as_str();
    let mut bibliography = None;
    let mut bibliography_style = "plain".to_owned();
    while let Some(index) = rest.find('\\') {
        let (name, tail) = command(&rest[index..])?;
        rest = tail;
        match name {
            "input" | "include" | "def" | "gdef" | "newenvironment" | "newtheorem"
            | "setcounter" | "addtocounter" | "numberwithin" => {
                return Err(unsupported(format!(
                    "'\\{name}' must be expanded by the source loader or is outside the paper profile"
                )));
            }
            "begin" => {
                let (environment, tail) = group(rest)?;
                if !matches!(
                    environment,
                    "document"
                        | "abstract"
                        | "equation"
                        | "equation*"
                        | "align*"
                        | "aligned"
                        | "bmatrix"
                        | "pmatrix"
                        | "matrix"
                        | "cases"
                        | "table"
                        | "tabular"
                        | "itemize"
                        | "enumerate"
                        | "quote"
                        | "figure"
                        | "tikzpicture"
                        | "scope"
                ) {
                    return Err(unsupported(format!(
                        "unsupported environment '{environment}'"
                    )));
                }
                if environment == "equation" {
                    let (body, _) = tail
                        .split_once(r"\end{equation}")
                        .ok_or_else(|| unsupported("unclosed equation"))?;
                    if !body.contains(r"\label{") {
                        return Err(unsupported("every numbered equation requires a label"));
                    }
                }
                rest = tail;
            }
            "bibliographystyle" => {
                let (style, tail) = group(rest)?;
                if !matches!(style, "plain" | "unsrt" | "abbrv" | "alpha") {
                    return Err(unsupported(
                        "supported BibTeX styles are plain, unsrt, abbrv and alpha",
                    ));
                }
                bibliography_style = style.into();
                rest = tail;
            }
            "bibliography" => {
                let (name, tail) = group(rest)?;
                if bibliography.replace(name).is_some() {
                    return Err(unsupported("only one bibliography is supported"));
                }
                rest = tail;
            }
            _ => {}
        }
    }
    Ok(SourceDetails {
        bibliography: bibliography.map(str::to_owned),
        bibliography_style,
        date,
        document_class,
        macros,
        tikz_libraries,
    })
}

fn command(input: &str) -> Result<(&str, &str), Error> {
    let tail = input
        .strip_prefix('\\')
        .ok_or_else(|| unsupported("unexpected preamble text"))?;
    let end = tail
        .find(|ch: char| !ch.is_ascii_alphabetic())
        .unwrap_or(tail.len());
    let end = if end == 0 {
        tail.chars().next().map(char::len_utf8).unwrap_or(0)
    } else {
        end
    };
    Ok((&tail[..end], &tail[end..]))
}

fn options(input: &str) -> Result<(&str, &str), Error> {
    let input = input.trim_start();
    match input.strip_prefix('[') {
        None => Ok(("", input)),
        Some(tail) => tail
            .split_once(']')
            .ok_or_else(|| unsupported("unclosed options")),
    }
}

fn without_comments(source: &str) -> String {
    let mut result = String::new();
    for line in source.lines() {
        let mut escaped = false;
        for ch in line.chars() {
            if ch == '%' && !escaped {
                break;
            }
            result.push(ch);
            escaped = ch == '\\' && !escaped;
        }
        result.push('\n');
    }
    result
}
