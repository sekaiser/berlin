use berlin_content::DocumentCollection;
use berlin_document::*;
use berlin_publication::*;
use std::collections::BTreeMap;

fn paragraph(content: Vec<Inline>) -> Block {
    Block::Paragraph { content }
}
fn source(blocks: Vec<Block>) -> Document {
    Document {
        id: ContentId("source".into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some("Original work".into()),
            ..Default::default()
        },
        blocks,
        relations: Vec::new(),
        provenance: Provenance {
            source: "file:///private/source.org".into(),
            source_format: SourceFormat::Org,
            source_hash: "a".repeat(64),
        },
    }
}
fn section(blocks: Vec<Block>) -> Block {
    Block::Section {
        id: ComponentId("selected".into()),
        level: 2,
        role: None,
        title: vec![Inline::Text {
            value: "Selected section".into(),
        }],
        blocks,
    }
}
fn prepare(document: Document) -> Result<PublicationAssembly> {
    let catalog = PublicationCatalog::new(
        DocumentCollection::new(vec![document]),
        PublicationOptions {
            source_site_url: Some("https://original.example/notebook".parse().unwrap()),
            ..Default::default()
        },
    )?;
    let excerpt = Excerpt {
        source: catalog.section_ref("source", "selected")?,
        annotation: None,
    }
    .into_call();
    let spec = PublicationSpec {
        schema_version: 1,
        queries: Default::default(),
        metadata: PublicationMetadata {
            id: PublicationId("p".into()),
            title: "Portfolio".into(),
            description: None,
        },
        root: Portfolio {}.into_call().with_body(ViewFragment(vec![
            excerpt.clone().keyed("first"),
            excerpt.keyed("second"),
        ])),
    };
    PublicationAssembly::resolve(&spec, catalog, &ComponentRegistry::builtins())
}
fn reference() -> ExcerptRef {
    ExcerptRef {
        source: ContentRef::new("source"),
        section: Some("selected".into()),
    }
}

