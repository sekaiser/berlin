use berlin_content::DocumentCollection;
use berlin_document::*;
use berlin_document_html::{FragmentError, Renderer};
use berlin_publication::*;
use berlin_publication_html::excerpt_context;

fn paragraph(content: Vec<Inline>) -> Block {
    Block::Paragraph { content }
}
fn fixture() -> (PublicationAssembly, ExcerptRef) {
    let blocks = vec![
        Block::Section {
            id: ComponentId("selected".into()),
            level: 2,
            role: None,
            title: vec![Inline::Text {
                value: "Selected".into(),
            }],
            blocks: vec![
                paragraph(vec![
                    Inline::Link {
                        destination: "#selected".into(),
                        title: None,
                        content: vec![Inline::Text {
                            value: "Inside".into(),
                        }],
                    },
                    Inline::Link {
                        destination: "#outside".into(),
                        title: None,
                        content: vec![Inline::Text {
                            value: "Outside".into(),
                        }],
                    },
                    Inline::FootnoteReference {
                        name: "note".into(),
                    },
                ]),
                Block::Code(CodeBlock {
                    language: Some("rust".into()),
                    value: "let x = 1;\nprintln!(\"{x}\");\n".into(),
                    references: CodeReferences {
                        id: Some("listing".into()),
                        line_anchor_prefix: Some("line".into()),
                        first_line: 1,
                    },
                    caption: Some(vec![Inline::Text {
                        value: "Code caption".into(),
                    }]),
                    highlights: vec![CodeLineRange { start: 1, end: 1 }],
                }),
                paragraph(vec![Inline::Link {
                    destination: "#line-1".into(),
                    title: None,
                    content: vec![Inline::Text {
                        value: "First line".into(),
                    }],
                }]),
                Block::Figure(Figure {
                    source: "/attachments/figure.svg".into(),
                    caption: Some("Figure caption".into()),
                    alt: Some("Example figure".into()),
                }),
            ],
        },
        Block::Section {
            id: ComponentId("outside".into()),
            level: 2,
            role: None,
            title: vec![],
            blocks: vec![],
        },
        Block::FootnoteDefinition {
            name: "note".into(),
            blocks: vec![paragraph(vec![Inline::Text {
                value: "Footnote text".into(),
            }])],
        },
    ];
    let document = Document {
        id: ContentId("source".into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some("Original".into()),
            ..Default::default()
        },
        blocks,
        relations: vec![],
        provenance: Provenance {
            source: "file:///private/source.md".into(),
            source_format: SourceFormat::Markdown,
            source_hash: "a".repeat(64),
        },
    };
    let catalog = PublicationCatalog::new(
        DocumentCollection::new(vec![document]),
        PublicationOptions {
            source_site_url: Some("https://source.example/notebook".parse().unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    let reference = catalog.section_ref("source", "selected").unwrap();
    let call = Excerpt {
        source: reference.clone(),
        annotation: None,
    }
    .into_call();
    let spec = PublicationSpec {
        schema_version: 1,
        metadata: PublicationMetadata {
            id: PublicationId("example".into()),
            title: "Example".into(),
            description: None,
        },
        root: Portfolio {}.into_call().with_body(ViewFragment(vec![
            call.clone().keyed("first"),
            call.keyed("second"),
        ])),
    };
    (
        PublicationAssembly::resolve(&spec, catalog, &ComponentRegistry::builtins()).unwrap(),
        reference,
    )
}

#[test]
fn b05_b06_b07_b09_occurrences_namespace_anchors_and_keep_source_code() {
    let (assembly, reference) = fixture();
    let excerpt = assembly.excerpt(&reference).unwrap();
    let renderer = Renderer::default();
    let first = &assembly.placements()[1].occurrence.id;
    let second = &assembly.placements()[2].occurrence.id;
    let a = renderer
        .render_fragment(
            excerpt.blocks(),
            &excerpt_context(excerpt, first, 1).unwrap(),
            None,
            1_000_000,
        )
        .unwrap();
    let b = renderer
        .render_fragment(
            excerpt.blocks(),
            &excerpt_context(excerpt, second, 1).unwrap(),
            None,
            1_000_000,
        )
        .unwrap();
    for (html, id) in [(a.as_str(), first), (b.as_str(), second)] {
        for anchor in ["selected", "listing", "listing-L1", "line-1", "fn-note"] {
            assert!(
                html.contains(&format!("id=\"{}--{anchor}\"", id.0)),
                "{anchor}: {html}"
            );
        }
        assert!(html.contains(&format!("href=\"#{}--selected\"", id.0)));
        assert!(html.contains(&format!("href=\"#{}--line-1\"", id.0)));
        assert!(
            html.contains("href=\"https://source.example/notebook/notes/original.html#outside\"")
        );
        assert!(html.contains("https://source.example/notebook/attachments/figure.svg"));
        assert!(html.contains("Code caption"));
        assert!(html.contains("Figure caption"));
        assert!(html.contains("Footnote text"));
        assert!(html.contains("<h3>"));
        assert!(!html.contains("file:///"));
    }
    assert!(!a.as_str().contains(&format!("id=\"{}--", second.0)));
    assert_ne!(a, b);
}

#[test]
fn f19_semantic_fragment_limits_fail_without_panicking() {
    let (assembly, reference) = fixture();
    let excerpt = assembly.excerpt(&reference).unwrap();
    let context = excerpt_context(excerpt, &assembly.placements()[1].occurrence.id, 0).unwrap();
    let renderer = Renderer::default();
    assert!(matches!(
        renderer.render_fragment(excerpt.blocks(), &context, None, 10),
        Err(FragmentError::OutputLimit)
    ));
    let complete = renderer
        .render_fragment(excerpt.blocks(), &context, None, 1_000_000)
        .unwrap();
    let exact = complete.as_str().len();
    assert_eq!(
        renderer
            .render_fragment(excerpt.blocks(), &context, None, exact)
            .unwrap(),
        complete
    );
    assert!(matches!(
        renderer.render_fragment(excerpt.blocks(), &context, None, exact - 1),
        Err(FragmentError::OutputLimit)
    ));
}
