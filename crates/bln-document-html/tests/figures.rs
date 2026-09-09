use berlin_document::{Block, Figure};

#[test]
fn figure_alt_survives_both_renderers_and_round_trips_without_becoming_markup() {
    for caption in ["", r#" caption="Visible caption""#] {
        for (argument, expected) in [
            ("", None),
            (r#" alt="""#, Some("")),
            (
                r#" alt="Input \"A\" & <output>""#,
                Some("Input \"A\" & <output>"),
            ),
        ] {
            let text = format!(r#"{{{{< figure src="/diagram.svg"{argument}{caption} >}}}}"#);
            let source = markdown::Source::in_memory(&text);
            let document = markdown::Parser::new().parse(&source).unwrap();
            let Block::Figure(figure) = &document.blocks[0] else {
                panic!("expected a typed figure");
            };
            assert_eq!(figure.alt.as_deref(), expected);
            let serialized = serde_json::to_string(figure).unwrap();
            assert_eq!(
                serde_json::from_str::<Figure>(&serialized).unwrap(),
                *figure
            );
            let semantic = berlin_document_html::Renderer::default()
                .render(&document)
                .unwrap();
            let direct = markdown::Renderer::default().render(&source).unwrap();
            for html in [semantic.as_str(), direct.as_str()] {
                match expected {
                    Some("") => assert!(html.contains(" alt=\"\"")),
                    Some(_) => {
                        assert!(html.contains("alt=\"Input &quot;A&quot; &amp; &lt;output&gt;\""));
                        assert!(!html.contains("<output>"));
                    }
                    None => assert!(!html.contains(" alt=")),
                }
            }
        }
    }
}

#[test]
fn older_serialized_figures_remain_valid() {
    let json = r#"{"source":"/diagram.svg","caption":null}"#;
    let figure: Figure = serde_json::from_str(json).unwrap();
    assert_eq!(figure.alt, None);
    assert_eq!(serde_json::to_string(&figure).unwrap(), json);
}