#[test]
fn whole_document_selection_preserves_opening_and_sections_in_one_shared_fragment() {
    let doc = source(vec![
        paragraph(vec![Inline::Text {
            value: "Opening before any section.".into(),
        }]),
        section(vec![paragraph(vec![Inline::Text {
            value: "Section body.".into(),
        }])]),
    ]);
    let catalog = PublicationCatalog::new(
        DocumentCollection::new(vec![doc]),
        PublicationOptions {
            source_site_url: Some("https://example.test".parse().unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    let whole = catalog.document_excerpt("source").unwrap();
    assert_eq!(whole.section, None);
    let spec = PublicationSpec::new(
        PublicationMetadata {
            id: PublicationId("whole".into()),
            title: "Whole article".into(),
            description: None,
        },
        view! {Portfolio(){
            Excerpt(source:whole.clone(),key:"first")
            Excerpt(source:whole.clone(),key:"second")
        }},
    )
    .unwrap();
    let assembly =
        PublicationAssembly::resolve(&spec, catalog, &ComponentRegistry::builtins()).unwrap();
    let resolved = assembly.excerpt(&whole).unwrap();
    assert_eq!(resolved.blocks().len(), 2);
    assert!(std::ptr::eq(resolved, assembly.excerpt(&whole).unwrap()));
    assert!(assembly.catalog().section_ref("source", "").is_err());
}

#[test]
fn f17_excerpt_budget_counts_inline_nodes_before_copying() {
    let doc = source(vec![section(vec![paragraph(vec![
        Inline::Text {
            value: "one".into(),
        },
        Inline::Text {
            value: "two".into(),
        },
    ])])]);
    let resolve = |maximum| {
        let catalog = PublicationCatalog::new(
            DocumentCollection::new(vec![doc.clone()]),
            PublicationOptions {
                source_site_url: Some("https://example.test".parse().unwrap()),
                limits: PublicationLimits {
                    excerpt_nodes: maximum,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
        PublicationAssembly::resolve(
            &PublicationSpec {
                schema_version: 1,
                queries: Default::default(),
                metadata: PublicationMetadata {
                    id: PublicationId("p".into()),
                    title: "P".into(),
                    description: None,
                },
                root: Portfolio::builder()
                    .body(Excerpt::builder().source(reference()).build())
                    .build(),
            },
            catalog,
            &ComponentRegistry::builtins(),
        )
    };
    assert!(resolve(5).is_ok());
    assert_eq!(resolve(4).unwrap_err().code, Code::ResourceLimit);
}

#[test]
fn b03_b08_b12_selection_includes_section_and_footnote_closure_without_source_mutation() {
    let original = source(vec![
        section(vec![paragraph(vec![Inline::FootnoteReference {
            name: "note".into(),
        }])]),
        Block::FootnoteDefinition {
            name: "note".into(),
            blocks: vec![paragraph(vec![Inline::Text {
                value: "Outside definition".into(),
            }])],
        },
    ]);
    let before = canonical_bytes(&original).unwrap();
    let assembly = prepare(original.clone()).unwrap();
    let excerpt = assembly.excerpt(&reference()).unwrap();
    assert_eq!(excerpt.blocks().len(), 2);
    assert_eq!(excerpt.dependency_names, ["note"]);
    assert!(excerpt.local_anchors.contains("selected"));
    assert!(excerpt.local_anchors.contains("fn-note"));
    assert_eq!(
        before,
        canonical_bytes(assembly.catalog().entry("source").unwrap().document()).unwrap()
    );
    assert!(std::ptr::eq(
        excerpt,
        assembly.excerpt(&reference()).unwrap()
    ));
}

#[test]
fn b04_b08_missing_selectors_and_missing_or_cyclic_footnotes_fail() {
    assert_eq!(
        prepare(source(Vec::new())).unwrap_err().code,
        Code::MissingExcerpt
    );
    let missing = prepare(source(vec![section(vec![paragraph(vec![
        Inline::FootnoteReference {
            name: "missing".into(),
        },
    ])])]))
    .unwrap_err();
    assert_eq!(missing.code, Code::UnresolvedReference);
    assert_eq!(missing.source_id.as_deref(), Some("source"));
    assert_eq!(missing.selector.as_deref(), Some("selected"));
    let cyclic = source(vec![
        section(vec![paragraph(vec![Inline::FootnoteReference {
            name: "a".into(),
        }])]),
        Block::FootnoteDefinition {
            name: "a".into(),
            blocks: vec![paragraph(vec![Inline::FootnoteReference {
                name: "b".into(),
            }])],
        },
        Block::FootnoteDefinition {
            name: "b".into(),
            blocks: vec![paragraph(vec![Inline::FootnoteReference {
                name: "a".into(),
            }])],
        },
    ]);
    assert_eq!(prepare(cyclic).unwrap_err().code, Code::UnresolvedReference);
}

#[test]
fn b10_assets_keep_the_source_mount_and_reject_private_locations() {
    let assembly = prepare(source(vec![section(vec![Block::Figure(Figure {
        source: "/attachments/figure.svg".into(),
        caption: Some("A figure".into()),
        alt: Some("Example".into()),
    })])]))
    .unwrap();
    let excerpt = assembly.excerpt(&reference()).unwrap();
    assert_eq!(
        excerpt
            .resolve_url("/attachments/figure.svg")
            .unwrap()
            .as_str(),
        "https://original.example/notebook/attachments/figure.svg"
    );
    assert_eq!(
        excerpt
            .resolve_url("../attachments/figure.svg")
            .unwrap()
            .as_str(),
        "https://original.example/notebook/attachments/figure.svg"
    );
    for path in [
        "file:///private/file",
        "@data/attachments/file",
        "../../private/file",
        "javascript:alert(1)",
    ] {
        assert_eq!(excerpt.resolve_url(path).unwrap_err().code, Code::UnsafeUrl);
    }
}

#[test]
fn b11_portable_excerpts_reject_raw_html_and_unregistered_components() {
    for block in [
        Block::Html {
            value: "<p>Raw</p>".into(),
        },
        paragraph(vec![Inline::Html {
            value: "<i>Raw</i>".into(),
        }]),
        Block::Component {
            id: ComponentId("custom".into()),
            name: "legacy".into(),
            properties: BTreeMap::new(),
            blocks: vec![Block::ThematicBreak],
        },
    ] {
        assert!(matches!(
            prepare(source(vec![section(vec![block])]))
                .unwrap_err()
                .code,
            Code::UnsupportedChannelFeature | Code::UnsupportedChannelComponent
        ));
    }
    let valid = source(vec![section(vec![Block::Component {
        id: ComponentId("callout".into()),
        name: "berlin.callout@1".into(),
        properties: BTreeMap::from([("kind".into(), PropertyValue::Text("note".into()))]),
        blocks: vec![paragraph(vec![Inline::Text {
            value: "A note".into(),
        }])],
    }])]);
    prepare(valid).unwrap();
}
