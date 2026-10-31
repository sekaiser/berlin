//! Structural selection and dependency validation, before channel rendering.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use berlin_document::{Block, Inline};
use url::Url;

use crate::{
    Code, ComponentRegistry, Diagnostic, ExcerptRef, PublicationCatalog, Result, Stage, mount_url,
    validate_public_url,
};

/// One immutable selected fragment per unique source/selector, reused by all
/// occurrences. Renderers add occurrence identity without cloning source bodies.
#[derive(Clone, Debug)]
pub struct ResolvedExcerpt {
    pub reference: ExcerptRef,
    blocks: Arc<[Block]>,
    pub local_anchors: BTreeSet<String>,
    pub source_anchors: BTreeSet<String>,
    pub original_url: Url,
    pub source_mount: Option<Url>,
    pub dependency_names: Vec<String>,
    pub linked_sources: BTreeSet<String>,
    pub semantic_components: BTreeSet<crate::ComponentKey>,
    pub nodes_visited: usize,
}

impl ResolvedExcerpt {
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }
    pub fn resolve_url(&self, destination: &str) -> Result<Url> {
        resolve_source_url(destination, &self.original_url, self.source_mount.as_ref())
    }
}

pub(crate) fn resolve_excerpt(
    catalog: &PublicationCatalog,
    reference: &ExcerptRef,
    registry: &ComponentRegistry,
) -> Result<ResolvedExcerpt> {
    let run = || {
        let entry = catalog.resolve(&reference.source)?;
        let document = entry.document();
        crate::canonical::encoded_size(&document.blocks, catalog.limits().fragment_bytes)?;
        let original_url = entry.original_url.clone().ok_or_else(|| {
            Diagnostic::new(
                Code::UnsafeUrl,
                Stage::Resolution,
                "excerpt requires an original source URL",
            )
        })?;
        let source_mount = catalog
            .options()
            .source_site_url
            .as_ref()
            .map(mount_url)
            .transpose()?;
        let mut sections = BTreeMap::new();
        let mut definitions = BTreeMap::new();
        let mut nodes = 0;
        walk_blocks(&document.blocks, &mut |block| {
            nodes += 1;
            if nodes > catalog.limits().excerpt_nodes {
                return Err(limit());
            }
            // Account for inline trees before cloning the selected section.
            // A single paragraph must not hide an unbounded dependency graph.
            visit_block_inlines(block, &mut |_| {
                nodes += 1;
                if nodes > catalog.limits().excerpt_nodes {
                    return Err(limit());
                }
                Ok(())
            })?;
            if let Block::Section { id, .. } = block
                && sections.insert(id.0.as_str(), block).is_some()
            {
                return Err(Diagnostic::new(
                    Code::MissingExcerpt,
                    Stage::Resolution,
                    "ambiguous section ID",
                ));
            }
            if let Block::FootnoteDefinition { name, .. } = block {
                definitions.insert(name.as_str(), block);
            }
            Ok(())
        })?;
        let mut blocks = if let Some(section) = &reference.section {
            let selected = sections.get(section.as_str()).ok_or_else(|| {
                Diagnostic::new(
                    Code::MissingExcerpt,
                    Stage::Resolution,
                    format!(
                        "no stable section '{}'; provide an explicit source section ID",
                        section
                    ),
                )
            })?;
            vec![(**selected).clone()]
        } else {
            document.blocks.clone()
        };
        let mut needed = BTreeSet::new();
        let mut included = BTreeSet::new();
        walk_blocks(&blocks, &mut |b| {
            if let Block::FootnoteDefinition { name, .. } = b {
                included.insert(name.clone());
            }
            visit_block_inlines(b, &mut |i| {
                if let Inline::FootnoteReference { name } = i {
                    needed.insert(name.clone());
                }
                Ok(())
            })
        })?;
        fn add(
            name: &str,
            definitions: &BTreeMap<&str, &Block>,
            visiting: &mut BTreeSet<String>,
            complete: &mut BTreeSet<String>,
            extra: &mut Vec<Block>,
            nodes: &mut usize,
            maximum: usize,
        ) -> Result<()> {
            if visiting.len() >= 32 {
                return Err(limit());
            }
            if visiting.contains(name) {
                return Err(Diagnostic::new(
                    Code::UnresolvedReference,
                    Stage::Resolution,
                    format!("cyclic footnote dependency at '{name}'"),
                ));
            }
            if complete.contains(name) {
                return Ok(());
            }
            let definition = definitions.get(name).ok_or_else(|| {
                Diagnostic::new(
                    Code::UnresolvedReference,
                    Stage::Resolution,
                    format!("missing footnote definition '{name}'"),
                )
            })?;
            visiting.insert(name.into());
            let mut needed = BTreeSet::new();
            walk_blocks(std::slice::from_ref(*definition), &mut |b| {
                *nodes += 1;
                if *nodes > maximum {
                    return Err(limit());
                }
                visit_block_inlines(b, &mut |i| {
                    if let Inline::FootnoteReference { name } = i {
                        needed.insert(name.clone());
                    }
                    Ok(())
                })
            })?;
            for child in needed {
                add(
                    &child,
                    definitions,
                    visiting,
                    complete,
                    extra,
                    nodes,
                    maximum,
                )?;
            }
            visiting.remove(name);
            complete.insert(name.into());
            extra.push((**definition).clone());
            Ok(())
        }
        let mut extra = Vec::new();
        let mut complete = BTreeSet::new();
        for name in needed {
            add(
                &name,
                &definitions,
                &mut BTreeSet::new(),
                &mut complete,
                &mut extra,
                &mut nodes,
                catalog.limits().excerpt_nodes,
            )?;
        }
        let dependency_names: Vec<_> = complete.iter().cloned().collect();
        blocks.extend(extra.into_iter().filter(
            |b| !matches!(b, Block::FootnoteDefinition { name, .. } if included.contains(name)),
        ));
        let source_anchors = entry.anchors.clone();
        let mut local_anchors = BTreeSet::new();
        let mut linked_sources = BTreeSet::new();
        let mut semantic_components = BTreeSet::new();
        walk_blocks(&blocks, &mut |b| {
            collect_anchors(b, &mut local_anchors);
            if let Block::Html { .. } = b {
                return Err(unsupported(
                    "raw HTML blocks are not portable excerpt content",
                ));
            }
            if let Block::Component {
                name,
                properties,
                blocks,
                ..
            } = b
            {
                registry.validate_semantic_component(name, properties, blocks)?;
                if name != "bibliography-entry" {
                    semantic_components.insert(name.parse()?);
                }
            }
            if let Block::Figure(figure) = b {
                resolve_source_url(&figure.source, &original_url, source_mount.as_ref())?;
            }
            visit_block_inlines(b, &mut |i| {
                match i {
                    Inline::Html { .. } => {
                        return Err(unsupported(
                            "raw HTML inlines are not portable excerpt content",
                        ));
                    }
                    Inline::CrossReference { target, .. } => {
                        validate_anchor(&target.0, &source_anchors)?
                    }
                    Inline::Link { destination, .. } => {
                        if let Some(anchor) = destination.strip_prefix('#') {
                            validate_anchor(anchor, &source_anchors)?;
                        } else if !destination.starts_with("mailto:")
                            && !destination.starts_with("tel:")
                        {
                            resolve_source_url(destination, &original_url, source_mount.as_ref())?;
                        }
                    }
                    Inline::Image { source, .. } => {
                        resolve_source_url(source, &original_url, source_mount.as_ref())?;
                    }
                    Inline::DocumentLink(link) => {
                        linked_sources.insert(link.target.0.clone());
                        let target = catalog.entry(&link.target.0)?;
                        if target.original_url.is_none() {
                            return Err(Diagnostic::new(
                                Code::UnsafeUrl,
                                Stage::Resolution,
                                "linked original has no URL",
                            ));
                        }
                        if let Some(fragment) = &link.fragment
                            && !target.anchors.contains(fragment)
                        {
                            return Err(Diagnostic::new(
                                Code::UnresolvedReference,
                                Stage::Resolution,
                                format!(
                                    "unknown anchor '{fragment}' in linked source '{}'",
                                    link.target.0
                                ),
                            ));
                        }
                        local_anchors.insert(link.anchor.0.clone());
                    }
                    _ => {}
                }
                Ok(())
            })
        })?;
        Ok(ResolvedExcerpt {
            reference: reference.clone(),
            blocks: blocks.into(),
            local_anchors,
            source_anchors,
            original_url,
            source_mount,
            dependency_names,
            linked_sources,
            semantic_components,
            nodes_visited: nodes,
        })
    };
    run().map_err(|e: Diagnostic| e.source(&reference.source.id.0, reference.section.as_deref()))
}

