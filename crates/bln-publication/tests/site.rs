use berlin_content::DocumentCollection;
use berlin_document::*;
use berlin_publication::*;

fn doc(id: &str) -> Document {
    Document {
        id: ContentId(id.into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some(format!("Work {id}")),
            slug: Some(format!("work-{id}")),
            tags: vec!["Storage".into()],
            ..Default::default()
        },
        blocks: vec![],
        relations: vec![],
        provenance: Provenance {
            source: format!("file:///private/{id}.org"),
            source_hash: "a".repeat(64),
            source_format: SourceFormat::Org,
        },
    }
}
fn catalog(docs: Vec<Document>) -> PublicationCatalog {
    PublicationCatalog::new(
        DocumentCollection::new(docs),
        PublicationOptions {
            source_site_url: Some("https://example.test/original/repository".parse().unwrap()),
            ..Default::default()
        },
    )
    .unwrap()
}
fn query(ids: &[&str]) -> ResourceQuery {
    ResourceQuery {
        ids: Some(
            ids.iter()
                .map(|id| ResourceId::local(id).unwrap())
                .collect(),
        ),
        order: ResourceOrder::Curated,
        ..Default::default()
    }
}
fn publication(id: &str) -> PublicationSpec {
    PublicationSpec::new(
        PublicationMetadata {
            id: PublicationId(id.into()),
            title: id.into(),
            description: None,
        },
        view! {Portfolio(){Introduction(text:"Editorial opening")}},
    )
    .unwrap()
}
fn notebook() -> SiteSpec {
    let mut spec = SiteSpec::new(PublicationId("notebook".into()), "Notebook");
    spec.add_page("home", "index.html", publication("home"));
    spec.add_page("about", "about.html", publication("about"));
    spec.add_page("reading", "feed.html", publication("reading"));
    spec.add_document_pages(query(&["a"]), "notes");
    spec.add_tag_pages(query(&["a"]));
    spec.add_search(query(&["a"]));
    spec.add_redirects(query(&["a"]));
    spec
}
#[test]
fn f01_f04_one_route_registry_preserves_explicit_pages_and_generated_intentions() {
    let mut a = doc("a");
    a.metadata.previous_slugs = vec!["old-work".into()];
    let mut hidden = doc("draft");
    hidden.metadata.draft = true;
    let c = catalog(vec![a, doc("unselected"), hidden]);
    let site =
        SiteAssembly::resolve(&notebook(), c.clone(), &ComponentRegistry::builtins()).unwrap();
    assert_eq!(
        site.routes().keys().map(String::as_str).collect::<Vec<_>>(),
        vec![
            "about.html",
            "feed.html",
            "index.html",
            "notes/old-work.html",
            "notes/work-a.html",
            "search.html",
            "search/client.js",
            "search/index.json",
            "tags/storage.html"
        ]
    );
    assert!(!site.routes().contains_key("notes/work-unselected.html"));
    assert_eq!(site.catalog().len(), 2);
    assert!(std::ptr::eq(
        site.catalog().entry("a").unwrap().document(),
        c.entry("a").unwrap().document()
    ));
    assert_eq!(site.source_ids().len(), 2);
    assert!(
        ComposedArtifact::Site(std::sync::Arc::new(site))
            .single()
            .is_err()
    );
}
#[test]
fn f05_route_conflicts_are_case_insensitive_and_cover_every_generator() {
    for (route, kind) in [
        ("INDEX.html", "composed"),
        ("notes/work-a.html", "document"),
        ("tags/storage.html", "tag"),
        ("search/index.json", "search"),
    ] {
        let mut spec = notebook();
        if kind == "search" {
            // A second search intention collides even if it selects another subset.
            spec.add_search(query(&["a"]));
        } else {
            spec.add_page("collision", route, publication("collision"));
        }
        assert_eq!(
            SiteAssembly::resolve(
                &spec,
                catalog(vec![doc("a")]),
                &ComponentRegistry::builtins()
            )
            .unwrap_err()
            .code,
            Code::OutputConflict,
            "{kind}"
        );
    }
    for route in [
        "../escape.html",
        "/absolute.html",
        "a//b.html",
        "a/%2e.html",
        "bad?query.html",
        "a\\b.html",
    ] {
        assert!(validate_site_route(route).is_err(), "{route}");
    }
    let mut no_index = SiteSpec::new(PublicationId("none".into()), "None");
    no_index.add_page("about", "about.html", publication("about"));
    assert!(
        SiteAssembly::resolve(&no_index, catalog(vec![]), &ComponentRegistry::builtins()).is_err()
    );
}
#[test]
fn f06_full_catalog_links_remain_valid_outside_rendered_subset_and_drafts_fail() {
    let mut a = doc("a");
    a.blocks = vec![Block::Paragraph {
        content: vec![Inline::DocumentLink(DocumentLink {
            title: None,
            target: ContentId("b".into()),
            anchor: ComponentId("ref-b".into()),
            fragment: None,
            content: vec![Inline::Text {
                value: "Other work".into(),
            }],
        })],
    }];
    let c = catalog(vec![a.clone(), doc("b")]);
    let spec = notebook();
    let site = SiteAssembly::resolve(&spec, c, &ComponentRegistry::builtins()).unwrap();
    assert!(
        site.catalog()
            .entry("b")
            .unwrap()
            .original_url
            .as_ref()
            .unwrap()
            .as_str()
            .ends_with("/original/repository/notes/work-b.html")
    );
    let mut b = doc("b");
    b.metadata.draft = true;
    assert_eq!(
        SiteAssembly::resolve(&spec, catalog(vec![a, b]), &ComponentRegistry::builtins())
            .unwrap_err()
            .code,
        Code::UnresolvedReference
    );
}
#[test]
fn f08_custom_document_prefix_and_redirects_share_one_policy() {
    let mut a = doc("a");
    a.metadata.previous_slugs = vec!["old-work".into()];
    let mut spec = SiteSpec::new(PublicationId("nested".into()), "Nested");
    spec.add_page("home", "index.html", publication("home"));
    spec.add_document_pages(query(&["a"]), "engineering/notes");
    spec.add_redirects(query(&["a"]));
    let site =
        SiteAssembly::resolve(&spec, catalog(vec![a]), &ComponentRegistry::builtins()).unwrap();
    assert_eq!(
        site.document_routes()[&ResourceId::local("a").unwrap()],
        "engineering/notes/work-a.html"
    );
    let SiteRoute::Redirect { target, .. } = &site.routes()["engineering/notes/old-work.html"]
    else {
        panic!("redirect expected")
    };
    assert_eq!(target, "engineering/notes/work-a.html");
}

