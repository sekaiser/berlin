//! BibTeX's compiled text is authoritative, not a second citation style engine.
use crate::{Error, aux::group, unsupported};

pub struct BibliographyEntry {
    pub key: String,
    /// Pandoc JSON obtained by parsing this compiled BibTeX item's LaTeX.
    pub pandoc_json: String,
}

/// Extracts entries from the supported `plain` .bbl envelope. `newblock` is
/// presentation spacing; all other item content is preserved for AST conversion.
pub fn bibtex_entries(source: &str) -> Result<Vec<(String, String)>, Error> {
    if source.len() > 1024 * 1024 {
        return Err(unsupported("bibliography exceeds 1 MiB"));
    }
    let rest = source
        .trim()
        .strip_prefix(r"\begin{thebibliography}")
        .ok_or_else(|| unsupported("unsupported compiled bibliography envelope"))?;
    let (_, rest) = group(rest)?;
    let rest = rest
        .trim()
        .strip_suffix(r"\end{thebibliography}")
        .ok_or_else(|| unsupported("unclosed compiled bibliography"))?;
    let mut entries = Vec::new();
    let mut rest = rest.trim();
    while !rest.is_empty() {
        let item = rest
            .strip_prefix(r"\bibitem")
            .ok_or_else(|| unsupported("unsupported bibliography preamble or item"))?;
        let (_, item) = crate::tex::optional(item)?;
        let (key, body) = group(item)?;
        let (body, tail) = match body.find(r"\bibitem") {
            Some(index) => (&body[..index], &body[index..]),
            None => (body, ""),
        };
        entries.push((key.into(), body.replace(r"\newblock", " ").trim().into()));
        rest = tail.trim();
    }
    Ok(entries)
}
