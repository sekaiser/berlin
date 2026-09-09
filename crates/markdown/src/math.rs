//! Recognize Comrak math nodes before Markdown syntax can consume their TeX.
use crate::Error;
use berlin_math::DisplayMode;
use comrak::nodes::{AstNode, NodeHtmlBlock, NodeValue};

pub(super) fn expression(value: &NodeValue) -> Option<(&str, DisplayMode)> {
    match value {
        NodeValue::Math(math) => Some((
            &math.literal,
            if math.display_math {
                DisplayMode::Block
            } else {
                DisplayMode::Inline
            },
        )),
        NodeValue::CodeBlock(code) if code.info.trim() == "math" => {
            Some((&code.literal, DisplayMode::Block))
        }
        _ => None,
    }
}

pub(super) fn validate<'a>(root: &'a AstNode<'a>, uri: &str) -> Result<(), Error> {
    for node in root.descendants() {
        render_equation(node, uri)?;
    }
    Ok(())
}

fn render_equation<'a>(
    node: &'a AstNode<'a>,
    uri: &str,
) -> Result<Option<(DisplayMode, String)>, Error> {
    let data = node.data.borrow();
    let Some((tex, mode)) = expression(&data.value) else {
        return Ok(None);
    };
    let error = |message| Error::InvalidMath {
        source_uri: uri.into(),
        line: data.sourcepos.start.line,
        message,
    };
    if matches!(data.value, NodeValue::Math(_))
        && mode == DisplayMode::Block
        && node
            .parent()
            .is_some_and(|parent| !matches!(parent.data.borrow().value, NodeValue::Paragraph))
    {
        return Err(error(
            "display math must be in a paragraph, not inside emphasis, headings or link labels"
                .into(),
        ));
    }
    let (label, cleaned) = crate::references::expression(tex).map_err(error)?;
    if label.is_some() && mode == DisplayMode::Inline {
        return Err(error("equation labels require display math".into()));
    }
    berlin_math::render(&cleaned, mode)
        .map(|html| Some((mode, html.into_string())))
        .map_err(|source| error(source.to_string()))
}

pub(super) fn render<'a>(
    root: &'a AstNode<'a>,
    uri: &str,
    references: &crate::references::References,
) -> Result<bool, Error> {
    let mut found = false;
    for node in root.descendants() {
        let Some((mode, mut html)) = render_equation(node, uri)? else {
            continue;
        };
        if let Some((tex, _)) = expression(&node.data.borrow().value)
            && let Some(id) = crate::references::expression(tex)
                .map_err(|message| crate::references::invalid(uri, message))?
                .0
        {
            let number = references
                .number(&id)
                .map_err(|message| crate::references::invalid(uri, message))?;
            html = format!(
                "<span id=\"{}\" class=\"berlin-equation\">{} <a href=\"#{}\" aria-label=\"Equation {}\">({})</a></span>",
                crate::references::escape(&id),
                html,
                crate::references::escape(&id),
                number,
                number
            );
        }
        let mut data = node.data.borrow_mut();
        // MathML is valid phrasing content even in display mode. Fences are block nodes.
        data.value = if matches!(data.value, NodeValue::CodeBlock(_)) {
            NodeValue::HtmlBlock(NodeHtmlBlock {
                block_type: 1,
                literal: format!(
                    "<div class=\"berlin-math-display\" tabindex=\"0\" role=\"region\" aria-label=\"Equation\">{html}</div>\n"
                ),
            })
        } else {
            NodeValue::HtmlInline(if mode == DisplayMode::Block {
                format!(
                    "<span class=\"berlin-math-display\" style=\"display:block\" tabindex=\"0\" role=\"region\" aria-label=\"Equation\">{html}</span>"
                )
            } else {
                html
            })
        };
        found = true;
    }
    Ok(found)
}
