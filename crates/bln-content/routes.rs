//! Canonical website article routes, shared by legacy sites and publications.
use std::fmt;

use berlin_document::Document;
use slugify::slugify;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteError(String);
impl fmt::Display for RouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for RouteError {}

pub fn document_route(document: &Document) -> Result<String, RouteError> {
    let title = document
        .metadata
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .ok_or_else(|| RouteError(format!("Document '{}' has no title", document.id.0)))?;
    let slug = match document.metadata.slug.as_deref() {
        Some(slug) => validated_document_slug(document, slug)?.to_owned(),
        None => {
            let slug = slugify!(title);
            if slug.is_empty() {
                return Err(RouteError(format!(
                    "Cannot derive a document route from '{title}'"
                )));
            }
            slug
        }
    };
    Ok(format!("notes/{slug}.html"))
}

pub fn validated_document_slug<'a>(
    document: &Document,
    slug: &'a str,
) -> Result<&'a str, RouteError> {
    if !slug.split('-').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    }) {
        return Err(RouteError(format!(
            "Document '{}' has invalid slug '{slug}'; use lowercase ASCII letters, digits, and single hyphens",
            document.id.0
        )));
    }
    Ok(slug)
}