#[test]
fn site_scope_is_link_evidence_not_an_unselected_body_dependency() {
    let spec = notebook();
    let first = SiteAssembly::resolve(
        &spec,
        catalog(vec![doc("a"), doc("unselected")]),
        &ComponentRegistry::builtins(),
    )
    .unwrap();
    let mut other = doc("unselected");
    other.blocks = vec![Block::Paragraph {
        content: vec![Inline::Text {
            value: "Another unselected body".into(),
        }],
    }];
    let second = SiteAssembly::resolve(
        &spec,
        catalog(vec![doc("a"), other]),
        &ComponentRegistry::builtins(),
    )
    .unwrap();
    assert_eq!(
        first.composition_fingerprint(),
        second.composition_fingerprint()
    );
    assert_ne!(
        first.catalog().input_digest(),
        second.catalog().input_digest()
    );
}

#[test]
fn cumulative_site_queries_and_property_storage_are_bounded_before_cloning() {
    let mut opts = PublicationOptions::default();
    opts.resource_limits.query_work = 2;
    let c = PublicationCatalog::new(DocumentCollection::new(vec![doc("a")]), opts).unwrap();
    assert_eq!(
        SiteAssembly::resolve(&notebook(), c, &ComponentRegistry::builtins())
            .unwrap_err()
            .code,
        Code::ResourceLimit
    );
    let mut opts = PublicationOptions::default();
    opts.limits.property_bytes = 500;
    let c = PublicationCatalog::new(DocumentCollection::new(vec![doc("a")]), opts).unwrap();
    let mut site = SiteSpec::new(PublicationId("bytes".into()), "Bytes");
    for n in 0..10 {
        site.add_page(
            n.to_string(),
            format!("{n}.html"),
            publication("Repeated view"),
        );
    }
    assert_eq!(
        SiteAssembly::resolve(&site, c, &ComponentRegistry::builtins())
            .unwrap_err()
            .code,
        Code::ResourceLimit
    );
}
