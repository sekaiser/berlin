use berlin_content::DocumentCollection;
use berlin_document::*;
use berlin_publication::*;
use serde_json::json;

fn doc(id: &str, date: &str) -> Document {
    Document {
        id: ContentId(id.into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some(format!("Work {id}")),
            slug: Some(format!("work-{id}")),
            published: Some(date.parse().unwrap()),
            tags: vec!["Storage".into()],
            ..Default::default()
        },
        blocks: vec![],
        relations: vec![],
        provenance: Provenance {
            source: format!("file:///private/{id}.md"),
            source_hash: "a".repeat(64),
            source_format: SourceFormat::Markdown,
        },
    }
}
fn catalog() -> PublicationCatalog {
    let mut draft = doc("draft", "2026-10-31");
    draft.metadata.draft = true;
    let mut guide = doc("guide", "2026-10-01");
    guide.kind = DocumentKind::Guide;
    let mut recent = doc("recent", "2026-10-30");
    recent.metadata.previous_slugs = vec!["former-title".into()];
    let external = ExternalResourceCollection::new(vec![(
        serde_json::from_value(json!({"id":"paper", "kind":"paper", "title":"External work", "url":"https://example.test/paper", "topics":["Storage"]})).unwrap(),
        ResourceSource {namespace:"reading".into(), logical_name:"reading.yaml".into(), sha256:"b".repeat(64), record:1},
    )], &ResourceLimits::default(), &PublicationLimits::default()).unwrap();
    PublicationCatalog::with_resources(
        DocumentCollection::new(vec![doc("old", "2025-01-01"), recent, guide, draft]),
        external,
        PublicationOptions {
            source_site_url: Some("https://example.test".parse().unwrap()),
            ..Default::default()
        },
    )
    .unwrap()
}
#[test]
fn notebook_preserves_document_identity_and_builds_mixed_indexes_and_redirects() {
    let catalog = catalog();
    let registry = ComponentRegistry::builtins();
    let mut options = NotebookConfig::default();
    options.pages.push(NotebookPage {
        id: "about".into(),
        route: "about.html".into(),
        title: "About".into(),
        introduction: "An authored introduction.".into(),
    });
    let ComposedArtifact::Site(site) = StandardComposition::Notebook((options.clone()).into())
        .resolve(catalog.clone(), &registry)
        .unwrap()
    else {
        panic!("site")
    };
    for route in [
        "index.html",
        "notes.html",
        "feed.html",
        "about.html",
        "search.html",
        "search/index.json",
        "notes/work-recent.html",
        "notes/former-title.html",
        "tags/storage.html",
    ] {
        assert!(site.routes().contains_key(route), "{route}");
    }
    assert!(!site.routes().contains_key("notes/work-draft.html"));
    let SiteRoute::Tag { resources, .. } = &site.routes()["tags/storage.html"] else {
        panic!("tag")
    };
    assert_eq!(resources.len(), 4); // three local works, one external reference
    assert!(
        resources
            .iter()
            .any(|r| r.id.origin() == ResourceOrigin::External)
    );
    let SiteRoute::Composed { assembly: home, .. } = &site.routes()["index.html"] else {
        panic!("home")
    };
    let home_body = &home.spec().root.slots["body"].0;
    assert_eq!(
        home_body[0].properties.get("title"),
        Some(&Property::Text("Start here".into()))
    );
    let Some(Property::List(guides)) = home_body[0].properties.get("sources") else {
        panic!("guide references")
    };
    assert_eq!(guides.len(), 1);
    assert!(matches!(&guides[0], Property::Resource(r) if r.id.as_str() == "local:guide"));
    assert_eq!(
        home_body[1].properties.get("title"),
        Some(&Property::Text("From the notebook".into()))
    );
    let SiteRoute::Composed { assembly, .. } = &site.routes()["notes.html"] else {
        panic!("notes")
    };
    let featured: Vec<_> = assembly.spec().root.slots["body"]
        .0
        .iter()
        .flat_map(|section| section.slots.values().flat_map(|slot| slot.0.iter()))
        .filter_map(|call| match call.properties.get("source") {
            Some(Property::Resource(r)) => Some(r.id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(featured, ["local:recent", "local:guide", "local:old"]);
    let again = StandardComposition::Notebook((options).into())
        .resolve(catalog, &registry)
        .unwrap();
    let ComposedArtifact::Site(again) = again else {
        panic!("site")
    };
    assert_eq!(
        site.composition_fingerprint(),
        again.composition_fingerprint()
    );
    assert!(!site.spec().queries.is_empty());
}
#[test]
fn empty_notebook_is_valid_and_filters_do_not_shrink_source_scope() {
    let empty =
        PublicationCatalog::new(DocumentCollection::default(), PublicationOptions::default())
            .unwrap();
    assert!(
        StandardComposition::Notebook(Default::default())
            .resolve(empty, &ComponentRegistry::builtins())
            .is_ok()
    );
    let config = NotebookConfig {
        documents: ResourceQuery {
            ids: Some(vec![ResourceId::local("recent").unwrap()]),
            ..Default::default()
        },
        ..Default::default()
    };
    let ComposedArtifact::Site(site) = StandardComposition::Notebook((config).into())
        .resolve(catalog(), &ComponentRegistry::builtins())
        .unwrap()
    else {
        panic!("site")
    };
    assert!(site.routes().contains_key("notes/work-recent.html"));
    assert!(!site.routes().contains_key("notes/work-old.html"));
    assert_eq!(
        site.spec().source_scope.origins_any,
        Some(vec![ResourceOrigin::Local])
    );
}
#[test]
fn home_limit_preserves_the_archive_guides_routes_and_selection_evidence() {
    let options = NotebookConfig {
        home_limit: Some(1),
        ..Default::default()
    };
    let ComposedArtifact::Site(site) = StandardComposition::Notebook(options.into())
        .resolve(catalog(), &ComponentRegistry::builtins())
        .unwrap()
    else {
        panic!("site")
    };
    let cards = |route: &str| {
        let SiteRoute::Composed { assembly, .. } = &site.routes()[route] else {
            panic!("composed page")
        };
        assembly.spec().root.slots["body"]
            .0
            .iter()
            .flat_map(|section| section.slots.values().flat_map(|slot| slot.0.iter()))
            .filter_map(|call| match call.properties.get("source") {
                Some(Property::Resource(r)) => Some(r.id.as_str().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(cards("index.html"), ["local:recent"]);
    assert_eq!(
        cards("notes.html"),
        ["local:recent", "local:guide", "local:old"]
    );
    let SiteRoute::Composed { assembly, .. } = &site.routes()["index.html"] else {
        panic!("home")
    };
    let Some(Property::List(guides)) = assembly.spec().root.slots["body"].0[0]
        .properties
        .get("sources")
    else {
        panic!("guides")
    };
    assert!(matches!(&guides[0], Property::Resource(r) if r.id.as_str() == "local:guide"));
    for route in [
        "notes/work-old.html",
        "notes/work-guide.html",
        "notes/former-title.html",
        "tags/storage.html",
        "search/index.json",
    ] {
        assert!(site.routes().contains_key(route), "{route}");
    }
    let evidence = site
        .spec()
        .queries
        .values()
        .find(|evidence| evidence.query.limit == Some(1))
        .unwrap();
    assert_eq!(evidence.selected.len(), 1);
    assert_eq!(evidence.selected[0].id.as_str(), "local:recent");

    let options = NotebookConfig {
        home_limit: Some(5),
        documents: ResourceQuery {
            limit: Some(1),
            ..NotebookConfig::default().documents
        },
        ..Default::default()
    };
    let spec = options.compose(&catalog()).unwrap();
    assert!(spec.queries.values().all(|e| e.query.limit != Some(5)));
}
#[test]
fn presets_refuse_bad_routes_types_collisions_and_resource_budgets() {
    for value in [
        json!({"typo":true}),
        json!({"pages":[{"id":"x","title":"X","route":"x.html","unknown":true}]}),
        json!({"documents":{"limit":"all"}}),
        json!({"home_limit":-1}),
        json!({"home_limit":"six"}),
    ] {
        assert!(serde_json::from_value::<NotebookConfig>(value).is_err());
    }
    assert_eq!(
        NotebookConfig {
            home_limit: Some(0),
            ..Default::default()
        }
        .compose(&catalog())
        .unwrap_err()
        .code,
        Code::InvalidProperty
    );
    for route in ["../escape.html", "notes.html"] {
        let config = NotebookConfig {
            pages: vec![NotebookPage {
                id: "extra".into(),
                route: route.into(),
                title: "Extra".into(),
                introduction: String::new(),
            }],
            ..Default::default()
        };
        assert!(
            StandardComposition::Notebook((config).into())
                .resolve(catalog(), &ComponentRegistry::builtins())
                .is_err()
        );
    }
    let bad = NotebookConfig {
        documents: ResourceQuery {
            origins_any: Some(vec![ResourceOrigin::External]),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(bad.compose(&catalog()).is_err());
    let mut options = PublicationOptions::default();
    options.limits.occurrences = 8;
    let small = PublicationCatalog::new(DocumentCollection::default(), options).unwrap();
    assert_eq!(
        NotebookConfig::default().compose(&small).unwrap_err().code,
        Code::ResourceLimit
    );
}
#[test]
fn article_draft_is_explicit_bounded_and_records_selection_evidence() {
    let cat = catalog();
    let config = ArticleDraftConfig::default();
    let spec = config.compose(&cat).unwrap();
    assert_eq!(spec.metadata.title, "Work recent");
    assert_eq!(spec.queries.len(), 1);
    let explicit = ArticleDraftConfig {
        selection: ResourceQuery {
            ids: Some(vec![ResourceId::local("old").unwrap()]),
            ..Default::default()
        },
    };
    assert_eq!(explicit.compose(&cat).unwrap().metadata.title, "Work old");
    let ambiguous = ArticleDraftConfig {
        selection: ResourceQuery::default(),
    };
    assert!(ambiguous.compose(&cat).is_err());
    let external = ArticleDraftConfig {
        selection: ResourceQuery {
            ids: Some(vec![ResourceId::external("paper").unwrap()]),
            ..Default::default()
        },
    };
    assert!(external.compose(&cat).is_err());
}
