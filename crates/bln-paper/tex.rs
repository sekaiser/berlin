//! Small lexical helpers for declared resources, not a TeX interpreter.
pub use crate::aux::group;
use crate::{Error, unsupported};

pub struct Command<'a> {
    pub name: &'a str,
    pub start: usize,
    pub end: usize,
    pub depth: usize,
}

/// Comments and escaped punctuation are excluded. Argument depth lets callers
/// refuse resource declarations hidden inside macros or other arguments.
pub fn commands(source: &str) -> Vec<Command<'_>> {
    let bytes = source.as_bytes();
    let mut output = Vec::new();
    let mut i = 0;
    let mut depth = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'\\' => {
                let start = i;
                i += 1;
                let name = i;
                if i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    output.push(Command {
                        name: &source[name..i],
                        start,
                        end: i,
                        depth,
                    });
                } else if i < bytes.len() {
                    i += source[i..].chars().next().unwrap().len_utf8();
                }
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            _ => i += 1,
        }
    }
    output
}

pub fn optional(input: &str) -> Result<(Option<&str>, &str), Error> {
    let input = input.trim_start();
    let Some(rest) = input.strip_prefix('[') else {
        return Ok((None, input));
    };
    let mut braces = 0usize;
    let mut escaped = false;
    for (index, ch) in rest.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            ']' if braces == 0 => return Ok((Some(&rest[..index]), &rest[index + 1..])),
            _ => {}
        }
    }
    Err(unsupported("unclosed optional argument"))
}

pub fn simple_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}

/// Only literal command definitions with positional arguments. Structural/I/O
/// control sequences cannot be redefined or hidden inside a definition.
pub fn macro_definition(input: &str) -> Result<&str, Error> {
    let token = commands(input)
        .into_iter()
        .next()
        .ok_or_else(|| unsupported("missing macro definition"))?;
    let (name, rest) = group(&input[token.end..])?;
    let name = name
        .strip_prefix('\\')
        .ok_or_else(|| unsupported("macro name must be a control sequence"))?;
    if !simple_name(name) || reserved(name) {
        return Err(unsupported("macro name overrides document structure"));
    }
    let (arity, rest) = optional(rest)?;
    if let Some(arity) = arity
        && !matches!(
            arity,
            "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"
        )
    {
        return Err(unsupported("macro arity must be 0 through 9"));
    }
    let (_, rest) = optional(rest)?;
    let (body, tail) = group(rest)?;
    if commands(body).iter().any(|token| reserved(token.name)) {
        return Err(unsupported(
            "macro body contains structural or file-access commands",
        ));
    }
    Ok(&input[..input.len() - tail.len()])
}

fn reserved(name: &str) -> bool {
    matches!(
        name,
        "input"
            | "include"
            | "includegraphics"
            | "documentclass"
            | "usepackage"
            | "RequirePackage"
            | "LoadClass"
            | "begin"
            | "end"
            | "section"
            | "subsection"
            | "subsubsection"
            | "chapter"
            | "label"
            | "ref"
            | "eqref"
            | "cite"
            | "bibliography"
            | "bibliographystyle"
            | "caption"
            | "write"
            | "openout"
            | "openin"
            | "read"
            | "catcode"
            | "csname"
            | "endcsname"
            | "def"
            | "gdef"
            | "edef"
            | "xdef"
            | "newcommand"
            | "renewcommand"
            | "providecommand"
    )
}
