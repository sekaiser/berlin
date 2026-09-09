use berlin_document::{ContentId, Metadata, Provenance, SourceFormat};
use berlin_paper::{Labels, Publication};
use serde_json::{Value, json};

const AST: &str = include_str!("fixtures/paper.json");
const AUX: &str = include_str!("fixtures/paper.aux");
fn bibliography() -> Vec<berlin_paper::BibliographyEntry> {
    vec![berlin_paper::BibliographyEntry {
        key: "example".into(),
        pandoc_json: include_str!("fixtures/reference.json").into(),
    }]
}
const SOURCE: &str = include_str!("../../../support/fixtures/papers/paper.tex");

fn publication() -> Publication {
    Publication {
        id: ContentId("paper-test".into()),
        metadata: Metadata {
            published: Some("2026-09-09".parse().unwrap()),
            ..Default::default()
        },
        provenance: Provenance {
            source: "file:///papers/paper.tex".into(),
            source_format: SourceFormat::Latex,
            source_hash: "0".repeat(64),
        },
        pdf_url: "/papers/example.pdf".into(),
    }
}
fn import(ast: &Value, aux: &str) -> Result<berlin_document::Document, berlin_paper::Error> {
    berlin_paper::import(
        &ast.to_string(),
        &Labels::parse(aux)?,
        &bibliography(),
        publication(),
    )
}

#[test]
fn preserves_the_synthetic_paper_in_notebook_semantics() {
    berlin_paper::validate_source(SOURCE, Some("references")).unwrap();
    let document = berlin_paper::import(
        AST,
        &Labels::parse(AUX).unwrap(),
        &bibliography(),
        publication(),
    )
    .unwrap();
    document.validate().unwrap();
    assert_eq!(document.metadata.authors, ["Example Author"]);
    assert_eq!(
        document.metadata.title.as_deref(),
        Some("A Small Study of Incremental Publishing")
    );
    let html = berlin_document_html::Renderer::default()
        .render(&document)
        .unwrap();
    for expected in [
        "This synthetic paper",
        "Download the paper (PDF)",
        "href=\"/papers/example.pdf\"",
        "id=\"eq:work\"",
        "href=\"#eq:work\">(1)</a>",
        "href=\"#sec:model\">1</a>",
        "href=\"#tab:cost\">1</a>",
        "href=\"#ref-example\">[1]</a>",
        "id=\"ref-example\"",
        "Synthetic reference for conversion testing",
        "Test fixture; not a real publication.",
        "id=\"fn-1\"",
        "class=\"footnote-label\">[1]</span>",
        "id=\"paper-footnotes\"",
        "1. A simple cost model",
        "2. An illustrative comparison",
        "<table>",
    ] {
        assert!(
            html.as_str().contains(expected),
            "missing: {expected}\n{}",
            html.as_str()
        );
    }
    assert!(!html.as_str().contains("\\eqref"));
    assert!(!html.as_str().contains("\\label"));
}

#[test]
fn rejects_unknown_nodes_raw_content_and_incompatible_api_versions() {
    let ast: Value = serde_json::from_str(AST).unwrap();
    for node in [
        json!({"t":"RawBlock","c":["latex","\\input{private}"]}),
        json!({"t":"RawBlock","c":["html","<script>bad()</script>"]}),
        json!({"t":"FuturePandocNode","c":[]}),
    ] {
        let mut changed = ast.clone();
        changed["blocks"].as_array_mut().unwrap().push(node);
        assert!(import(&changed, AUX).is_err());
    }
    let mut changed = ast;
    changed["pandoc-api-version"] = json!([2]);
    assert!(import(&changed, AUX).is_err());
}

#[test]
fn requires_every_compiled_label_and_bibliography_entry_to_survive() {
    let ast: Value = serde_json::from_str(AST).unwrap();
    let extra = format!("{AUX}\n\\newlabel{{missing}}{{{{3}}{{1}}{{Missing}}{{section.3}}{{}}}}\n");
    assert!(
        import(&ast, &extra)
            .unwrap_err()
            .to_string()
            .contains("was not preserved")
    );
    let extra = format!("{AUX}\n\\bibcite{{missing}}{{2}}\n");
    assert!(
        import(&ast, &extra)
            .unwrap_err()
            .to_string()
            .contains("was not preserved")
    );
    assert!(import(&ast, "").is_err());
}

#[test]
fn rejects_table_spans_and_missing_citations_instead_of_flattening_them() {
    fn mutate(value: &mut Value, tag: &str, change: &mut impl FnMut(&mut Value)) {
        if value["t"] == tag {
            change(value);
            return;
        }
        match value {
            Value::Object(map) => {
                for value in map.values_mut() {
                    mutate(value, tag, change);
                }
            }
            Value::Array(items) => {
                for value in items {
                    mutate(value, tag, change);
                }
            }
            _ => {}
        }
    }
    let mut ast: Value = serde_json::from_str(AST).unwrap();
    mutate(&mut ast, "Table", &mut |node| {
        node["c"][3][1][0][1][0][2] = json!(2)
    });
    assert!(import(&ast, AUX).unwrap_err().to_string().contains("spans"));
    let mut ast: Value = serde_json::from_str(AST).unwrap();
    mutate(&mut ast, "Cite", &mut |node| {
        node["c"][0][0]["citationId"] = json!("missing")
    });
    assert!(
        import(&ast, AUX)
            .unwrap_err()
            .to_string()
            .contains("no compiled citation")
    );
}

#[test]
fn rejects_unsupported_source_features_before_compilation() {
    for (from, to) in [
        ("{article}", "{../class}"),
        ("{amsmath}", "{shellesc}"),
        ("\\maketitle", "\\input{private}"),
        ("\\label{eq:work}", ""),
        ("\\begin{table}", "\\begin{mystery}"),
        ("\\bibliographystyle{plain}", "\\bibliographystyle{unknown}"),
    ] {
        assert!(
            berlin_paper::validate_source(&SOURCE.replace(from, to), Some("references")).is_err(),
            "{from}"
        );
    }
    assert!(berlin_paper::validate_source(SOURCE, None).is_err());
    assert!(berlin_paper::validate_source(SOURCE, Some("different")).is_err());
}

#[test]
fn auxiliary_parser_never_executes_and_rejects_ambiguous_numbers() {
    for aux in [
        r"\newlabel{x}{{1}",
        r"\bibcite{x}{\input{private}}",
        "\\bibcite{x}{1}\n\\bibcite{x}{2}",
        r"\@input{other.aux}",
    ] {
        assert!(Labels::parse(aux).is_err());
    }
    assert!(Labels::parse(r"\write18{never executed}").is_ok());
}

#[test]
fn section_ids_cannot_collide_with_rendered_footnote_anchors() {
    let ast: Value = serde_json::from_str(&AST.replace("sec:model", "fn-1")).unwrap();
    let error = import(&ast, &AUX.replace("sec:model", "fn-1")).unwrap_err();
    assert!(error.to_string().contains("duplicate anchor 'fn-1'"));
}