fn validate_anchor(anchor: &str, known: &BTreeSet<String>) -> Result<()> {
    if known.contains(anchor) {
        Ok(())
    } else {
        Err(Diagnostic::new(
            Code::UnresolvedReference,
            Stage::Resolution,
            format!("no stable source anchor '{anchor}'"),
        ))
    }
}
fn unsupported(detail: &str) -> Diagnostic {
    Diagnostic::new(Code::UnsupportedChannelFeature, Stage::Capability, detail)
}
fn limit() -> Diagnostic {
    Diagnostic::new(
        Code::ResourceLimit,
        Stage::Resolution,
        "excerpt dependency traversal budget exceeded",
    )
}

pub fn resolve_source_url(destination: &str, original: &Url, mount: Option<&Url>) -> Result<Url> {
    if destination.trim().is_empty() || destination.starts_with('@') || destination.contains('\\') {
        return Err(Diagnostic::new(
            Code::UnsafeUrl,
            Stage::Resolution,
            "source asset/link has no resolvable public URL",
        ));
    }
    if let Ok(url) = Url::parse(destination) {
        validate_public_url(&url)?;
        return Ok(url);
    }
    let result = if destination.starts_with('/') && !destination.starts_with("//") {
        match mount {
            Some(base) => base.join(destination.trim_start_matches('/')),
            None => original.join(destination),
        }
    } else {
        original.join(destination)
    }
    .map_err(|e| Diagnostic::new(Code::UnsafeUrl, Stage::Resolution, e.to_string()))?;
    validate_public_url(&result)?;
    if !destination.starts_with("//")
        && let Some(mount) = mount
        && (result.origin() != mount.origin() || !result.path().starts_with(mount.path()))
    {
        return Err(Diagnostic::new(
            Code::UnsafeUrl,
            Stage::Resolution,
            "relative source URL escapes its configured mount",
        ));
    }
    Ok(result)
}

