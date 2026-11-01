//! Behavioral tests for the public pipeline DSL contract.

use berlin_content::DocumentCollection;
use berlin_core::FunctionRef;
use berlin_core::PipelineLoader;
use berlin_document::Block;
use berlin_document::Document;
use berlin_document::Inline;
use berlin_pipeline_dsl::CompileError;
use berlin_pipeline_dsl::RhaiPipelineLoader;
use berlin_pipeline_dsl::compile;

const SCRIPT: &str = include_str!("../../../support/fixtures/publishing/berlin.pipeline.rhai");

#[test]
fn paper_compilation_is_typed_and_validates_public_pdf_paths() {
    let script = r#"fn draft(c) {throw "not executed";} pipeline site {
        let papers = compile_papers(load_latex("papers/*.tex"), "_site/papers");
        output render_text(compose(catalog([papers]),Fn("draft")), "_site/drafts",text_config(#{}));
    }"#;
    let plans = compile(script).unwrap();
    assert!(plans["site"].nodes().iter().any(
        |node| matches!(&node.operation, berlin_core::Operation::CompilePapers { url_prefix }
            if url_prefix == "/papers")
            && node.operation.signature().produces_value
            && node.operation.signature().writes_output
    ));
    assert!(compile(&script.replace("load_latex", "load_markdown")).is_err());
    for prefix in [
        "/",
        "//host",
        "/../secret",
        "relative",
        "/a?b",
        "/a/",
        "/%2e",
    ] {
        let script = script.replace(
            "\"_site/papers\")",
            &format!("\"_site/papers\", {prefix:?})"),
        );
        assert!(compile(&script).is_err(), "accepted {prefix}");
    }
}

#[test]
fn deployment_is_typed_metadata_not_an_executed_operation() {
    let source = SCRIPT.replace(
        ".with_assets(",
        ".deploy_to(github_pages(\"owner/notebook\")).with_assets(",
    );
    assert_ne!(source, SCRIPT, "fixture must contain an HTML asset mount");
    let plans = compile(&source).unwrap();
    assert_eq!(plans["site"].nodes().len(), 8);
    let target = plans["site"]
        .nodes()
        .iter()
        .find_map(|node| node.deployment.as_ref())
        .unwrap();
    assert_eq!(
        target,
        &berlin_core::DeploymentTarget::GitHubPages {
            repository: "owner/notebook".into()
        }
    );
    for repo in [
        "../outside",
        "https://github.com/owner/site",
        "owner/repo.git",
        "owner/repo/extra",
        "owner/--bad;command",
    ] {
        assert!(compile(&format!("let target = github_pages({repo:?});")).is_err());
    }
    assert!(compile(r#"pipeline bad { output load_markdown("*.md").deploy_to(github_pages("owner/repo")); }"#).is_err());
    assert!(
        compile(&source.replace(
            ".deploy_to(github_pages(\"owner/notebook\"))",
            ".deploy_to(github_pages(\"owner/notebook\")).deploy_to(github_pages(\"other/repo\"))"
        ))
        .is_err()
    );
}

#[test]
fn source_roots_are_explicit_typed_configuration_not_path_traversal() {
    let loader = RhaiPipelineLoader::new("const SOURCE_ROOTS = #{data: \"../data\"};").unwrap();
    assert_eq!(loader.source_roots()["data"], "../data");
    for invalid in [
        "42",
        "#{data: 1}",
        "#{data: \"\"}",
        "#{\"../data\": \"../data\"}",
    ] {
        assert!(RhaiPipelineLoader::new(&format!("const SOURCE_ROOTS = {invalid};")).is_err());
    }
    assert!(
        compile(r#"pipeline bad { output copy_assets(load_assets("../data/*"), "out"); }"#)
            .is_err()
    );
    assert!(compile(r#"const SOURCE_ROOTS = #{data: "../data"}; pipeline bad { output copy_assets(load_assets("@data/*"), "../outside"); }"#).is_err());
}

#[test]
fn compiles_lisp_inspired_pipeline_forms() {
    let pipelines = compile(SCRIPT).unwrap();

    assert_eq!(pipelines["org"].nodes().len(), 2);
    assert_eq!(pipelines["site"].nodes().len(), 8);
    assert_eq!(pipelines["linkedin"].nodes().len(), 7);
    assert_eq!(pipelines["site"].validate(), Ok(()));
    let export = &pipelines["org"].nodes()[1];
    assert_eq!(
        export.output_path.as_deref(),
        Some(std::path::Path::new(".berlin/generated/org"))
    );
    assert!(matches!(&export.operation,
        berlin_core::Operation::ExportOrg { backend, section }
        if backend == "ox-hugo" && section == "notes"));
}

#[test]
fn website_settings_are_typed_and_scoped_to_render_nodes() {
    let plans = compile(r#"
        fn notebook(c) {throw "not executed";}
        pipeline first {
            let website = compose(catalog([documents(load_markdown("*.md"))]),Fn("notebook"));
            output render_html(website, "_first", html_config(#{title: "First", theme: "../themes/notebook", url: "https://first.example", external_links_new_tab: true, profiles: #{github: "https://github.com/first"}}));
        }
        pipeline second {
            let website = compose(catalog([documents(load_markdown("*.md"))]),Fn("notebook"));
            output render_html(website, "_second", html_config(#{title: "Second"}));
        }
        pipeline social {
            output render_text(compose(catalog([documents(load_markdown("*.md"))]),Fn("notebook")), "_social",text_config(#{}));
        }
    "#).unwrap();
    for (name, title) in [("first", "First"), ("second", "Second")] {
        let config = plans[name]
            .nodes()
            .iter()
            .find_map(|node| match &node.operation {
                berlin_core::Operation::RenderHtml { config, .. } => Some(config),
                _ => None,
            })
            .unwrap();
        assert_eq!(config.title.as_deref(), Some(title));
        assert_eq!(config.external_links_new_tab, name == "first");
        if name == "first" {
            assert_eq!(
                config.theme.as_deref(),
                Some(std::path::Path::new("../themes/notebook"))
            );
            assert_eq!(
                config.url.as_ref().unwrap().host_str(),
                Some("first.example")
            );
            assert_eq!(
                config.profiles.github.as_ref().unwrap().as_str(),
                "https://github.com/first"
            );
        } else {
            assert!(config.theme.is_none());
            assert!(config.url.is_none());
            assert!(config.profiles.github.is_none());
        }
    }
    assert!(
        !plans["social"]
            .nodes()
            .iter()
            .any(|node| matches!(node.operation, berlin_core::Operation::RenderHtml { .. }))
    );
}

#[test]
fn website_settings_reject_unknown_fields_wrong_types_and_invalid_urls() {
    for settings in [
        "#{titel: \"Typo\"}",
        "#{title: 42}",
        "#{theme: 42}",
        "#{external_links_new_tab: \"true\"}",
        "#{url: \"not an absolute URL\"}",
        "#{profiles: #{gitub: \"https://github.com/example\"}}",
        "#{profiles: #{github: \"not a URL\"}}",
        "#{giscus: #{repo: \"owner/repo\"}}",
        "#{giscus: true}",
    ] {
        let source = format!("let settings = html_config({settings});");
        assert!(compile(&source).is_err(), "accepted {settings}");
    }
    assert!(compile("let settings = html_config(#{});").is_ok());
    assert!(
        compile(
            r#"
        let settings = html_config(#{giscus: #{
            repo: "owner/notebook", repo_id: "R_TEST",
            category: "Article comments", category_id: "DIC_TEST"
        }});
    "#
        )
        .is_ok()
    );
}

#[test]
fn rejects_invalid_artifact_composition() {
    let error = compile(
        r#"
            pipeline invalid {
                output documents(load_assets("assets/*"));
            }
        "#,
    )
    .unwrap_err();

    assert!(error.to_string().contains("documents"));
    assert!(error.to_string().contains("StaticAssets"));
}

#[test]
fn rejects_conflicting_implicit_node_names() {
    let error = compile(
        r#"
            pipeline invalid {
                output load_markdown("content/one/*.md");
                output load_markdown("content/two/*.md");
            }
        "#,
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("node 'markdown_sources' is defined twice"),
        "{error}"
    );
}

#[test]
fn rejects_missing_document_mapper_before_execution() {
    let error = compile(
        r#"
            fn notebook(c) {throw "not executed";}
            pipeline invalid {
                let documents = documents(load_markdown("content/*.md"));
                let mapped = map_documents(documents, Fn("missing"));
                output render_html(compose(catalog([mapped]),Fn("notebook")), "_site", html_config(#{}));
            }
        "#,
    )
    .unwrap_err();

    assert!(matches!(error, CompileError::UnknownMapper(name) if name == "missing"));
}

#[test]
fn loader_selects_named_pipeline() {
    let loader = RhaiPipelineLoader::new(SCRIPT).unwrap();

    assert_eq!(loader.load("org").unwrap().nodes().len(), 2);
    assert!(matches!(
        loader.load("missing"),
        Err(CompileError::UnknownPipeline(name)) if name == "missing"
    ));
}

#[test]
fn explicit_names_allow_repeated_operation_types() {
    let pipelines = compile(
        r#"
            pipeline valid {
                output load_markdown("content/one/*.md").named("one");
                output load_markdown("content/two/*.md").named("two");
            }
        "#,
    )
    .unwrap();

    assert_eq!(pipelines["valid"].nodes().len(), 2);
    assert_eq!(pipelines["valid"].validate(), Ok(()));
}

#[test]
fn rejects_output_outside_pipeline() {
    let error = compile(r#"output load_markdown("content/*.md");"#).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("output can only be used inside a pipeline")
    );
}

#[test]
fn maps_documents_and_nested_sections_with_script_functions() {
    let loader = RhaiPipelineLoader::new(
        r#"
            fn rewrite_section(section) {
                section.role = "lead";
                section.title = section.title + "!";
                section
            }

            fn rewrite_document(document) {
                document.title = "Mapped title";
                document.add_tag("rhai");
                map_sections(document, Fn("rewrite_section"))
            }
        "#,
    )
    .unwrap();
    let documents = DocumentCollection::new(vec![Document {
        id: berlin_document::ContentId("article".into()),
        kind: berlin_document::DocumentKind::Article,
        metadata: berlin_document::Metadata::default(),
        blocks: vec![Block::Component {
            id: berlin_document::ComponentId("body".into()),
            name: "body".into(),
            properties: std::collections::BTreeMap::new(),
            blocks: vec![Block::Section {
                id: berlin_document::ComponentId("intro".into()),
                level: 2,
                role: None,
                title: vec![Inline::Text {
                    value: "Intro".into(),
                }],
                blocks: Vec::new(),
            }],
        }],
        relations: Vec::new(),
        provenance: berlin_document::Provenance {
            source: "file:///article.md".into(),
            source_format: berlin_document::SourceFormat::Markdown,
            source_hash: "0".repeat(64),
        },
    }]);

    let mapped = loader
        .map_documents(&FunctionRef::new("rewrite_document"), &documents)
        .unwrap();
    let document = &mapped.as_slice()[0];

    assert_eq!(document.id.0, "article");
    assert_eq!(document.metadata.title.as_deref(), Some("Mapped title"));
    assert_eq!(document.metadata.tags, ["rhai"]);
    let Block::Component { blocks, .. } = &document.blocks[0] else {
        panic!("expected component")
    };
    let Block::Section {
        id, role, title, ..
    } = &blocks[0]
    else {
        panic!("expected section")
    };
    assert_eq!(id.0, "intro");
    assert_eq!(role.as_deref(), Some("lead"));
    assert!(matches!(
        title.as_slice(),
        [Inline::Text { value }] if value == "Intro!"
    ));
}
