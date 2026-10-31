use berlin_content::DocumentCollection;
use berlin_core::{ArtifactKind, FunctionRef, PipelineLoader};
use berlin_document::{ContentId, Document, DocumentKind, Metadata, Provenance, SourceFormat};
use berlin_pipeline_dsl::RhaiPipelineLoader;
use berlin_publication::*;

const COMPOSER: &str = r#"
fn portfolio(catalog) {
    let selected=select_work(catalog,#{tags_all:["systems"],order:"published_desc"});
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
        "let selected=select_work(catalog,",
        "throw(\"must not run during planning\"); let selected=select_work(catalog,",
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
