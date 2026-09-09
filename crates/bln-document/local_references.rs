//! Checked within-document references. Ordinary authored URLs remain ordinary links.

use std::collections::HashSet;

use crate::{Document, DocumentValidationError, Inline, plain_text};

pub(super) fn validate(
    document: &Document,
    anchors: &HashSet<String>,
) -> Result<(), DocumentValidationError> {
    let mut result = Ok(());
    super::references::visit_passages(&document.blocks, None, &mut |content, _| {
        if result.is_ok() {
            result = validate_inlines(content, anchors, false);
        }
    });
    result
}

fn validate_inlines(
    inlines: &[Inline],
    anchors: &HashSet<String>,
    inside_link: bool,
) -> Result<(), DocumentValidationError> {
    for inline in inlines {
        match inline {
            Inline::CrossReference { target, content } => {
                if inside_link {
                    return Err(invalid(
                        "a cross-reference cannot be nested in another link",
                    ));
                }
                // Restrict fragments to directly representable HTML anchors. No
                // URL decoding or URL construction belongs in the domain model.
                if target.0.is_empty()
                    || !target
                        .0
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
                {
                    return Err(invalid(format!("invalid target '{}'", target.0)));
                }
                if !anchors.contains(&target.0) {
                    return Err(invalid(format!("unknown target '{}'", target.0)));
                }
                if plain_text(content).trim().is_empty() {
                    return Err(invalid(format!("reference to '{}' has no label", target.0)));
                }
                validate_inlines(content, anchors, true)?;
            }
            Inline::Link { content, .. } => {
                if inside_link {
                    return Err(invalid("links cannot be nested"));
                }
                validate_inlines(content, anchors, true)?;
            }
            Inline::DocumentLink(link) => {
                if inside_link {
                    return Err(invalid("links cannot be nested"));
                }
                validate_inlines(&link.content, anchors, true)?;
            }
            Inline::Emphasis { content }
            | Inline::Strong { content }
            | Inline::Strikethrough { content } => {
                validate_inlines(content, anchors, inside_link)?;
            }
            Inline::Image { description, .. } => {
                validate_inlines(description, anchors, inside_link)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> DocumentValidationError {
    DocumentValidationError::InvalidLocalReference(message.into())
}
