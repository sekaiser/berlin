use berlin_content::DocumentCollection;
use berlin_core::{ArtifactKind, FunctionRef, PipelineLoader};
use berlin_document::{ContentId, Document, DocumentKind, Metadata, Provenance, SourceFormat};
use berlin_pipeline_dsl::RhaiPipelineLoader;
use berlin_publication::*;

const COMPOSER: &str = r#"
fn portfolio(catalog) {
    let selected=catalog.select(#{topics_all:["systems"],order:"published_desc"});
    let entries=[];
    for source in selected { entries.push(view_component("berlin.featured_work@1",#{source:source},#{})); }
    let root=view_component("berlin.portfolio@1",#{},#{body:[view_component("berlin.introduction@1",#{text:"Selected work."},#{}),view_component("berlin.section@1",#{title:"Systems"},#{body:entries})]});
    publication(#{id:"systems",title:"Selected work"},root)
}
pipeline html {
    let documents=parse_markdown(load_markdown("missing/*.md"));
    let publication=compose_publication(documents,Fn("portfolio"),publication_options(#{source_site_url:"https://example.test/notebook"}));
    output render_publication_website(publication,"_portfolio",website_config(#{url:"https://example.test/portfolio"}));
}
pipeline text {
    let publication=compose_publication(parse_markdown(load_markdown("missing/*.md")),Fn("portfolio"),publication_options(#{source_site_url:"https://example.test/notebook"}));
    output render_publication_text(publication,"_text",text_config(#{profile:"plain"}));
}
"#;

#[test]
fn d07_composition_modules_cannot_bypass_explicit_program_inputs() {
    for source in [
        format!("import \"unselected\" as hidden; {COMPOSER}"),
        COMPOSER.replace(
            "fn portfolio(catalog) {",
            "fn portfolio(catalog) { import \"unselected\" as hidden;",
        ),
    ] {
        assert!(RhaiPipelineLoader::new(&source).is_err());
    }
}

#[test]
fn runtime_extensions_share_the_frozen_component_contract() {
    #[derive(Component)]
    #[component(key = "example.badge@1", channels(html, text))]
    pub struct Badge {
        pub text: String,
    }
    let mut registry = RegistryBuilder::default();
    registry.register::<Badge>().unwrap();
    let source = r#"fn compose(c){publication(#{id:"custom",title:"Custom"},view_component("example.badge@1",#{text:"native"},#{}))}
        pipeline text {output render_publication_text(compose_publication(parse_markdown(load_markdown("*.md")),Fn("compose"),publication_options(#{})),"_text",text_config(#{}));}"#;
    let loader = RhaiPipelineLoader::with_components(source, registry.finish()).unwrap();
    let assembly = loader
        .compose_publication(
            &FunctionRef::new("compose"),
            &DocumentCollection::default(),
            &PublicationOptions::default(),
        )
        .unwrap();
    assert_eq!(
        assembly.spec().root,
        Badge::builder().text("native").build()
    );
}
fn documents() -> DocumentCollection {
    DocumentCollection::new(vec![Document {
        id: ContentId("a".into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some("Mapped title".into()),
            tags: vec!["systems".into()],
            ..Default::default()
        },
        blocks: vec![],
        relations: vec![],
        provenance: Provenance {
            source: "file:///private/a.md".into(),
            source_format: SourceFormat::Markdown,
            source_hash: "a".repeat(64),
        },
    }])
}

#[test]
fn unified_d11_rust_and_rhai_queries_share_order_and_both_digests() {
    let external=ExternalResourceCollection::new(vec![(serde_json::from_value(serde_json::json!({
        "id":"repository","kind":"repository","url":"https://example.test/repository?x=1&y=2#Section",
        "title":"Repository <notes>","topics":["storage"]
    })).unwrap(),ResourceSource{namespace:"reading".into(),logical_name:"resources.yaml".into(),sha256:"b".repeat(64),record:1})],&ResourceLimits::default(),&PublicationLimits::default()).unwrap();
    let catalog =
        PublicationCatalog::with_resources(documents(), external, PublicationOptions::default())
            .unwrap();
    let selected=catalog.select(&serde_json::from_value::<ResourceQuery>(serde_json::json!({
        "topics_any":["Storage","SYSTEMS"],"origins_any":["external","local"],"order":"id_asc"
    })).unwrap()).unwrap();
    let source = r#"fn probe(catalog) {
        let selected=catalog.select(#{topics_any:["systems","storage"],origins_any:["local","external"],order:"id_asc"});
        let text=selected.query_digest+"|"+selected.selection_digest;
        for item in selected { text+="|"+item.id+"|"+catalog.describe(item).title; }
        publication(#{id:"probe",title:"Query parity"},view_component("berlin.introduction@1",#{text:text},#{}))
    }"#;
    let loader = RhaiPipelineLoader::new(source).unwrap();
    let result = loader
        .compose_catalog(&FunctionRef::new("probe"), catalog.clone())
        .unwrap();
    let mut expected = format!(
        "{}|{}",
        selected.query_digest(),
        selected.selection_digest()
    );
    for entry in selected.iter() {
        expected += &format!("|{}|{}", entry.descriptor().id, entry.descriptor().title);
    }
    assert_eq!(
        result.spec().root.properties["text"],
        Property::Text(expected)
    );
    assert_eq!(catalog.len(), 2);
    assert_eq!(catalog.index_builds(), 1);
}

#[test]
fn unified_queries_preflight_arrays_and_charge_cumulative_native_work() {
    let once = r#"fn probe(c) {
        let selected=c.select(#{});
        publication(#{id:"p",title:"P"},view_component("berlin.introduction@1",#{text:"OK"},#{}))
    }"#;
    let mut options = PublicationOptions::default();
    options.resource_limits.query_work = 1;
    let catalog = PublicationCatalog::new(documents(), options).unwrap();
    assert!(
        RhaiPipelineLoader::new(once)
            .unwrap()
            .compose_catalog(&FunctionRef::new("probe"), catalog.clone())
            .is_ok()
    );
    let twice = once.replace(
        "let selected=c.select(#{});",
        "let selected=c.select(#{}); let another=c.select(#{});",
    );
    assert!(
        RhaiPipelineLoader::new(&twice)
            .unwrap()
            .compose_catalog(&FunctionRef::new("probe"), catalog)
            .unwrap_err()
            .to_string()
            .contains("ResourceLimit")
    );
    let mut options = PublicationOptions::default();
    options.limits.occurrences = 2;
    let catalog = PublicationCatalog::new(documents(), options).unwrap();
    assert!(
        RhaiPipelineLoader::new(&twice)
            .unwrap()
            .compose_catalog(&FunctionRef::new("probe"), catalog.clone())
            .is_ok()
    );
    let triple = twice.replace(
        "let another=c.select(#{});",
        "let another=c.select(#{});let third=c.select(#{});",
    );
    assert!(
        RhaiPipelineLoader::new(&triple)
            .unwrap()
            .compose_catalog(&FunctionRef::new("probe"), catalog)
            .unwrap_err()
            .to_string()
            .contains("ResourceLimit")
    );
    let excessive=once.replace("let selected=c.select(#{});","let values=[];for i in 0..65 {values.push(\"storage\");}let selected=c.select(#{topics_any:values});");
    assert!(
        RhaiPipelineLoader::new(&excessive)
            .unwrap()
            .compose_catalog(
                &FunctionRef::new("probe"),
                PublicationCatalog::new(documents(), PublicationOptions::default()).unwrap()
            )
            .unwrap_err()
            .to_string()
            .contains("ResourceLimit")
    );
}
#[test]
fn d05_d08_rust_and_runtime_rhai_use_the_same_normalized_contract() {
    let loader = RhaiPipelineLoader::new(COMPOSER).unwrap();
    let options = PublicationOptions {
        source_site_url: Some("https://example.test/notebook".parse().unwrap()),
        ..Default::default()
    };
    let rhai = loader
        .compose_publication(&FunctionRef::new("portfolio"), &documents(), &options)
        .unwrap();
    let catalog = PublicationCatalog::new(documents(), options).unwrap();
    let rust = PublicationAssembly::resolve(
        &PublicationSpec {
            schema_version: 1,
            metadata: PublicationMetadata {
                id: PublicationId("systems".into()),
                title: "Selected work".into(),
                description: None,
            },
            root: Portfolio {}.into_call().with_body(ViewFragment(vec![
                Introduction {
                    text: "Selected work.".into(),
                }
                .into_call(),
                Section {
                    title: "Systems".into(),
                }
                .into_call()
                .with_body(
                    FeaturedWork {
                        source: catalog.work_ref("a").unwrap(),
                        annotation: None,
                    }
                    .into_call(),
                ),
            ])),
        },
        catalog,
        &ComponentRegistry::builtins(),
    )
    .unwrap();
    assert_eq!(rhai.spec(), rust.spec());
    assert_eq!(
        rhai.composition_fingerprint(),
        rust.composition_fingerprint()
    );
    assert_eq!(
        rhai.catalog()
            .entry("a")
            .unwrap()
            .document()
            .metadata
            .title
            .as_deref(),
        Some("Mapped title")
    );
    assert_eq!(
        loader
            .load("html")
            .unwrap()
            .nodes()
            .last()
            .unwrap()
            .operation
            .output_kind(),
        ArtifactKind::Website
    );
    assert_eq!(
        loader
            .load("text")
            .unwrap()
            .nodes()
            .last()
            .unwrap()
            .operation
            .output_kind(),
        ArtifactKind::PublicationText
    );
}
#[test]
fn d07_plan_checks_arity_without_loading_documents_or_executing_composers() {
    let source = COMPOSER.replace(
        "let selected=catalog.select(",
        "throw(\"must not run during planning\"); let selected=catalog.select(",
    );
    let loader = RhaiPipelineLoader::new(&source).unwrap();
    loader.load("html").unwrap().validate().unwrap();
    assert!(
        RhaiPipelineLoader::new(
            &COMPOSER.replace("fn portfolio(catalog)", "fn portfolio(catalog,other)")
        )
        .is_err()
    );
}
#[test]
fn d06_dynamic_types_and_unknown_fields_fail_the_shared_schema() {
    for replacement in ["#{text:42}", "#{text:\"hello\",unknown:true}"] {
        let source = COMPOSER.replace("#{text:\"Selected work.\"}", replacement);
        let loader = RhaiPipelineLoader::new(&source).unwrap();
        assert!(
            loader
                .compose_publication(
                    &FunctionRef::new("portfolio"),
                    &documents(),
                    &PublicationOptions {
                        source_site_url: Some("https://example.test".parse().unwrap()),
                        ..Default::default()
                    }
                )
                .is_err()
        );
    }
}
#[test]
fn d09_composer_rejects_curried_anonymous_and_wrong_results() {
    assert!(
        RhaiPipelineLoader::new(
            &COMPOSER.replace("Fn(\"portfolio\")", "Fn(\"portfolio\").curry(1)")
        )
        .is_err()
    );
    assert!(
        RhaiPipelineLoader::new(&COMPOSER.replace("Fn(\"portfolio\")", "|catalog| catalog"))
            .is_err()
    );
    let loader = RhaiPipelineLoader::new(&COMPOSER.replace(
        "publication(#{id:\"systems\",title:\"Selected work\"},root)",
        "\"wrong result\"",
    ))
    .unwrap();
    assert!(
        loader
            .compose_publication(
                &FunctionRef::new("portfolio"),
                &documents(),
                &PublicationOptions {
                    source_site_url: Some("https://example.test".parse().unwrap()),
                    ..Default::default()
                }
            )
            .is_err()
    );
}
#[test]
fn d10_native_fragment_multiplication_cannot_bypass_lowered_limits() {
    let source = r#"fn p(c) {let entry=view_component("berlin.introduction@1",#{text:"hello"},#{});let group=view_component("berlin.section@1",#{title:"Group"},#{body:[entry,entry,entry]});let root=view_component("berlin.portfolio@1",#{},#{body:[group,group,group]});publication(#{id:"p",title:"P"},root)} pipeline text {output render_publication_text(compose_publication(parse_markdown(load_markdown("*.md")),Fn("p"),publication_options(#{})),"_text",text_config(#{}));}"#;
    let loader = RhaiPipelineLoader::new(source).unwrap();
    let result = loader.compose_publication(
        &FunctionRef::new("p"),
        &documents(),
        &PublicationOptions {
            limits: PublicationLimits {
                occurrences: 8,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert!(result.is_err());
}

#[test]
fn d10_reused_large_children_remain_shared_until_bounded_lowering() {
    let source = r#"fn p(c) {
        let text="x";text.pad(100000,'x');
        let leaf=view_component("berlin.introduction@1",#{text:text},#{});
        let unused=[];
        for n in 0..100 { unused.push(view_component("berlin.section@1",#{title:"unused"},#{body:[leaf]})); }
        let root=view_component("berlin.portfolio@1",#{},#{body:[leaf]});
        publication(#{id:"p",title:"P"},root)
    } pipeline text {output render_publication_text(compose_publication(parse_markdown(load_markdown("*.md")),Fn("p"),publication_options(#{})),"_text",text_config(#{}));}"#;
    let loader = RhaiPipelineLoader::new(source).unwrap();
    let result = loader
        .compose_publication(
            &FunctionRef::new("p"),
            &documents(),
            &PublicationOptions::default(),
        )
        .unwrap();
    assert_eq!(result.placements().len(), 2);
}
