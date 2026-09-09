use berlin_document::{Block, ComponentId, Document, Equation, Inline, plain_text};
use berlin_document_html::Renderer;

fn text(value: &str) -> Vec<Inline> {
    vec![Inline::Text {
        value: value.into(),
    }]
}

fn reference(target: &str, label: &str) -> Inline {
    Inline::CrossReference {
        target: ComponentId(target.into()),
        content: text(label),
    }
}

fn paper() -> Document {
    let mut document = markdown::Parser::new()
        .parse(&markdown::Source::in_memory("Paper"))
        .unwrap();
    document.blocks = vec![
        Block::Paragraph {
            content: vec![
                Inline::Text {
                    value: "Equation ".into(),
                },
                reference("eq:work", "(2.1)"),
                Inline::Text {
                    value: " appears in Section ".into(),
                },
                reference("sec:model", "2"),
                Inline::Text { value: ".".into() },
            ],
        },
        Block::Section {
            id: ComponentId("sec:model".into()),
            level: 2,
            role: None,
            title: text("2. A cost model"),
            blocks: vec![Block::Equation(Equation {
                id: ComponentId("eq:work".into()),
                number: "2.1".into(),
                tex: r"W_{\mathrm{full}}=nc,\qquad W_{\mathrm{incremental}}=kc".into(),
            })],
        },
    ];
    document
}

#[test]
fn retains_forward_references_and_source_numbering_in_html_and_json() {
    let document = paper();
    document.validate().unwrap();
    let html = Renderer::default().render(&document).unwrap();
    let html = html.as_str();
    assert!(html.contains("<a href=\"#eq:work\">(2.1)</a>"));
    assert!(html.contains("<a href=\"#sec:model\">2</a>"));
    assert!(html.contains("class=\"berlin-equation\" id=\"eq:work\""));
    assert!(html.contains("aria-label=\"Equation 2.1\">(2.1)</a>"));
    assert_eq!(html.matches("<math ").count(), 1);
    assert_eq!(html.matches("<style>").count(), 1);
    assert!(!html.contains("<script"));
    assert!(!html.contains("<merror"));
    let encoded = serde_json::to_string(&document).unwrap();
    assert_eq!(document, serde_json::from_str(&encoded).unwrap());
    let Block::Paragraph { content } = &document.blocks[0] else {
        panic!()
    };
    assert_eq!(plain_text(content), "Equation (2.1) appears in Section 2.");
}

#[test]
fn rejects_a_reference_whose_target_was_removed() {
    let mut document = paper();
    document.blocks.pop();
    assert!(
        document
            .validate()
            .unwrap_err()
            .to_string()
            .contains("unknown target 'eq:work'")
    );
}

#[test]
fn checks_duplicate_anchors_across_block_kinds() {
    let mut document = paper();
    document.blocks.push(Block::Heading {
        level: 2,
        id: Some("eq:work".into()),
        content: text("Duplicate"),
    });
    assert!(
        document
            .validate()
            .unwrap_err()
            .to_string()
            .contains("duplicate anchor 'eq:work'")
    );
}

#[test]
fn rejects_empty_numbers_labels_and_unsafe_reference_fragments() {
    let mut document = paper();
    for (target, label) in [
        ("eq:work", " "),
        ("eq:work#fragment", "(2.1)"),
        ("eq:work\"", "(2.1)"),
    ] {
        document.blocks[0] = Block::Paragraph {
            content: vec![reference(target, label)],
        };
        assert!(document.validate().is_err());
    }
    let mut document = paper();
    let Block::Section { blocks, .. } = &mut document.blocks[1] else {
        panic!()
    };
    let Block::Equation(equation) = &mut blocks[0] else {
        panic!()
    };
    equation.number.clear();
    assert!(
        document
            .validate()
            .unwrap_err()
            .to_string()
            .contains("equation number")
    );
}

#[test]
fn checks_references_inside_nested_emphasis_tables_and_captions() {
    let mut document = paper();
    document.blocks.push(Block::Table(berlin_document::Table {
        alignments: vec![],
        rows: vec![berlin_document::TableRow {
            header: false,
            cells: vec![vec![Inline::Strong {
                content: vec![reference("missing", "Missing")],
            }]],
        }],
    }));
    assert!(
        document
            .validate()
            .unwrap_err()
            .to_string()
            .contains("unknown target 'missing'")
    );
    document.blocks.pop();
    document
        .blocks
        .push(Block::Code(berlin_document::CodeBlock {
            language: None,
            value: "example".into(),
            references: Default::default(),
            caption: Some(vec![reference("missing", "Missing")]),
            highlights: vec![],
        }));
    assert!(
        document
            .validate()
            .unwrap_err()
            .to_string()
            .contains("unknown target 'missing'")
    );
}

#[test]
fn rejects_nested_links_in_either_direction() {
    let mut document = paper();
    for inline in [
        Inline::Link {
            destination: "https://example.com".into(),
            title: None,
            content: vec![reference("eq:work", "(2.1)")],
        },
        Inline::CrossReference {
            target: ComponentId("eq:work".into()),
            content: vec![Inline::Strong {
                content: vec![Inline::Link {
                    destination: "https://example.com".into(),
                    title: None,
                    content: text("Nested"),
                }],
            }],
        },
    ] {
        document.blocks[0] = Block::Paragraph {
            content: vec![inline],
        };
        assert!(
            document
                .validate()
                .unwrap_err()
                .to_string()
                .contains("nested")
        );
    }
}

#[test]
fn renders_math_in_reference_labels_and_escapes_equation_numbers() {
    let mut document = paper();
    document.blocks[0] = Block::Paragraph {
        content: vec![Inline::CrossReference {
            target: ComponentId("eq:work".into()),
            content: vec![Inline::Math { tex: "W".into() }],
        }],
    };
    let Block::Section { blocks, .. } = &mut document.blocks[1] else {
        panic!()
    };
    let Block::Equation(equation) = &mut blocks[0] else {
        panic!()
    };
    equation.number = "2<\"&".into();
    document.validate().unwrap();
    let html = Renderer::default().render(&document).unwrap();
    assert_eq!(html.as_str().matches("<math ").count(), 2);
    assert!(
        html.as_str()
            .contains("aria-label=\"Equation 2&lt;&quot;&amp;\"")
    );
    assert!(html.as_str().contains("(2&lt;\"&amp;)"));
}
