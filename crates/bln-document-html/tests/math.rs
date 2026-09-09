use berlin_document::{Block, Inline};
use berlin_document_html::Renderer;

#[test]
fn renders_native_mathml_including_nested_content_once_per_page() {
    let source = markdown::Source::in_memory(
        r"# The value \(x_i\)

Inline \(x_i\) and **\(a_b\)**.

\[\frac{1}{2}\]

- In a list: \(c_d\).

| Symbol |
|--------|
| \(e_f\) |

```math
\begin{bmatrix}1 & 2 \\ 3 & 4\end{bmatrix}
```
",
    );
    let document = markdown::Parser::new().parse(&source).unwrap();
    let html = Renderer::default().render(&document).unwrap();
    assert_eq!(html.as_str().matches("<math ").count(), 7);
    assert_eq!(html.as_str().matches("<style>").count(), 1);
    assert_eq!(html.as_str().matches("aria-label=\"Equation\"").count(), 2);
    assert!(html.as_str().contains("<mfrac>"));
    assert!(html.as_str().contains("<mtable"));
    assert!(!html.as_str().contains("<script"));
    assert!(!html.as_str().contains("<merror"));
}

#[test]
fn checks_scripted_math_before_returning_any_html() {
    let source = markdown::Source::in_memory("Intro");
    let mut document = markdown::Parser::new().parse(&source).unwrap();
    document.blocks.push(Block::Math {
        tex: r"\input{private.tex}".into(),
    });
    assert!(Renderer::default().render(&document).is_err());
    document.blocks = vec![Block::Paragraph {
        content: vec![Inline::Math { tex: " ".into() }],
    }];
    assert!(document.validate().is_err());
    assert!(Renderer::default().render(&document).is_err());
    document.blocks = vec![Block::Math { tex: " ".into() }];
    assert!(document.validate().is_err());
}