pub fn walk_blocks<'a>(
    blocks: &'a [Block],
    visitor: &mut impl FnMut(&'a Block) -> Result<()>,
) -> Result<()> {
    fn visit<'a>(
        blocks: &'a [Block],
        depth: usize,
        visitor: &mut impl FnMut(&'a Block) -> Result<()>,
    ) -> Result<()> {
        if depth > 32 {
            return Err(limit());
        }
        for block in blocks {
            visitor(block)?;
            match block {
                Block::Section { blocks, .. }
                | Block::Component { blocks, .. }
                | Block::FootnoteDefinition { blocks, .. }
                | Block::BlockQuote { blocks } => visit(blocks, depth + 1, visitor)?,
                Block::List(list) => {
                    for item in &list.items {
                        visit(&item.blocks, depth + 1, visitor)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    visit(blocks, 1, visitor)?;
    Ok(())
}

/// Visits each block's own inline content; block recursion belongs to walk_blocks.
pub fn visit_block_inlines(
    block: &Block,
    visitor: &mut impl FnMut(&Inline) -> Result<()>,
) -> Result<()> {
    fn visit(
        inlines: &[Inline],
        depth: usize,
        visitor: &mut impl FnMut(&Inline) -> Result<()>,
    ) -> Result<()> {
        if depth > 32 {
            return Err(limit());
        }
        for inline in inlines {
            visitor(inline)?;
            match inline {
                Inline::Emphasis { content }
                | Inline::Strong { content }
                | Inline::Strikethrough { content }
                | Inline::Link { content, .. }
                | Inline::CrossReference { content, .. } => visit(content, depth + 1, visitor)?,
                Inline::DocumentLink(link) => visit(&link.content, depth + 1, visitor)?,
                Inline::Image { description, .. } => visit(description, depth + 1, visitor)?,
                _ => {}
            }
        }
        Ok(())
    }
    match block {
        Block::Paragraph { content } | Block::Heading { content, .. } => {
            visit(content, 1, visitor)?
        }
        Block::Section { title, .. } => visit(title, 1, visitor)?,
        Block::Code(code) => {
            if let Some(caption) = &code.caption {
                visit(caption, 1, visitor)?;
            }
        }
        Block::Table(table) => {
            for row in &table.rows {
                for cell in &row.cells {
                    visit(cell, 1, visitor)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn collect_anchors(block: &Block, anchors: &mut BTreeSet<String>) {
    match block {
        Block::Section { id, .. } | Block::Component { id, .. } => {
            anchors.insert(id.0.clone());
        }
        Block::Equation(e) => {
            anchors.insert(e.id.0.clone());
        }
        Block::Heading { id: Some(id), .. } => {
            anchors.insert(id.clone());
        }
        Block::FootnoteDefinition { name, .. } => {
            anchors.insert(format!("fn-{name}"));
        }
        Block::Code(code) => {
            if let Some(id) = &code.references.id {
                anchors.insert(id.clone());
            }
            for offset in 0..code.value.lines().count() {
                if let Some(prefix) = &code.references.line_anchor_prefix {
                    anchors.insert(format!("{prefix}-{}", code.references.first_line + offset));
                }
            }
        }
        _ => {}
    }
}
