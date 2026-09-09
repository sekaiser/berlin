//! A synthetic renderer preview, not a LaTeX import example.
use berlin_document::{Block, ComponentId, Equation, Inline};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = markdown::Parser::new().parse(&markdown::Source::in_memory(
        "# Numbered equations\n\nA synthetic document demonstrating source-assigned numbers and local references.",
    ))?;
    document.blocks.push(Block::Paragraph {
        content: vec![Inline::CrossReference {
            target: ComponentId("eq:work".into()),
            content: vec![Inline::Text {
                value: "See Equation (2.1).".into(),
            }],
        }],
    });
    for (id, number, tex) in [
        (
            "eq:work",
            "2.1",
            r"W_{\mathrm{full}}=nc,\qquad W_{\mathrm{incremental}}=kc",
        ),
        (
            "eq:aligned",
            "2.2",
            r"\begin{align}a &= b+c \\ d &= e\end{align}",
        ),
        (
            "eq:wide",
            "2.3",
            r"W=\sum_{i=1}^{n}c_i+\sum_{i=1}^{n}d_i+\sum_{i=1}^{n}e_i+\sum_{i=1}^{n}f_i+\sum_{i=1}^{n}g_i+\sum_{i=1}^{n}h_i",
        ),
    ] {
        document.blocks.push(Block::Equation(Equation {
            id: ComponentId(id.into()),
            number: number.into(),
            tex: tex.into(),
        }));
    }
    document.validate()?;
    let html = berlin_document_html::Renderer::default().render(&document)?;
    println!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Numbered equations — renderer preview</title><style>body{{margin:0;padding:24px;background:#e5e9e0;color:#26362d;font:18px/1.6 Georgia,serif}}main{{box-sizing:border-box;max-width:760px;margin:auto;padding:24px;background:#fff9ed}}h1{{font-size:2em;line-height:1.2}}a{{color:inherit}}</style><body><main>{}</main></body></html>",
        html.as_str()
    );
    Ok(())
}
