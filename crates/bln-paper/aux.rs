//! Reads values from TeX's auxiliary file. Never evaluates its commands.
use crate::{Error, unsupported};
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct Labels {
    pub(crate) references: BTreeMap<String, Label>,
    pub(crate) citations: BTreeMap<String, String>,
}

#[derive(Debug)]
pub(crate) struct Label {
    pub number: String,
    pub kind: String,
}

impl Labels {
    pub fn parse(aux: &str) -> Result<Self, Error> {
        if aux.len() > 1024 * 1024 {
            return Err(unsupported("auxiliary file exceeds 1 MiB"));
        }
        let mut labels = Self::default();
        for line in aux.lines().map(str::trim) {
            if let Some(rest) = line.strip_prefix(r"\newlabel") {
                let (id, rest) = group(rest)?;
                let (fields, rest) = group(rest)?;
                if !rest.trim().is_empty() {
                    return Err(unsupported("trailing label data"));
                }
                let (number, fields) = group(fields)?;
                let (_, fields) = group(fields)?; // printed page
                let (_, fields) = group(fields)?; // title
                let (destination, _) = group(fields)?; // hyperref target
                validate_number(number)?;
                let kind = destination.split('.').next().unwrap_or_default();
                if !matches!(
                    kind,
                    "chapter"
                        | "section"
                        | "subsection"
                        | "subsubsection"
                        | "equation"
                        | "table"
                        | "figure"
                ) {
                    return Err(unsupported(format!(
                        "label '{id}' has unsupported target kind '{kind}'"
                    )));
                }
                if labels
                    .references
                    .insert(
                        id.into(),
                        Label {
                            number: number.into(),
                            kind: kind.into(),
                        },
                    )
                    .is_some()
                {
                    return Err(unsupported(format!("duplicate TeX label '{id}'")));
                }
            } else if let Some(rest) = line.strip_prefix(r"\bibcite") {
                let (id, rest) = group(rest)?;
                let (number, rest) = group(rest)?;
                validate_number(number)?;
                if !rest.trim().is_empty()
                    || labels.citations.insert(id.into(), number.into()).is_some()
                {
                    return Err(unsupported(format!("invalid or duplicate citation '{id}'")));
                }
            } else if line.starts_with(r"\@input") {
                return Err(unsupported("multi-file auxiliary data is not supported"));
            }
        }
        Ok(labels)
    }

    pub(crate) fn reference(&self, id: &str) -> Result<&Label, Error> {
        self.references.get(id).ok_or_else(|| {
            unsupported(format!(
                "no compiled label for '{id}'; label every section and numbered equation"
            ))
        })
    }

    pub(crate) fn citation(&self, id: &str) -> Result<&str, Error> {
        self.citations
            .get(id)
            .map(String::as_str)
            .ok_or_else(|| unsupported(format!("no compiled citation for '{id}'")))
    }
}

fn validate_number(value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.len() > 32
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
    {
        return Err(unsupported(format!(
            "unsupported printed reference number '{value}'"
        )));
    }
    Ok(())
}

/// A balanced, borrowed TeX group. Escaped braces do not affect its depth.
pub fn group(input: &str) -> Result<(&str, &str), Error> {
    let input = input.trim_start();
    if !input.starts_with('{') {
        return Err(unsupported("expected a braced TeX argument"));
    }
    let mut depth = 0usize;
    let mut escaped = false;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((&input[1..index], &input[index + 1..]));
                }
            }
            _ => {}
        }
    }
    Err(unsupported("unclosed TeX argument"))
}
