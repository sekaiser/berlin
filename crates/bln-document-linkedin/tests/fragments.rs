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
