//! HTML adapter boundary. Receives effective immutable presentation inputs from
//! orchestration; it must not discover source roots or perform deployment.

use berlin_document_html::FragmentContext;
use berlin_publication::{OccurrenceId, ResolvedExcerpt, Result, visit_block_inlines, walk_blocks};
use std::collections::HashMap;

/// Projection from a validated excerpt, not a rewrite of generated HTML.
pub fn excerpt_context(
    excerpt: &ResolvedExcerpt,
    occurrence: &OccurrenceId,
    heading_offset: i8,
) -> Result<FragmentContext> {
    let mut destinations = HashMap::new();
    for anchor in &excerpt.source_anchors {
        let destination = if excerpt.local_anchors.contains(anchor) {
            format!("#{}--{anchor}", occurrence.0)
        } else {
            let mut url = excerpt.original_url.clone();
            url.set_fragment(Some(anchor));
            url.to_string()
        };
        destinations.insert(format!("#{anchor}"), destination.clone());
        destinations.insert(format!("{}#{anchor}", excerpt.original_url), destination);
    }
    walk_blocks(excerpt.blocks(), &mut |block| {
        if let berlin_document::Block::Figure(figure) = block {
            destinations.insert(
                figure.source.clone(),
                excerpt.resolve_url(&figure.source)?.to_string(),
            );
        }
        visit_block_inlines(block, &mut |inline| {
            let source = match inline {
                berlin_document::Inline::Image { source, .. } => Some(source),
                berlin_document::Inline::Link { destination, .. }
                    if !destination.starts_with('#')
                        && !destination.starts_with("mailto:")
                        && !destination.starts_with("tel:") =>
                {
                    Some(destination)
                }
                _ => None,
            };
            if let Some(source) = source {
                destinations.insert(source.clone(), excerpt.resolve_url(source)?.to_string());
            }
            Ok(())
        })
    })?;
    Ok(FragmentContext {
        namespace: occurrence.0.clone(),
        destinations,
        heading_offset,
    })
}
