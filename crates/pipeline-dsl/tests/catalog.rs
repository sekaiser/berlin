use berlin_core::{ArtifactKind, Operation, PipelineLoader};
use berlin_pipeline_dsl::RhaiPipelineLoader;

const PROGRAM: &str =
    include_str!("../../../support/fixtures/unified-resources/berlin.pipeline.rhai");
#[test]
fn g01_g02_typed_variadic_catalog_and_one_composer_feed_both_channels() {
    let loader = RhaiPipelineLoader::new(PROGRAM).unwrap();
    for name in ["portfolio", "site", "text", "excerpts", "excerpt_text"] {
        let plan = loader.load(name).unwrap();
        plan.validate().unwrap();
        let catalog = plan
            .nodes()
            .iter()
            .find(|n| matches!(n.operation, Operation::AssembleCatalog { .. }))
            .unwrap();
        assert_eq!(
            catalog.operation.signature().inputs.as_ref(),
            [ArtifactKind::Documents, ArtifactKind::ExternalResources]
        );
        assert_eq!(catalog.dependencies.len(), 2);
        assert!(
            plan.nodes()
                .iter()
                .any(|n| matches!(n.operation, Operation::Compose { .. }))
        );
        if name == "site" {
            let html = plan
                .nodes()
                .iter()
                .find(|n| matches!(n.operation, Operation::RenderHtml { .. }))
                .unwrap();
            assert_eq!(
                html.operation.signature().inputs.as_ref(),
                [ArtifactKind::Composed, ArtifactKind::StaticAssets]
            );
        }
    }
}
#[test]
fn g03_g04_g05_bad_graph_types_arity_and_composers_fail_without_loading_sources() {
    let valid = r#"fn publication(c){throw "composer must not run during planning";}
      pipeline test {output render_html(compose(catalog([documents(load_markdown("missing/*.md"))]),Fn("publication")),"_site",html_config(#{url:"https://example.test"}));}"#;
    assert!(RhaiPipelineLoader::new(valid).unwrap().load("test").is_ok());
    for replacement in [
        valid.replace(
            "catalog([documents(load_markdown(\"missing/*.md\"))])",
            "catalog([])",
        ),
        valid.replace(
            "catalog([documents(load_markdown(\"missing/*.md\"))])",
            "catalog([load_markdown(\"missing/*.md\")])",
        ),
        valid.replace(
            "compose(catalog([documents(load_markdown(\"missing/*.md\"))]),Fn(\"publication\"))",
            "documents(load_markdown(\"missing/*.md\"))",
        ),
        valid.replace("Fn(\"publication\")", "Fn(\"unknown_composer\")"),
        valid.replace("fn publication(c)", "fn publication(c,extra)"),
        valid.replace("\"_site\"", "\"../escape\""),
    ] {
        assert!(
            RhaiPipelineLoader::new(&replacement).is_err(),
            "unexpectedly valid: {replacement}"
        );
    }
    let duplicate=valid.replace("catalog([documents(load_markdown(\"missing/*.md\"))])","catalog([documents(load_markdown(\"missing/*.md\")),documents(load_markdown(\"missing/*.md\"))])");
    assert!(
        RhaiPipelineLoader::new(&duplicate)
            .err()
            .unwrap()
            .to_string()
            .contains("duplicate dependency")
    );
}
#[test]
fn g06_identical_shared_nodes_are_deduplicated_but_changed_definitions_fail() {
    let source = r#"fn draft(c){throw "not executed";}
      pipeline test {
        let sources=load_markdown("missing/*.md");
        let docs=documents(sources);
        let a=map_documents(docs,Fn("first")).named("first_docs");
        let b=map_documents(docs,Fn("second")).named("second_docs");
        output render_text(compose(catalog([a,b]),Fn("draft")),"_text",text_config(#{}));
      }
      fn first(document){document}
      fn second(document){document}"#;
    let plan = RhaiPipelineLoader::new(source)
        .unwrap()
        .load("test")
        .unwrap();
    assert_eq!(
        plan.nodes()
            .iter()
            .filter(|n| matches!(n.operation, Operation::LoadMarkdown { .. }))
            .count(),
        1
    );
    let changed = source.replace(
        "let b=map_documents(docs,Fn(\"second\"))",
        "let b=map_documents(documents(load_markdown(\"different/*.md\")),Fn(\"second\"))",
    );
    assert!(RhaiPipelineLoader::new(&changed).is_err());
}
#[test]
fn g09_html_owns_its_product_and_declared_asset_mounts_are_confined() {
    let source = r#"fn draft(c){throw "not executed";}
      pipeline test {
        let composed=compose(catalog([documents(load_markdown("missing/*.md"))]),Fn("draft"));
        output render_html(composed,"bundle/site",html_config(#{url:"https://example.test"}));
        output render_text(composed,"bundle/text",text_config(#{}));
      }"#;
    assert!(RhaiPipelineLoader::new(source).is_ok());
    assert!(RhaiPipelineLoader::new(&source.replace("bundle/text", "bundle/site/text")).is_err());
    let source = PROGRAM.replace("\"attachments\");", "\"../escape\");");
    assert!(RhaiPipelineLoader::new(&source).is_err());
    let source=PROGRAM.replace(".with_assets(load_assets(\"attachments/**/*\"),\"attachments\")",".with_assets(load_assets(\"attachments/**/*\"),\"attachments\").with_assets(load_assets(\"attachments/**/*\"),\"another\")");
    assert!(RhaiPipelineLoader::new(&source).is_err());
}
#[test]
fn g11_removed_publishing_operations_are_not_compatibility_aliases() {
    for call in [
        "parse_feed(load_data(\"feed.csv\"))",
        "assemble_website(1,2)",
        "render_website(1,\"out\",1)",
        "render_linkedin(1,\"out\")",
        "compose_publication(1,Fn(\"p\"),1)",
        "render_publication_website(1,\"out\",1)",
        "render_publication_text(1,\"out\",1)",
        "parse_markdown(load_markdown(\"*.md\"))",
        "parse_documents(load_data(\"*.json\"))",
        "website_config(#{})",
    ] {
        let source = format!("fn p(c) {{ c }} let removed = {call};");
        assert!(berlin_pipeline_dsl::compile(&source).is_err(), "{call}");
    }
}

#[test]
fn native_graph_growth_has_a_budget_independent_of_rhai_instructions() {
    let source = r#"
        fn page(c) {throw "not executed";}
        let text="x"; text.pad(500000,'x');
        let config=html_config(#{description:text});
        let c=compose(catalog([documents(load_markdown("missing.md"))]),Fn("page"));
        let retained=[];
        for n in 0..1000 {retained.push(render_html(c,"out",config));}
    "#;
    let error = berlin_pipeline_dsl::compile(source)
        .unwrap_err()
        .to_string();
    assert!(error.contains("native graph byte budget"), "{error}");
}

#[test]
fn ambient_clock_and_process_inputs_are_unavailable() {
    for source in [
        "let clock=timestamp();",
        "let random=rand();",
        "let secret=read_file(\"private.txt\");",
        "let result=system(\"echo unsafe\");",
        "let result=eval(\"1+1\");",
        "let indirect=Fn(\"timestamp\").call();",
    ] {
        assert!(berlin_pipeline_dsl::compile(source).is_err(), "{source}");
    }
}
