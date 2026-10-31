use berlin_content::DocumentCollection;
use berlin_document::{ContentId, Document, DocumentKind, Metadata, Provenance, SourceFormat};
use berlin_publication::*;
use serde_json::{Value, json};

fn local(id: &str, title: &str, date: Option<&str>, tags: &[&str]) -> Document {
    Document {
        id: ContentId(id.into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some(title.into()),
            slug: Some(id.into()),
            published: date.map(|d| d.parse().unwrap()),
            authors: vec!["Éva Dev".into()],
            tags: tags.iter().map(|t| (*t).into()).collect(),
            ..Default::default()
        },
        blocks: vec![],
        relations: vec![],
        provenance: Provenance {
            source: format!("file:///private/{id}.md"),
            source_format: SourceFormat::Markdown,
            source_hash: "a".repeat(64),
        },
    }
}
fn catalog(options: PublicationOptions) -> PublicationCatalog {
    let mut guide = local("l-b", "B", Some("2026-01-01"), &["storage", "durability"]);
    guide.kind = DocumentKind::Guide;
    let mut draft = local("draft", "PRIVATE_TITLE", None, &["storage"]);
    draft.metadata.draft = true;
    let documents = DocumentCollection::new(vec![
        local("l-a", "A", Some("2026-02-01"), &["STORAGE", "rust"]),
        guide,
        local("l-undated", "Ä", None, &["misc"]),
        draft,
    ]);
    let records=[("x-a","article","A",Some("2026-02-01"),Some("2026-10-01"),vec!["storage","rust"]),
        ("x-paper","paper","Paper",Some("2025-01-01"),Some("2026-09-01"),vec!["storage","durability"]),
        ("x-repo","repository","Repository",None,None,vec!["storage"])].into_iter().enumerate().map(|(i,(id,kind,title,date,added,tags))| {
        (serde_json::from_value(json!({"id":id,"kind":kind,"url":format!("https://example.test/{id}"),"title":title,"published":date,"added":added,"topics":tags})).unwrap(),ResourceSource {namespace:"reading".into(),logical_name:"reading.yaml".into(),sha256:"b".repeat(64),record:i+1})
    }).collect();
    PublicationCatalog::with_resources(
        documents,
        ExternalResourceCollection::new(
            records,
            &ResourceLimits::default(),
            &PublicationLimits::default(),
        )
        .unwrap(),
        options,
    )
    .unwrap()
}
fn options() -> PublicationOptions {
    PublicationOptions {
        source_site_url: Some("https://example.test/notebook".parse().unwrap()),
        ..Default::default()
    }
}
fn query(value: Value) -> ResourceQuery {
    serde_json::from_value(value).unwrap()
}
fn ids(catalog: &PublicationCatalog, value: Value) -> Vec<String> {
    catalog
        .select(&query(value))
        .unwrap()
        .ids()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn d01_origins_and_kinds_or_within_fields_and_across_fields() {
    let c = catalog(options());
    assert_eq!(
        ids(
            &c,
            json!({"origins_any":["local","external"],"kinds_any":["article"]})
        ),
        ["external:x-a", "local:l-a", "local:l-undated"]
    );
    assert_eq!(
        ids(
            &c,
            json!({"origins_any":["external"],"kinds_any":["article","paper"]})
        ),
        ["external:x-a", "external:x-paper"]
    );
}

#[test]
fn d02_d03_all_any_none_tags_share_ingestion_normalization() {
    let c = catalog(options());
    assert_eq!(
        ids(&c, json!({"topics_all":[" Storage ","RUST"]})),
        ["external:x-a", "local:l-a"]
    );
    assert_eq!(
        ids(&c, json!({"topics_any":["rust","durability"]})),
        ["external:x-a", "local:l-a", "local:l-b", "external:x-paper"]
    );
    assert_eq!(
        ids(
            &c,
            json!({"topics_all":["storage"],"topics_none":["durability"]})
        ),
        ["external:x-a", "local:l-a", "external:x-repo"]
    );
    assert!(ids(&c, json!({"topics_all":["rust"],"topics_none":["RUST"]})).is_empty());
    let a = c
        .select(&query(json!({"topics_all":["RUST","Storage"]})))
        .unwrap();
    let b = c
        .select(&query(json!({"topics_all":["storage","rust"]})))
        .unwrap();
    assert_eq!(a.query_digest(), b.query_digest());
    assert_eq!(a.selection_digest(), b.selection_digest());
}

#[test]
fn d04_inclusive_independent_date_axes_exclude_missing_dates() {
    let c = catalog(options());
    assert_eq!(
        ids(
            &c,
            json!({"published_from":"2026-01-01","published_to":"2026-02-01"})
        ),
        ["external:x-a", "local:l-a", "local:l-b"]
    );
    assert_eq!(
        ids(
            &c,
            json!({"added_from":"2026-09-01","added_to":"2026-09-01"})
        ),
        ["external:x-paper"]
    );
    assert_eq!(
        ids(
            &c,
            json!({"published_from":"2026-01-01","added_from":"2026-10-01"})
        ),
        ["external:x-a"]
    );
    for value in [
        json!({"published_from":"2026-03-01","published_to":"2026-01-01"}),
        json!({"added_from":"2026-03-01","added_to":"2026-01-01"}),
    ] {
        assert!(c.select(&query(value)).is_err());
    }
}

#[test]
fn d05_d06_all_orders_have_byte_stable_ties_and_undated_last() {
    let c = catalog(options());
    assert_eq!(
        ids(&c, json!({})),
        [
            "external:x-a",
            "local:l-a",
            "local:l-b",
            "external:x-paper",
            "external:x-repo",
            "local:l-undated"
        ]
    );
    assert_eq!(
        ids(&c, json!({"order":"added_desc"})),
        [
            "external:x-a",
            "external:x-paper",
            "external:x-repo",
            "local:l-a",
            "local:l-b",
            "local:l-undated"
        ]
    );
    assert_eq!(
        ids(&c, json!({"order":"title_asc"})),
        [
            "external:x-a",
            "local:l-a",
            "local:l-b",
            "external:x-paper",
            "external:x-repo",
            "local:l-undated"
        ]
    );
    assert_eq!(
        ids(&c, json!({"order":"id_asc"})),
        [
            "external:x-a",
            "external:x-paper",
            "external:x-repo",
            "local:l-a",
            "local:l-b",
            "local:l-undated"
        ]
    );
}

#[test]
fn d07_d08_curated_ids_are_ordered_intersected_and_cannot_bypass_eligibility() {
    let c = catalog(options());
    assert_eq!(
        ids(
            &c,
            json!({"ids":["local:l-b","external:x-a"],"order":"curated"})
        ),
        ["local:l-b", "external:x-a"]
    );
    assert_eq!(
        ids(
            &c,
            json!({"ids":["local:l-b","external:x-a"],"origins_any":["external"],"order":"curated"})
        ),
        ["external:x-a"]
    );
    for value in [
        json!({"order":"curated"}),
        json!({"ids":["local:l-b","local:l-b"]}),
        json!({"ids":["local:missing"]}),
        json!({"ids":["local:draft"],"origins_any":["external"]}),
    ] {
        let error = c.select(&query(value)).unwrap_err();
        assert!(!error.to_string().contains("PRIVATE_TITLE"));
    }
    assert!(ids(&c, json!({"ids":[],"order":"curated"})).is_empty());
}

#[test]
fn d09_limits_apply_after_sorting_and_have_exact_boundaries() {
    let mut opts = options();
    opts.limits.query_sources = 2;
    let c = catalog(opts);
    assert!(c.select(&ResourceQuery::default()).is_err());
    assert_eq!(ids(&c, json!({"limit":2})), ["external:x-a", "local:l-a"]);
    assert_eq!(
        ids(&c, json!({"topics_any":["rust","durability"],"limit":1})),
        ["external:x-a"]
    );
    for value in [json!({"limit":0}), json!({"limit":3})] {
        assert_eq!(
            c.select(&query(value)).unwrap_err().code,
            Code::ResourceLimit
        );
    }
    let mut opts = options();
    opts.resource_limits.query_terms = 1;
    let c = catalog(opts);
    assert!(c.select(&query(json!({"topics_all":["storage"]}))).is_ok());
    assert_eq!(
        c.select(&query(json!({"topics_all":["storage","rust"]})))
            .unwrap_err()
            .code,
        Code::ResourceLimit
    );
}

#[test]
fn d10_bad_queries_fail_instead_of_broadening_selection() {
    let c = catalog(options());
    for value in [
        json!({"kinds_any":["unknown"]}),
        json!({"origins_any":["remote"]}),
        json!({"capabilities_all":["magical"]}),
        json!({"order":"random"}),
        json!({"extra":true}),
    ] {
        assert!(serde_json::from_value::<ResourceQuery>(value).is_err());
    }
    for value in [
        json!({"topics_any":[]}),
        json!({"origins_any":[]}),
        json!({"kinds_any":[]}),
        json!({"topics_all":[""]}),
        json!({"topics_all":["rust","RUST"]}),
        json!({"source_namespace":"../private"}),
        json!({"source_namespace":"unknown"}),
        json!({"author_text":"   "}),
    ] {
        assert!(c.select(&query(value)).is_err());
    }
    assert_eq!(
        ids(&c, json!({"author_text":"ÉVA"})),
        ["local:l-a", "local:l-b", "local:l-undated"]
    );
    assert_eq!(
        ids(
            &c,
            json!({"source_namespace":"reading","kinds_any":["article"]})
        ),
        ["external:x-a"]
    );
    let mut custom = local("custom", "Custom", None, &[]);
    custom.kind = DocumentKind::Custom("research:report".into());
    let c = PublicationCatalog::new(DocumentCollection::new(vec![custom]), options()).unwrap();
    assert_eq!(
        ids(&c, json!({"kinds_any":["research:report"]})),
        ["local:custom"]
    );
}

#[test]
fn d12_d13_capability_narrowing_and_empty_selections() {
    let c = catalog(options());
    let articles = c.select(&query(json!({"kinds_any":["article"]}))).unwrap();
    assert!(articles.local_documents().is_err());
    let local = c
        .select(&query(
            json!({"capabilities_all":["excerptable","has_semantic_body"]}),
        ))
        .unwrap();
    assert_eq!(local.len(), 3);
    for source in local.local_documents().unwrap() {
        c.resolve(&source).unwrap();
    }
    assert!(
        c.select(&query(json!({"topics_all":["not-present"]})))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn d14_parallel_queries_reuse_the_immutable_snapshot() {
    let c = catalog(options());
    let before = c.input_digest().to_owned();
    let children = (0..4)
        .map(|_| {
            let c = c.clone();
            std::thread::spawn(move || c.select(&query(json!({"topics_all":["storage"]}))).unwrap())
        })
        .collect::<Vec<_>>();
    let results = children
        .into_iter()
        .map(|child| child.join().unwrap())
        .collect::<Vec<_>>();
    for result in results {
        assert_eq!(result.len(), 5);
        assert!(std::ptr::eq(
            result.get(&ResourceId::local("l-a").unwrap()).unwrap(),
            c.get(&ResourceId::local("l-a").unwrap()).unwrap()
        ));
    }
    assert_eq!(c.input_digest(), before);
}

#[test]
fn d15_selective_indexed_queries_do_not_scan_or_copy_all_source_records() {
    let records=(0..10_000).map(|i| {
        (serde_json::from_value(json!({"id":format!("r{i:05}"),"kind":"repository","title":"Repository","url":format!("https://example.test/r{i}"),"topics":[if i==9999 {"rare"}else{"ordinary"}]})).unwrap(),ResourceSource{namespace:"references".into(),logical_name:"resources.yaml".into(),sha256:"a".repeat(64),record:i+1})
    }).collect();
    let external = ExternalResourceCollection::new(
        records,
        &ResourceLimits::default(),
        &PublicationLimits::default(),
    )
    .unwrap();
    let c = PublicationCatalog::with_resources(DocumentCollection::default(), external, options())
        .unwrap();
    assert_eq!(c.len(), 10_000);
    for _ in 0..20 {
        let result = c.select(&query(json!({"topics_all":["rare"]}))).unwrap();
        assert_eq!(result.examined_candidates(), 1);
        assert_eq!(result.ids(), &[ResourceId::external("r09999").unwrap()]);
        assert!(std::ptr::eq(
            result.iter().next().unwrap(),
            c.get(&ResourceId::external("r09999").unwrap()).unwrap()
        ));
    }
}
