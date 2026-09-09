use berlin_document::Block;
use markdown::{Parser, Renderer, Source};

const NOTE: &str = r"---
id: equations
references:
  - id: example
    text: Example Author. A synthetic reference. 2026.
    url: https://notebook.example.org/reference
---
See \eqref{eq:sum}, or equation \ref{eq:sum}, and \cite{example}.

```math
\begin{equation}
x = a + b \label{eq:sum}
\end{equation}
```

Code stays literal: `\cite{missing}` and `\ref{missing}`.
";

#[test]
fn labels_and_citations_resolve_forward_in_both_projections() {
    let source = Source::in_memory(NOTE);
    let document = Parser::new().parse(&source).unwrap();
    assert!(
        matches!(&document.blocks[1], Block::Equation(eq) if eq.id.0 == "eq:sum" && eq.number == "1" && !eq.tex.contains("label"))
    );
    let json = serde_json::to_string(&document).unwrap();
    assert!(json.contains("ref-example"));
    let html = Renderer::default().render(&source).unwrap();
    for text in [
        "href=\"#eq:sum\"",
        "id=\"eq:sum\"",
        "href=\"#ref-example\"",
        "id=\"ref-example\"",
        "A synthetic reference",
        "<math ",
        "<code>\\cite{missing}</code>",
    ] {
        assert!(html.as_str().contains(text), "{text}: {html}");
    }
}

#[test]
fn invalid_references_fail_without_silent_loss() {
    for text in [
        NOTE.replace("\\eqref{eq:sum}", "\\eqref{missing}"),
        NOTE.replace("\\cite{example}", "\\cite{missing}"),
        NOTE.replace("x = a + b", "\\label{duplicate} x = a + b"),
        NOTE.replace("\\cite{example}", "[\\cite{example}](https://example.org)"),
        NOTE.replace("\\cite{example}", "![\\cite{example}](image.png)"),
        NOTE.replace(
            "https://notebook.example.org/reference",
            "javascript:alert(1)",
        ),
    ] {
        let source = Source::in_memory(&text);
        assert!(Parser::new().parse(&source).is_err(), "{text}");
        assert!(Renderer::default().render(&source).is_err(), "{text}");
    }
}

#[test]
fn citation_labels_and_text_are_escaped_and_only_cited_entries_appear() {
    let text = NOTE
        .replace("id: example", "id: example\n    label: '<EX>'")
        .replace("text: Example Author.", "text: <Example> Author.");
    let html = Renderer::default()
        .render(&Source::in_memory(&text))
        .unwrap();
    assert!(html.as_str().contains("&lt;EX&gt;"));
    assert!(html.as_str().contains("&lt;Example&gt;"));
    let uncited = NOTE.replace("\\cite{example}", "no citation");
    assert!(
        !Renderer::default()
            .render(&Source::in_memory(&uncited))
            .unwrap()
            .as_str()
            .contains("id=\"references\"")
    );
}
