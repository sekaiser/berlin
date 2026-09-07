use berlin_document::Block;
use berlin_document::Inline;
use markdown::Error;
use markdown::Parser;
use markdown::Renderer;
use markdown::Source;
use url::Url;

#[test]
fn parses_front_matter_into_document_metadata() {
    let source = Source::in_memory(
        r#"---
title: "Pipeline test"
date: 2026-09-05
tags: ["berlin"]
---

Body
"#,
    );

    let document = Parser::new().parse(&source).unwrap();

    assert_eq!(document.metadata.title.as_deref(), Some("Pipeline test"));
    assert_eq!(
        document
            .metadata
            .published
            .as_ref()
            .map(|date| date.as_str()),
        Some("2026-09-05")
    );
    assert_eq!(document.metadata.tags, ["berlin"]);
}

#[test]
fn rejects_invalid_dates_at_the_front_matter_boundary() {
    for field in ["date", "lastmod"] {
        for value in ["2025-02-29", "2026-09-05T12:00:00Z"] {
            let text = format!("---\n{field}: \"{value}\"\n---\nBody\n");
            let source = Source::new(&text, "file:///content/invalid-date.md").unwrap();
            assert!(matches!(
                Parser::new().parse(&source),
                Err(Error::InvalidFrontMatter { source_uri, .. })
                    if source_uri == "file:///content/invalid-date.md"
            ));
        }
    }
}

#[test]
fn rejects_invalid_source_uris() {
    let error = Source::new("Body", "not a URI").unwrap_err();

    assert!(matches!(
        error,
        Error::InvalidSourceUri { source_uri, .. } if source_uri == "not a URI"
    ));
}

#[test]
fn reports_invalid_front_matter_with_its_source() {
    let source = Source::new(
        "---\ntitle: [unterminated\n---\nBody\n",
        "file:///content/broken.md",
    )
    .unwrap();
    let error = Parser::new().parse(&source).unwrap_err();

    assert!(matches!(
        error,
        Error::InvalidFrontMatter { source_uri, .. }
            if source_uri == "file:///content/broken.md"
    ));
}

#[test]
fn reports_unsupported_shortcodes_instead_of_leaving_placeholders() {
    let text = "{{< unknown value=\"test\" >}}\n";
    let document_source = Source::new(text, "file:///content/unsupported.md").unwrap();
    let html_source = Source::in_memory(text);

    let document_error = Parser::new().parse(&document_source).unwrap_err();
    let html_error = Renderer::default().render(&html_source).unwrap_err();

    assert!(matches!(document_error, Error::InvalidShortcode { .. }));
    assert!(
        document_error
            .to_string()
            .contains("unsupported shortcode 'unknown'")
    );
    assert!(matches!(html_error, Error::InvalidShortcode { .. }));
}

#[test]
fn produces_semantic_and_html_projections() {
    let source = Source::new(
        r#"---
title: "Semantic test"
author: ["Ada"]
description: "A typed document"
date: 2026-09-05
lastmod: 2026-09-06
tags: ["berlin", "rust"]
draft: true
id: "article-1"
---

Intro with **meaning**, [a link](https://example.com), and `code`.

## A section {#a-section}

- [ ] first
- [x] second

| Name | Value |
|------|------:|
| one  |     1 |

{{< figure src="/pics/example.png" caption="<span class=\"figure-number\">An example</span>" >}}

```rust
fn main() {}
```
"#,
        "file:///content/article.md",
    )
    .unwrap();

    let document = Parser::new().parse(&source).unwrap();
    let html = Renderer::default().render(&source).unwrap();

    assert_eq!(document.id.0, "article-1");
    assert_eq!(document.metadata.title.as_deref(), Some("Semantic test"));
    assert_eq!(document.metadata.authors, ["Ada"]);
    assert_eq!(
        document
            .metadata
            .modified
            .as_ref()
            .map(|date| date.as_str()),
        Some("2026-09-06")
    );
    assert_eq!(document.metadata.tags, ["berlin", "rust"]);
    assert!(document.metadata.draft);
    assert_eq!(document.provenance.source, "file:///content/article.md");
    assert_eq!(document.provenance.source_hash.len(), 64);

    let section = document
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Section {
                level,
                id,
                title,
                blocks,
                ..
            } => Some((level, id, title, blocks)),
            _ => None,
        })
        .expect("section should be represented");
    assert_eq!(*section.0, 2);
    assert_eq!(section.1.0, "a-section");
    assert_eq!(
        section.2,
        &vec![Inline::Text {
            value: "A section".into()
        }]
    );

    let list = section
        .3
        .iter()
        .find_map(|block| match block {
            Block::List(list) => Some(list),
            _ => None,
        })
        .expect("list should be represented");
    assert_eq!(
        list.items
            .iter()
            .map(|item| item.checked)
            .collect::<Vec<_>>(),
        vec![Some(false), Some(true)]
    );
    assert!(
        section
            .3
            .iter()
            .any(|block| matches!(block, Block::Table(_)))
    );
    assert!(section.3.iter().any(|block| matches!(
        block,
        Block::Code(code) if code.language.as_deref() == Some("rust")
    )));
    assert!(section.3.iter().any(|block| matches!(
        block,
        Block::Figure(figure)
            if figure.source == "/pics/example.png"
                && figure.caption.as_deref()
                    == Some("<span class=\"figure-number\">An example</span>")
    )));

    assert!(
        html.as_str().contains("<h2 id=\"a-section\">A section"),
        "{html}"
    );
    assert!(!html.as_str().contains("{#a-section}"));
}

#[test]
fn groups_heading_ranges_into_nested_stable_sections() {
    let source = Source::new(
        "## Same\n\nFirst\n\n### Child\n\nNested\n\n## Same\n\nSecond\n",
        "file:///content/sections.md",
    )
    .unwrap();
    let document = Parser::new().parse(&source).unwrap();

    let Block::Section {
        id, level, blocks, ..
    } = &document.blocks[0]
    else {
        panic!("expected first section")
    };
    assert_eq!(id.0, "same");
    assert_eq!(*level, 2);
    assert!(matches!(blocks[1], Block::Section { level: 3, .. }));
    let Block::Section { id, .. } = &document.blocks[1] else {
        panic!("expected second section")
    };
    assert_eq!(id.0, "same-1");
}

#[test]
fn resolves_relrefs_before_building_the_semantic_document() {
    let directory = tempfile::tempdir().expect("temporary directory should exist");
    let target = directory.path().join("target.md");
    std::fs::write(&target, "---\ntitle: Target article\n---\n")
        .expect("target fixture should be written");
    let source_path = directory.path().join("source.md");
    let source_uri = Url::from_file_path(&source_path).expect("source path should become a URL");
    let source = Source::new(
        "[Target]({{< relref \"target.md\" >}})\n",
        source_uri.as_str(),
    )
    .unwrap();

    let document = Parser::new().parse(&source).unwrap();

    assert!(document.blocks.iter().any(|block| matches!(
        block,
        Block::Paragraph { content }
            if matches!(
                content.as_slice(),
                [Inline::Link { destination, .. }]
                    if destination == "/notes/target-article.html"
            )
    )));
}
