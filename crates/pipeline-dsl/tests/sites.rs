use berlin_content::DocumentCollection;
use berlin_core::FunctionRef;
use berlin_document::*;
use berlin_pipeline_dsl::RhaiPipelineLoader;
use berlin_publication::*;

fn catalog() -> PublicationCatalog {
    PublicationCatalog::new(
        DocumentCollection::new(vec![Document {
            id: ContentId("a".into()),
            kind: DocumentKind::Article,
            metadata: Metadata {
                title: Some("Work a".into()),
                ..Default::default()
            },
            blocks: vec![],
            relations: vec![],
            provenance: Provenance {
                source: "file:///private/a.md".into(),
                source_hash: "a".repeat(64),
                source_format: SourceFormat::Markdown,
            },
        }]),
        PublicationOptions {
            source_site_url: Some("https://example.test/notebook".parse().unwrap()),
            ..Default::default()
        },
    )
    .unwrap()
}
const SOURCE: &str = r#"
fn notebook(catalog) {
    let authored=catalog.select(#{origins_any:["local"],order:"published_desc"});
    let intro=view_component("berlin.introduction@1",#{text:"Editorial opening"},#{});
    let root=view_component("berlin.portfolio@1",#{},#{body:[intro]});
    let page=publication(#{id:"home",title:"Home"},root);
    let result=site("notebook",#{title:"Notebook"});
    result.add_page("home","index.html",page);
    result.add_document_pages(authored,"notes");
    result.add_tag_pages(authored);
    result.add_search(authored);
    result.add_redirects(authored);
    result
}
"#;
#[test]
fn site_builders_are_runtime_only_and_rust_rhai_share_routes_and_fingerprints() {
    let catalog = catalog();
    let result = RhaiPipelineLoader::new(SOURCE)
        .unwrap()
        .compose_artifact(&FunctionRef::new("notebook"), catalog.clone())
        .unwrap();
    let ComposedArtifact::Site(dynamic) = result else {
        panic!("expected site")
    };
    let mut spec = SiteSpec::new(PublicationId("notebook".into()), "Notebook");
    spec.add_page(
        "home",
        "index.html",
        PublicationSpec::new(
            PublicationMetadata {
                id: PublicationId("home".into()),
                title: "Home".into(),
                description: None,
            },
            view! {Portfolio(){Introduction(text:"Editorial opening")}},
        )
        .unwrap(),
    );
    let selected = catalog
        .select(&ResourceQuery {
            origins_any: Some(vec![ResourceOrigin::Local]),
            ..Default::default()
        })
        .unwrap();
    spec.add_document_pages(selected.query().clone(), "notes");
    spec.add_tag_pages(selected.query().clone());
    spec.add_search(selected.query().clone());
    spec.add_redirects(selected.query().clone());
    let typed = SiteAssembly::resolve(&spec, catalog, &ComponentRegistry::builtins()).unwrap();
    assert_eq!(typed.spec(), dynamic.spec());
    assert_eq!(
        typed.composition_fingerprint(),
        dynamic.composition_fingerprint()
    );
    assert_eq!(
        typed.routes().keys().collect::<Vec<_>>(),
        dynamic.routes().keys().collect::<Vec<_>>()
    );
    assert!(
        RhaiPipelineLoader::new("let s=site(\"bad\",#{title:\"Forbidden at graph time\"});")
            .is_err()
    );
}
#[test]
fn site_builders_reject_bad_paths_and_cumulative_native_growth() {
    let source = SOURCE.replace("\"index.html\"", "\"../escape.html\"");
    assert!(
        RhaiPipelineLoader::new(&source)
            .unwrap()
            .compose_artifact(&FunctionRef::new("notebook"), catalog())
            .is_err()
    );
    let source = SOURCE.replace(
        "result.add_page(\"home\",\"index.html\",page);",
        "for n in 0..10001 { result.add_page(n.to_string(),\"index.html\",page); }",
    );
    let error = RhaiPipelineLoader::new(&source)
        .unwrap()
        .compose_artifact(&FunctionRef::new("notebook"), catalog())
        .unwrap_err();
    assert!(
        error.to_string().contains("limit") || error.to_string().contains("operations"),
        "{error}"
    );
}

#[test]
fn native_publication_lowering_cannot_be_repeated_outside_the_logical_budget() {
    let source = r#"fn repeated(c) {
        let root=view_component("berlin.introduction@1",#{text:"a small reusable fragment"},#{});
        let copies=[];
        for n in 0..100 {copies.push(publication(#{id:n.to_string(),title:"Repeated native value"},root));}
        copies[0]
    }"#;
    let mut options = PublicationOptions::default();
    options.limits.property_bytes = 512;
    let small = PublicationCatalog::new(DocumentCollection::default(), options).unwrap();
    let error = RhaiPipelineLoader::new(source)
        .unwrap()
        .compose_artifact(&FunctionRef::new("repeated"), small)
        .unwrap_err();
    assert!(error.to_string().contains("lowering budget"), "{error}");
}
