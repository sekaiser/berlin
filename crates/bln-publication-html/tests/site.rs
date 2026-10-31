use berlin_content::DocumentCollection;
use berlin_document::*;
use berlin_publication::*;
use berlin_publication_html::*;
use std::collections::BTreeMap;

fn templates() -> BTreeMap<String, String> {
    fn visit(
        root: &std::path::Path,
        path: &std::path::Path,
        templates: &mut BTreeMap<String, String>,
    ) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, templates);
            } else if path.extension().is_some_and(|s| s == "tera") {
                templates.insert(
                    path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                    std::fs::read_to_string(path).unwrap(),
                );
            }
        }
    }
    let theme = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes/notebook");
    let mut sources = BTreeMap::new();
    for layer in ["defaults/pages", "shared/pages"] {
        let root = theme.join(layer);
        visit(&root, &root, &mut sources);
    }
    sources
}
fn doc(id: &str) -> Document {
    Document {
        id: ContentId(id.into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some(format!("Work {id}")),
            slug: Some(id.into()),
            tags: vec!["Storage".into()],
            ..Default::default()
        },
        blocks: vec![],
        relations: vec![],
        provenance: Provenance {
            source: format!("file:///private/sentinel/{id}.md"),
            source_hash: "a".repeat(64),
            source_format: SourceFormat::Markdown,
        },
    }
}
fn link(target: &str, anchor: &str) -> Block {
    Block::Paragraph {
        content: vec![Inline::DocumentLink(DocumentLink {
            target: ContentId(target.into()),
            anchor: ComponentId(anchor.into()),
            fragment: None,
            title: None,
            content: vec![Inline::Text {
                value: format!("Related {target}"),
            }],
        })],
    }
}
fn query(ids: &[&str]) -> ResourceQuery {
    ResourceQuery {
        ids: Some(ids.iter().map(|id| id.parse().unwrap()).collect()),
        order: ResourceOrder::Curated,
        ..Default::default()
    }
}
fn fixture() -> (SiteAssembly, PublicationSite, SiteHtmlOptions) {
    let mut a = doc("a");
    a.metadata.previous_slugs = vec!["old-a".into()];
    a.metadata.comments = true;
    a.blocks = vec![Block::Section {
        id: ComponentId("argument".into()),
        level: 2,
        role: None,
        title: vec![Inline::Text {
            value: "An argument".into(),
        }],
        blocks: vec![
            link("b", "link-b"),
            Block::Code(CodeBlock {
                language: Some("rust".into()),
                value: "let x = 1;\n".into(),
                references: CodeReferences {
                    id: Some("example".into()),
                    line_anchor_prefix: Some("example-line".into()),
                    first_line: 1,
                },
                caption: None,
                highlights: vec![],
            }),
            Block::Figure(Figure {
                source: "/attachments/figure.svg".into(),
                caption: Some("Figure caption".into()),
                alt: Some("An authored figure".into()),
            }),
        ],
    }];
    let mut b = doc("b");
    b.blocks = vec![Block::Paragraph {
        content: vec![Inline::Text {
            value: "UNSELECTED_BODY_SENTINEL".into(),
        }],
    }];
    let mut c = doc("c");
    c.blocks = vec![link("a", "link-a")];
    let mut draft = doc("draft");
    draft.metadata.draft = true;
    draft.metadata.title = Some("DRAFT_SENTINEL".into());
    let external=ExternalResourceCollection::new(vec![(serde_json::from_value(serde_json::json!({"id":"paper","kind":"paper","url":"https://example.test/paper?x=1&y=2#Part","title":"A paper & its evidence","topics":["storage"]})).unwrap(),ResourceSource{namespace:"reading".into(),logical_name:"resources.yaml".into(),sha256:"b".repeat(64),record:1})],&ResourceLimits::default(),&PublicationLimits::default()).unwrap();
    let catalog = PublicationCatalog::with_resources(
        DocumentCollection::new(vec![a, b, c, draft]),
        external,
        PublicationOptions {
            source_site_url: Some("https://example.test/notebook".parse().unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    let make_page = |id: &str, title: &str, resource: Option<&str>| {
        PublicationSpec::new(
            PublicationMetadata {
                id: PublicationId(id.into()),
                title: title.into(),
                description: None,
            },
            Portfolio::builder()
                .body(match resource {
                    Some(id) => ResourceCard::builder()
                        .resource(ResourceRef::new(id.parse().unwrap()))
                        .build(),
                    None => Introduction::builder()
                        .text("Authored editorial opening")
                        .build(),
                })
                .build(),
        )
        .unwrap()
    };
    let mut spec = SiteSpec::new(PublicationId("notebook".into()), "Engineering notebook");
    spec.add_page(
        "home",
        "index.html",
        make_page("home", "Home", Some("local:a")),
    );
    spec.add_page("about", "about.html", make_page("about", "About", None));
    spec.add_page(
        "reading",
        "feed.html",
        make_page("reading", "Reading", Some("external:paper")),
    );
    let excerpt = catalog.section_ref("a", "argument").unwrap();
    spec.add_page(
        "excerpts",
        "excerpts.html",
        PublicationSpec::new(
            PublicationMetadata {
                id: PublicationId("excerpts".into()),
                title: "Excerpts".into(),
                description: None,
            },
            view! {Portfolio(){
                Excerpt(source:excerpt.clone(),key:"first")
                Excerpt(source:excerpt,key:"second")
            }},
        )
        .unwrap(),
    );
    spec.add_document_pages(query(&["local:a", "local:c"]), "notes");
    spec.add_tag_pages(query(&["local:a", "external:paper"]));
    spec.add_search(query(&["local:a", "external:paper"]));
    spec.add_redirects(query(&["local:a"]));
    let assembly = SiteAssembly::resolve(&spec, catalog, &ComponentRegistry::builtins()).unwrap();
    let site = PublicationSite {
        url: "https://example.test/portfolio".into(),
        title: Some("Notebook".into()),
        author: Some("Synthetic author".into()),
    };
    let options = SiteHtmlOptions {
        discussion: Some(DiscussionSettings {
            repo: "example/discussions".into(),
            repo_id: "R_123".into(),
            category: "Articles".into(),
            category_id: "D_123".into(),
            theme: None,
        }),
        ..Default::default()
    };
    (assembly, site, options)
}
#[test]
fn f01_f02_f03_f04_f06_f07_f09_site_reuses_semantic_articles_without_mirroring_external_work() {
    let (assembly, site, options) = fixture();
    let renderer = PublicationHtmlRenderer::new(templates(), HtmlBindings::builtins()).unwrap();
    let result = renderer
        .render_site(&assembly, &ComponentRegistry::builtins(), &site, &options)
        .unwrap();
    let article = String::from_utf8(result.files["notes/a.html"].clone()).unwrap();
    let article = html_escape::decode_html_entities(&article);
    for text in [
        "study-page",
        "An argument",
        "example-line-1",
        "Figure caption",
        "https://example.test/notebook/attachments/figure.svg",
        "https://example.test/notebook/notes/b.html",
        "Referenced in",
        "Work c",
        "berlin:a",
        "discussion-load",
        "static/js/main.js",
        "static/js/study.js",
    ] {
        assert!(article.contains(text), "missing {text}: {article}");
    }
    let home = String::from_utf8(result.files["index.html"].clone()).unwrap();
    assert!(
        html_escape::decode_html_entities(&home)
            .contains("https://example.test/portfolio/notes/a.html")
    );
    let redirect = String::from_utf8(result.files["notes/old-a.html"].clone()).unwrap();
    assert!(
        html_escape::decode_html_entities(&redirect)
            .contains("https://example.test/portfolio/notes/a.html")
    );
    assert!(!result.files.contains_key("notes/paper.html"));
    assert!(!result.files.contains_key("notes/b.html"));
    for (path, bytes) in &result.files {
        let value = String::from_utf8_lossy(bytes);
        for sentinel in [
            "UNSELECTED_BODY_SENTINEL",
            "DRAFT_SENTINEL",
            "/private/sentinel",
        ] {
            assert!(!value.contains(sentinel), "{path} leaked {sentinel}");
        }
    }
    let index: serde_json::Value =
        serde_json::from_slice(&result.files["search/index.json"]).unwrap();
    assert_eq!(index["documents"].as_array().unwrap().len(), 2);
    assert_eq!(index["documents"][0]["path"], "notes/a.html");
    assert_eq!(
        index["documents"][1]["path"],
        "https://example.test/paper?x=1&y=2#Part"
    );
    let reading = String::from_utf8(result.files["feed.html"].clone()).unwrap();
    assert!(reading.contains("A paper &amp; its evidence"));
    assert!(result.stylesheets.contains("article.css"));
    assert!(result.stylesheets.contains("portfolio.css"));
    assert!(result.stylesheets.contains("comments.css"));
    let excerpts = String::from_utf8(result.files["excerpts.html"].clone()).unwrap();
    assert_eq!(excerpts.matches("example-line-1\"").count(), 2);
    assert!(!excerpts.contains("id=\"example-line-1\""));
    let again = renderer
        .render_site(&assembly, &ComponentRegistry::builtins(), &site, &options)
        .unwrap();
    assert_eq!(result.files, again.files);
    assert_eq!(
        result.manifest.render_input_fingerprint,
        again.manifest.render_input_fingerprint
    );
}

#[test]
fn f10_one_page_site_does_not_invent_routes_or_navigation_to_originals() {
    let (original, mut site, options) = fixture();
    site.url = "https://example.test/".into();
    let mut spec = SiteSpec::new(PublicationId("single".into()), "One page");
    spec.add_page(
        "home",
        "index.html",
        PublicationSpec::new(
            PublicationMetadata {
                id: PublicationId("home".into()),
                title: "Home".into(),
                description: None,
            },
            view! {Portfolio(){ResourceCard(resource:ResourceRef::local("a").unwrap())}},
        )
        .unwrap(),
    );
    let assembly = SiteAssembly::resolve(
        &spec,
        original.catalog().clone(),
        &ComponentRegistry::builtins(),
    )
    .unwrap();
    let result = PublicationHtmlRenderer::new(templates(), HtmlBindings::builtins())
        .unwrap()
        .render_site(&assembly, &ComponentRegistry::builtins(), &site, &options)
        .unwrap();
    assert_eq!(
        result.files.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["index.html"]
    );
    let html = String::from_utf8(result.files["index.html"].clone()).unwrap();
    let html = html_escape::decode_html_entities(&html);
    assert!(html.contains("https://example.test/notebook/notes/a.html"));
    assert!(!html.contains("href=\"https://example.test/feed.html\""));
    assert!(!html.contains("href=\"https://example.test/about.html\""));
}
#[test]
fn comments_require_explicit_configuration_and_profiles_are_not_unsafe_links() {
    let (assembly, site, options) = fixture();
    let renderer = PublicationHtmlRenderer::new(templates(), HtmlBindings::builtins()).unwrap();
    assert!(
        renderer
            .render_site(
                &assembly,
                &ComponentRegistry::builtins(),
                &site,
                &Default::default()
            )
            .is_err()
    );
    let mut invalid = options;
    invalid.github = Some("javascript:alert(1)".into());
    assert!(
        renderer
            .render_site(&assembly, &ComponentRegistry::builtins(), &site, &invalid)
            .is_err()
    );
}
