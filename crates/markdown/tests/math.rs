use berlin_document::{Block, Inline};
use markdown::{Error, Parser, Renderer, Source};

#[test]
fn keeps_tex_typed_and_splits_display_equations_from_prose() {
    let text = r"Before \(x_i^2 + \frac{1}{2}\), $a_b$, and $`c_d`$.

First \[\sum_{i=1}^n x_i\] last.

$$E = mc^2$$

```math
\begin{aligned}a &= b+c \\ d &= e\end{aligned}
```
";
    let document = Parser::new().parse(&Source::in_memory(text)).unwrap();
    let Block::Paragraph { content } = &document.blocks[0] else {
        panic!("paragraph")
    };
    let equations = content
        .iter()
        .filter_map(|inline| match inline {
            Inline::Math { tex } => Some(tex.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(equations, [r"x_i^2 + \frac{1}{2}", "a_b", "c_d"]);
    assert!(matches!(&document.blocks[1], Block::Paragraph { .. }));
    assert!(matches!(&document.blocks[2], Block::Math { tex } if tex == r"\sum_{i=1}^n x_i"));
    assert!(matches!(&document.blocks[3], Block::Paragraph { .. }));
    assert!(matches!(&document.blocks[4], Block::Math { tex } if tex == "E = mc^2"));
    assert!(
        matches!(&document.blocks[5], Block::Math { tex } if tex.starts_with(r"\begin{aligned}"))
    );
    let json = serde_json::to_string(&document).unwrap();
    assert_eq!(document, serde_json::from_str(&json).unwrap());
}

#[test]
fn renders_math_without_scripts_and_leaves_code_and_currency_alone() {
    let source = Source::in_memory(
        r#"Inline \(x_i\).

\[\frac{a}{b}\]

The price is $5.00 and the other price is $10.00.

`\(not_math\)` and `$literal$`.

```rust
let cost = "$x_i$";
```

```latex
\unknownBerlinCommand{x}
```

```math
\sqrt{x}
```

Plain *emphasis*.
"#,
    );
    let html = Renderer::default().render(&source).unwrap();
    assert_eq!(html.as_str().matches("<math ").count(), 3);
    assert_eq!(html.as_str().matches("<style>").count(), 1);
    assert!(html.as_str().contains("<mfrac>"));
    assert!(html.as_str().contains("<code>\\(not_math\\)</code>"));
    assert!(html.as_str().contains("<code>$literal$</code>"));
    assert!(html.as_str().contains("$5.00"));
    assert!(html.as_str().contains("<em>emphasis</em>"));
    assert!(!html.as_str().contains("<script"));
    Parser::new().parse(&source).unwrap();
    assert!(
        !Renderer::default()
            .render(&Source::in_memory("Ordinary prose."))
            .unwrap()
            .as_str()
            .contains("<style>")
    );
}

#[test]
fn rejects_bad_math_with_source_and_line_in_both_projections() {
    for text in [
        "Intro\n\n\\(\\unknownBerlinCommand{x}\\)",
        "Intro\n\n$$\\frac{a}$$",
        "Intro\n\n```math\n\\input{private.tex}\n```",
        "Intro\n\n*\\[x\\]*",
    ] {
        let source = Source::new(text, "file:///content/math.md").unwrap();
        for error in [
            Parser::new().parse(&source).unwrap_err(),
            Renderer::default().render(&source).unwrap_err(),
        ] {
            assert!(
                matches!(error, Error::InvalidMath { source_uri, line: 3, .. } if source_uri == "file:///content/math.md")
            );
        }
    }
}
