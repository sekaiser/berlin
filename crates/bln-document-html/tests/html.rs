use std::collections::BTreeMap;

use berlin_document::Block;
use berlin_document::CodeBlock;
use berlin_document::ComponentId;
use berlin_document::Inline;
use berlin_document::PropertyValue;
use berlin_document_html::Renderer;

#[test]
fn renders_a_semantic_document_as_html() {
    let source = r#"---
title: Test
---

Intro with **meaning** and [a link](https://example.com).

## Stable heading {#stable-heading}

- first
- second

| Name | Value |
|------|------:|
| one  |     1 |

{{< figure src="/pics/example.png" caption="Example figure" >}}

```rust
fn main() {}
```
"#;
    let source = markdown::Source::new(source, "file:///content/test.md").unwrap();
    let document = markdown::Parser::new().parse(&source).unwrap();

    let output = Renderer::default().render(&document);
    let output = output.as_str();

    assert!(output.contains("<p>Intro with <strong>meaning</strong>"));
    assert!(output.contains("href=\"https://example.com\""));
    assert!(output.contains("id=\"stable-heading\""));
    assert!(output.contains("<ul>\n<li>first</li>"));
    assert!(output.contains("<table>"));
    assert!(output.contains(
        "<figure><img style=\"max-width:100%;\" \
         src=\"/pics/example.png\"><figcaption>Example figure</figcaption></figure>"
    ));
    assert!(output.contains("<code class=\"language-rust\">"));
    assert!(document.blocks.iter().any(|block| matches!(
        block,
        Block::Section { blocks, .. }
            if blocks.iter().any(|block| matches!(block, Block::Code(_)))
    )));
}

#[test]
fn renders_extensible_sections_and_components() {
    let source = markdown::Source::new("", "file:///content/components.md").unwrap();
    let mut document = markdown::Parser::new().parse(&source).unwrap();
    document.blocks = vec![Block::Section {
        id: ComponentId("summary".into()),
        level: 2,
        role: Some("summary".into()),
        title: vec![Inline::Text {
            value: "Summary".into(),
        }],
        blocks: vec![Block::Component {
            id: ComponentId("warning".into()),
            name: "callout".into(),
            properties: BTreeMap::from([(
                "severity".into(),
                PropertyValue::Text("warning".into()),
            )]),
            blocks: vec![Block::Paragraph {
                content: vec![Inline::Text {
                    value: "Check this.".into(),
                }],
            }],
        }],
    }];

    let output = Renderer::default().render(&document);
    let output = output.as_str();

    assert!(output.contains("<section data-role=\"summary\">"));
    assert!(output.contains("id=\"summary\"></a>Summary</h2>"));
    assert!(
        output
            .contains("<div id=\"warning\" data-component=\"callout\" data-severity=\"warning\">")
    );
    assert!(output.contains("<p>Check this.</p>"));
}

#[test]
fn renders_code_without_reinterpreting_it_as_markdown() {
    let source = markdown::Source::new("", "file:///content/code.md").unwrap();
    let mut document = markdown::Parser::new().parse(&source).unwrap();
    document.blocks = vec![Block::Code(CodeBlock {
        references: Default::default(),
        caption: None,
        highlights: Vec::new(),
        language: None,
        value: "{{< unknown >}}\n".into(),
    })];

    let output = Renderer::default().render(&document);

    assert!(output.as_str().contains("unknown"));
    assert!(output.as_str().contains("</code></pre>"));
}

fn render_code_blocks(values: &[&str]) -> String {
    let source = markdown::Source::new("", "file:///content/code.md").unwrap();
    let mut document = markdown::Parser::new().parse(&source).unwrap();
    document.blocks = values
        .iter()
        .map(|value| {
            Block::Code(CodeBlock {
                references: Default::default(),
                caption: None,
                highlights: Vec::new(),
                language: Some("rust".into()),
                value: (*value).into(),
            })
        })
        .collect();
    Renderer::default().render(&document).as_str().to_owned()
}

fn listing_ids(html: &str) -> Vec<&str> {
    html.split("<figure class=\"code-listing\" id=\"")
        .skip(1)
        .map(|tail| tail.split('"').next().unwrap())
        .collect()
}

#[test]
fn listing_links_survive_unrelated_insertions_and_disambiguate_duplicates() {
    let original = render_code_blocks(&["let answer = 42;\n"]);
    let inserted = render_code_blocks(&[
        "// another example\n",
        "let answer = 42;\n",
        "let answer = 42;\n",
    ]);
    let original_id = listing_ids(&original)[0];
    let ids = listing_ids(&inserted);
    assert_eq!(original_id, ids[1]);
    assert_eq!(format!("{original_id}-2"), ids[2]);
    assert_ne!(ids[0], ids[1]);
    assert_eq!(original, render_code_blocks(&["let answer = 42;\n"]));
}

#[test]
fn multiline_listings_have_native_line_links_and_optional_copy_controls() {
    let html = render_code_blocks(&["fn main() {\n\n}\n"]);
    let id = listing_ids(&html)[0];
    for line in 1..=3 {
        assert!(html.contains(&format!("id=\"{id}-L{line}\" href=\"#{id}-L{line}\"")));
    }
    assert!(!html.contains("data-line=\"4\""));
    assert!(html.contains("class=\"code-copy\" hidden"));
    assert!(html.contains("role=\"status\""));
    assert!(html.contains("<pre tabindex=\"0\""));
    // Numbers are outside the code, so copy and selection retain the source.
    let code = html
        .split("<code ")
        .nth(1)
        .unwrap()
        .split("</code>")
        .next()
        .unwrap();
    assert!(!code.contains("data-line"));
}

#[test]
fn short_and_empty_listings_do_not_get_a_line_gutter() {
    for value in ["", "cargo build", "cargo build\n"] {
        assert!(!render_code_blocks(&[value]).contains("class=\"code-gutter\""));
    }
}

#[test]
fn code_markup_is_escaped_and_multiline_highlighting_is_preserved() {
    let html = render_code_blocks(&["/* first\nsecond */\nlet text = \"<script>&\";\n"]);
    assert!(!html.contains("<script>"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("&amp;"));
    assert!(html.contains("second */"));
}

#[test]
fn renders_ox_hugo_references_and_return_links_without_javascript() {
    let source = markdown::Source::in_memory(include_str!(
        "../../markdown/tests/fixtures/code-references.md"
    ));
    let document = markdown::Parser::new().parse(&source).unwrap();
    let html = Renderer::default().render(&document);
    let html = html.as_str();
    assert!(
        html.contains("<figure class=\"code-listing\" id=\"code-snippet--country-extraction\"")
    );
    assert!(html.contains("class=\"code-line-reference\" id=\"org-coderef--c81016-21\""));
    assert!(html.contains("href=\"#org-coderef--c81016-21-note-1\""));
    assert!(html.contains("href=\"#org-coderef--c81016-21\" class=\"code-reference\" id=\"org-coderef--c81016-21-note-1\""));
    assert!(html.contains("data-line=\"20\""));
    assert!(!html.contains("data-line=\"1\""));
}

#[test]
fn multiple_explanations_have_distinct_return_links_even_before_the_code() {
    let source = markdown::Source::in_memory(
        "[First](#ref-1) and **[second](#ref-1)**.\n\n```rust { id=example, lineanchors=ref }\nx\n```\n",
    );
    let document = markdown::Parser::new().parse(&source).unwrap();
    let html = Renderer::default().render(&document);
    for index in 1..=2 {
        assert!(
            html.as_str()
                .contains(&format!("id=\"ref-1-note-{index}\""))
        );
        assert!(
            html.as_str()
                .contains(&format!("href=\"#ref-1-note-{index}\""))
        );
    }
    assert!(html.as_str().contains("class=\"code-gutter\""));
}

#[test]
fn author_assigned_listing_identity_survives_code_edits() {
    for code in ["old", "new\ncode"] {
        let text = format!("```rust {{ id=example }}\n{code}\n```\n");
        let source = markdown::Source::in_memory(&text);
        let document = markdown::Parser::new().parse(&source).unwrap();
        assert!(
            Renderer::default()
                .render(&document)
                .as_str()
                .contains("<figure class=\"code-listing\" id=\"example\"")
        );
    }
}

#[test]
fn reference_led_notes_render_once_inside_native_line_disclosures() {
    let source = markdown::Source::in_memory(
        "```rust { id=example, lineanchors=ref }\nlet x = 1;\nx\n```\n\n1. [First](#ref-1) with **emphasis**.\n2. [Second](#ref-1) and `code`.\n",
    );
    let document = markdown::Parser::new().parse(&source).unwrap();
    let html = Renderer::default().render(&document);
    let html = html.as_str();
    assert_eq!(html.matches("<details class=\"code-note\">").count(), 1);
    assert!(!html.contains("<ol"));
    for id in [
        "ref-1-note-1",
        "ref-1-note-2",
        "ref-1",
        "example-L1",
        "example-L2",
    ] {
        assert_eq!(html.matches(&format!("id=\"{id}\"")).count(), 1);
    }
    assert!(html.contains("<strong>emphasis</strong>"));
    assert!(html.contains("and <code>code</code>"));
}

#[test]
fn annotated_source_preserves_exact_bytes_and_cannot_end_its_data_element() {
    let source = markdown::Source::in_memory(
        "```text { id=example, lineanchors=ref }\none\n```\n\n- [Explain](#ref-1) this line.\n",
    );
    let mut document = markdown::Parser::new().parse(&source).unwrap();
    let original = "\r\n</script><script>alert(1)</script>&\t\r\nlast";
    let Block::Code(code) = &mut document.blocks[0] else {
        panic!("code fixture");
    };
    code.value = original.into();
    let html = Renderer::default().render(&document);
    let data = html
        .as_str()
        .split("class=\"code-source\">")
        .nth(1)
        .unwrap()
        .split("</script>")
        .next()
        .unwrap();
    assert_eq!(serde_json::from_str::<String>(data).unwrap(), original);
    assert!(!data.contains('<'));
    assert!(!html.as_str().contains("<script>alert(1)</script>"));
}

#[test]
fn mixed_lists_and_task_lists_remain_in_the_prose() {
    for notes in [
        "1. [Explain](#ref-1) this.\n2. Ordinary item.",
        "1. [Explain](#ref-1) this.\n2. [External](https://example.com) context.",
        "- [ ] [Explain](#ref-1) this.",
    ] {
        let markdown = format!("```text {{ lineanchors=ref }}\none\n```\n\n{notes}\n");
        let document = markdown::Parser::new()
            .parse(&markdown::Source::in_memory(&markdown))
            .unwrap();
        let html = Renderer::default().render(&document);
        assert!(!html.as_str().contains("class=\"code-note\""));
        assert!(html.as_str().contains("this."));
        assert!(html.as_str().contains("<li>"));
    }
}

#[test]
fn renders_caption_inside_the_figure_and_highlights_by_source_offset() {
    let source = markdown::Source::in_memory(
        "```rust { linenostart=20, hl_lines=[\"2\"] }\n/* first\nsecond */\n```\n<div class=\"src-block-caption\">\n<span class=\"src-block-number\">Code Snippet 1:</span>\nA <em>multiline</em> comment.\n</div>\n",
    );
    let document = markdown::Parser::new().parse(&source).unwrap();
    let html = Renderer::default().render(&document);
    let html = html.as_str();
    assert!(html.contains("<span class=\"code-caption\">A <em>multiline</em> comment.</span>"));
    assert!(!html.contains("src-block-caption"));
    assert!(
        html.contains("class=\"code-line-reference is-highlighted\" style=\"--line-offset:1\"")
    );
    assert!(html.contains("data-line=\"21\""));
    assert!(html.contains("second */"));
    assert!(
        !html
            .split("<code ")
            .nth(1)
            .unwrap()
            .split("</code>")
            .next()
            .unwrap()
            .contains("--line-offset")
    );
}
#[test]
fn document_links_use_current_routes_and_keep_source_anchors() {
    let source =
        markdown::Source::new("[Earlier work](id:target#details)", "file:///source.md").unwrap();
    let document = markdown::Parser::new().parse(&source).unwrap();
    let renderer = berlin_document_html::Renderer::default().with_document_routes(
        [(
            berlin_document::ContentId("target".into()),
            "https://example.com/notebook/notes/renamed.html".into(),
        )]
        .into(),
    );
    let html = renderer.render(&document);
    assert!(html.as_str().contains("id=\"bln-ref-1\""));
    assert!(
        html.as_str()
            .contains("href=\"https://example.com/notebook/notes/renamed.html#details\"")
    );
    let standalone = berlin_document_html::Renderer::default().render(&document);
    assert!(standalone.as_str().contains("Earlier work"));
    assert!(!standalone.as_str().contains("href="));
    assert!(!standalone.as_str().contains("id:target"));
}
