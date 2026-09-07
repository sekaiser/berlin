use berlin_document::Block;

fn parse(text: &str) -> Result<berlin_document::Document, markdown::Error> {
    markdown::Parser::new().parse(&markdown::Source::in_memory(text))
}

#[test]
fn preserves_ox_hugo_named_listing_and_line_coordinates() {
    let document = parse(include_str!("fixtures/code-references.md")).unwrap();
    let code = document
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Code(code) => Some(code),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        code.references.id.as_deref(),
        Some("code-snippet--country-extraction")
    );
    assert_eq!(code.references.first_line, 20);
    assert_eq!(
        code.references.line_anchor_prefix.as_deref(),
        Some("org-coderef--c81016")
    );
    assert!(code.value.contains("&value[\"countryObject\"][\"name\"]"));
    assert!(!document.blocks.iter().any(|block| matches!(block, Block::Paragraph { content } if content.iter().any(|inline| matches!(inline, berlin_document::Inline::Html { value } if value.contains("code-snippet--country-extraction"))))));
}

#[test]
fn retains_reference_metadata_and_highlights_in_nested_blocks() {
    let document = parse("> ```rust { hl_lines=[\"1\",\"2-3\"], linenostart=5, lineanchors=example }\n> a\n> b\n> c\n> ```\n\n[Explain](#example-6)\n").unwrap();
    let Block::BlockQuote { blocks } = &document.blocks[0] else {
        panic!("expected quote")
    };
    let Block::Code(code) = &blocks[0] else {
        panic!("expected code")
    };
    assert_eq!(code.references.first_line, 5);
    assert_eq!(
        code.highlights,
        vec![
            berlin_document::CodeLineRange { start: 1, end: 1 },
            berlin_document::CodeLineRange { start: 2, end: 3 }
        ]
    );
    assert_eq!(
        code.references.line_anchor_prefix.as_deref(),
        Some("example")
    );
}

#[test]
fn associates_exported_caption_with_its_listing() {
    let document = parse("```rust\nx\n```\n<div class=\"src-block-caption\">\n  <span class=\"src-block-number\"><a href=\"#example\">Code Snippet 1</a>:</span>\n  Read <em>carefully</em> &amp; compare.\n</div>\n").unwrap();
    assert_eq!(document.blocks.len(), 1);
    let Block::Code(code) = &document.blocks[0] else {
        panic!("expected code")
    };
    let caption = code.caption.as_ref().unwrap();
    assert!(caption.iter().any(|inline| matches!(inline, berlin_document::Inline::Text { value } if value.contains("carefully"))));
    assert!(!format!("{caption:?}").contains("Code Snippet"));
}

#[test]
fn rejects_highlight_ranges_outside_the_source_regardless_of_displayed_numbers() {
    for value in [
        "[\"0\"]",
        "[\"2\"]",
        "[\"2-1\"]",
        "[\"no\"]",
        "[\"999999999999999999999999999\"]",
    ] {
        assert!(
            parse(&format!(
                "```rust {{ linenostart=20, hl_lines={value} }}\nx\n```"
            ))
            .is_err()
        );
    }
    assert!(parse("```rust { linenostart=20, hl_lines=[\"1\"] }\nx\n```").is_ok());
}

#[test]
fn does_not_consume_a_caption_wrapper_with_unrelated_nested_html() {
    let document =
        parse("```rust\nx\n```\n<div class=\"src-block-caption\"><div>Unrelated</div></div>\n")
            .unwrap();
    assert_eq!(document.blocks.len(), 2);
}

#[test]
fn rejects_invalid_or_ambiguous_references() {
    for text in [
        "```rust { linenostart=no }\nx\n```",
        "```rust { linenostart=0 }\nx\n```",
        "```rust { lineanchors=\"bad id\" }\nx\n```",
        "```rust { lineanchors=ref }\nx\n```\n\n[Missing](#ref-2)",
        "[Missing](#org-coderef--gone-1)",
        "```rust { id=duplicate }\nx\n```\n\n```rust { id=duplicate }\ny\n```",
        "```rust { lineanchors=ref }\nx\n```\n\n```rust { lineanchors=ref }\ny\n```",
        "```rust { lineanchors=\"unterminated }\nx\n```",
    ] {
        assert!(parse(text).is_err(), "accepted {text}");
    }
}

#[test]
fn unrelated_html_is_not_consumed_as_listing_metadata() {
    let document = parse("<a id=\"keep\">Visible text</a>\n\n```rust\nx\n```\n").unwrap();
    let Block::Code(code) = &document.blocks[1] else {
        panic!("expected code")
    };
    assert!(code.references.id.is_none());
}
