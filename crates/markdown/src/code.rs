//! The small subset of ox-hugo's fenced-code metadata that carries references.

use berlin_document::{Block, CodeBlock, CodeLineRange, CodeReferences, Inline};
use comrak::nodes::NodeCodeBlock;

pub(super) fn from_node(node: &NodeCodeBlock) -> Result<Block, String> {
    let (language, attributes) = node
        .info
        .split_once(char::is_whitespace)
        .unwrap_or((&node.info, ""));
    let mut references = CodeReferences::default();
    let mut highlights = Vec::new();
    let attributes = attributes.trim();
    if let Some(attributes) = attributes.strip_prefix('{') {
        let attributes = attributes
            .strip_suffix('}')
            .ok_or("Unclosed code attributes")?;
        for attribute in split_attributes(attributes)? {
            let Some((key, value)) = attribute.split_once('=') else {
                continue;
            };
            let raw_value = value.trim();
            let value = raw_value.trim_matches('"');
            match key.trim() {
                "lineanchors" => references.line_anchor_prefix = Some(value.into()),
                "id" => references.id = Some(value.into()),
                "hl_lines" => highlights = parse_highlights(raw_value)?,
                "linenostart" => {
                    references.first_line = value
                        .parse()
                        .map_err(|_| format!("Invalid code linenostart '{value}'"))?
                }
                _ => {} // Styling attributes remain renderer policy.
            }
        }
    }
    Ok(Block::Code(CodeBlock {
        language: (!language.is_empty()).then(|| language.into()),
        value: node.literal.clone(),
        references,
        caption: None,
        highlights,
    }))
}

fn parse_highlights(value: &str) -> Result<Vec<CodeLineRange>, String> {
    let entries = if let Some(array) = value.strip_prefix('[') {
        split_attributes(
            array
                .strip_suffix(']')
                .ok_or("Unclosed highlighted-line array")?,
        )?
    } else {
        vec![value]
    };
    entries
        .into_iter()
        .filter(|entry| !entry.trim().is_empty())
        .map(|entry| {
            let entry = entry.trim().trim_matches('"');
            let (start, end) = entry.split_once('-').unwrap_or((entry, entry));
            Ok(CodeLineRange {
                start: start
                    .parse()
                    .map_err(|_| format!("Invalid highlighted line '{entry}'"))?,
                end: end
                    .parse()
                    .map_err(|_| format!("Invalid highlighted line '{entry}'"))?,
            })
        })
        .collect()
}

fn split_attributes(attributes: &str) -> Result<Vec<&str>, String> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;
    let mut depth = 0usize;
    for (index, character) in attributes.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '[' if !quoted => depth += 1,
            ']' if !quoted => depth = depth.checked_sub(1).ok_or("Unbalanced code attributes")?,
            ',' if !quoted && depth == 0 => {
                result.push(&attributes[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if quoted || depth != 0 {
        return Err("Unclosed code attribute value".into());
    }
    result.push(&attributes[start..]);
    Ok(result)
}

/// ox-hugo puts a named source block's anchor immediately before its fence.
/// Consume only that exact standalone anchor, leaving arbitrary HTML untouched.
pub(super) fn associate_named_listings(blocks: Vec<Block>) -> Result<Vec<Block>, String> {
    let mut result = Vec::new();
    for mut block in blocks {
        if let Some(Block::Code(code)) = result.last_mut()
            && let Some(caption) = exported_caption(&block)
        {
            code.caption = Some(caption);
            continue;
        }
        if let Block::Code(code) = &mut block
            && let Some(id) = result.last().and_then(standalone_anchor)
        {
            if code
                .references
                .id
                .as_deref()
                .is_some_and(|existing| existing != id)
            {
                return Err("Conflicting named code listing anchors".into());
            }
            code.references.id = Some(id);
            result.pop();
        }
        result.push(block);
    }
    Ok(result)
}

fn exported_caption(block: &Block) -> Option<Vec<Inline>> {
    let Block::Html { value } = block else {
        return None;
    };
    let contents = value
        .trim()
        .strip_prefix("<div class=\"src-block-caption\">")?
        .strip_suffix("</div>")?
        .trim();
    // Only consume the exporter's exact wrapper, never a larger arbitrary HTML block.
    if contents.contains("<div") || contents.contains("</div>") {
        return None;
    }
    let contents = if contents.starts_with("<span class=\"src-block-number\">") {
        contents.split_once("</span>")?.1.trim()
    } else {
        contents
    };
    Some(crate::document::inline_fragment(contents))
}

fn standalone_anchor(block: &Block) -> Option<String> {
    let html = match block {
        Block::Html { value } => value.clone(),
        Block::Paragraph { content } => content
            .iter()
            .map(|inline| match inline {
                Inline::Html { value } => Some(value.as_str()),
                _ => None,
            })
            .collect::<Option<String>>()?,
        _ => return None,
    };
    let id = html
        .trim()
        .strip_prefix("<a id=\"")?
        .strip_suffix("\"></a>")?;
    // Entity-encoded or otherwise unusual HTML remains a raw HTML block.
    (!id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte)))
    .then(|| id.into())
}
