use berlin_document::{Block, CodeBlock, CodeReferences, Inline};
use berlin_document_linkedin::fragment::{Context, write_blocks};

#[test]
fn portable_text_keeps_literal_characters_and_exact_code_whitespace() {
    let code = "  let x = 1;\n\tprintln!(\"<>&\");\n\n";
    let blocks = vec![
        Block::Paragraph {
            content: vec![Inline::Text {
                value: "Literal <script> & >".into(),
            }],
        },
        Block::Code(CodeBlock {
            language: Some("rust".into()),
            value: code.into(),
            references: CodeReferences::default(),
            caption: None,
            highlights: vec![],
        }),
    ];
    let mut output = Vec::new();
    write_blocks(&blocks, &Context::default(), &mut output).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        format!("Literal <script> & >\n\n{code}")
    );
    assert!(
        write_blocks(
            &[Block::Html {
                value: "<b>opaque</b>".into()
            }],
            &Context::default(),
            &mut Vec::new()
        )
        .is_err()
    );
}

#[test]
fn portable_bibliography_keeps_prose_and_rejects_properties_or_empty_entries() {
    use berlin_document::{ComponentId, PropertyValue};
    use std::collections::BTreeMap;
    let entry = |properties, blocks| Block::Component {
        id: ComponentId("ref-example".into()),
        name: "bibliography-entry".into(),
        properties,
        blocks,
    };
    let prose = vec![Block::Paragraph {
        content: vec![Inline::Text {
            value: "[1] Example <source> & author.".into(),
        }],
    }];
    let mut output = Vec::new();
    write_blocks(
        &[entry(BTreeMap::new(), prose.clone())],
        &Context::default(),
        &mut output,
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "[1] Example <source> & author."
    );
    for invalid in [
        entry(BTreeMap::new(), vec![]),
        entry(
            BTreeMap::from([("unsafe".into(), PropertyValue::Text("ignored?".into()))]),
            prose,
        ),
    ] {
        assert!(write_blocks(&[invalid], &Context::default(), &mut Vec::new()).is_err());
    }
}
