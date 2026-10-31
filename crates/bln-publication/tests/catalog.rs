use berlin_content::DocumentCollection;
use berlin_document::{ContentId, Document, DocumentKind, Metadata, Provenance, SourceFormat};
use berlin_publication::*;
use std::collections::BTreeMap;

fn document(id: &str, date: Option<&str>, tags: &[&str]) -> Document {
    Document {
        id: ContentId(id.into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some(format!("Work {id}")),
            slug: Some(format!("work-{id}")),
            published: date.map(|d| d.parse().unwrap()),
            tags: tags.iter().map(|t| (*t).into()).collect(),
            ..Default::default()
        },
        blocks: Vec::new(),
        relations: Vec::new(),
        provenance: Provenance {
            source: format!("file:///private/notes/{id}.md"),
            source_format: SourceFormat::Markdown,
            source_hash: "a".repeat(64),
        },
    }
}
fn documents() -> Vec<Document> {
    let mut draft = document("draft", Some("2026-03-01"), &["systems"]);
    draft.metadata.draft = true;
    let mut guide = document("guide", Some("2025-01-01"), &["systems", "storage"]);
    guide.kind = DocumentKind::Guide;
    vec![
        document("a", Some("2026-01-01"), &["systems", "storage"]),
        document("b", Some("2026-01-01"), &["systems"]),
        document("old", Some("2024-01-01"), &["storage"]),
        document("undated", None, &["systems"]),
        draft,
        guide,
    ]
}
fn catalog(docs: Vec<Document>) -> PublicationCatalog {
    PublicationCatalog::new(
        DocumentCollection::new(docs),
        PublicationOptions {
            source_site_url: Some("https://example.test/notebook".parse().unwrap()),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn f17_selected_source_budget_has_an_exact_boundary() {
    let c = PublicationCatalog::new(
        DocumentCollection::new(documents()),
        PublicationOptions {
            limits: PublicationLimits {
                query_sources: 2,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        c.select(&WorkQuery {
            limit: Some(2),
            ..Default::default()
        })
        .unwrap()
        .len(),
        2
    );
    assert!(
        c.select(&WorkQuery {
            limit: Some(3),
            ..Default::default()
        })
        .is_err()
    );
    assert!(c.select(&WorkQuery::default()).is_err());
}
fn ids(catalog: &PublicationCatalog, query: WorkQuery) -> Vec<String> {
    catalog
        .select(&query)
        .unwrap()
        .into_iter()
        .map(|r| r.id.0)
        .collect()
}

#[test]
fn a05_explicit_curation_preserves_order_and_rejects_invalid_sources() {
    let c = catalog(documents());
    let query = |names: &[&str]| WorkQuery {
        ids: Some(names.iter().map(|n| (*n).into()).collect()),
        order: WorkOrder::Curated,
        ..Default::default()
    };
    assert_eq!(ids(&c, query(&["b", "old", "a"])), ["b", "old", "a"]);
    for names in [&["a", "a"][..], &["missing"], &["draft"]] {
        assert!(c.select(&query(names)).is_err());
    }
    assert_eq!(c.work_ref("draft").unwrap_err().code, Code::DraftSelection);
}

#[test]
fn a06_a07_a08_filters_dates_ties_undated_and_limits() {
    let c = catalog(documents());
    assert_eq!(
        ids(&c, WorkQuery::default()),
        ["a", "b", "guide", "old", "undated"]
    );
    assert_eq!(
        ids(
            &c,
            WorkQuery {
                order: WorkOrder::PublishedAsc,
                ..Default::default()
            }
        ),
        ["old", "guide", "a", "b", "undated"]
    );
    assert_eq!(
        ids(
            &c,
            WorkQuery {
                tags_all: vec!["systems".into(), "storage".into()],
                ..Default::default()
            }
        ),
        ["a", "guide"]
    );
    assert_eq!(
        ids(
            &c,
            WorkQuery {
                tags_any: vec!["storage".into()],
                ..Default::default()
            }
        ),
        ["a", "guide", "old"]
    );
    assert_eq!(
        ids(
            &c,
            WorkQuery {
                kinds: vec!["guide".into()],
                ..Default::default()
            }
        ),
        ["guide"]
    );
    assert_eq!(
        ids(
            &c,
            WorkQuery {
                published_from: Some("2026-01-01".parse().unwrap()),
                published_to: Some("2026-01-01".parse().unwrap()),
                ..Default::default()
            }
        ),
        ["a", "b"]
    );
    assert_eq!(
        ids(
            &c,
            WorkQuery {
                tags_any: vec!["storage".into()],
                limit: Some(1),
                ..Default::default()
            }
        ),
        ["a"]
    );
    let empty = WorkQuery {
        tags_all: vec!["Systems".into()],
        ..Default::default()
    };
    assert!(c.select(&empty).is_err());
    assert_eq!(
        ids(
            &c,
            WorkQuery {
                allow_empty: true,
                ..empty
            }
        ),
        Vec::<String>::new()
    );
    for kind in ["unknown", "custom:"] {
        assert!(
            c.select(&WorkQuery {
                kinds: vec![kind.into()],
                ..Default::default()
            })
            .is_err()
        );
    }
    assert!(
        c.select(&WorkQuery {
            limit: Some(0),
            ..Default::default()
        })
        .is_err()
    );
    assert!(serde_json::from_str::<WorkQuery>(r#"{"unknown":1}"#).is_err());
}

#[test]
fn a07_title_ties_and_unicode_order_are_not_locale_dependent() {
    let mut docs = documents();
    for d in &mut docs {
        d.metadata.title = Some("Equal".into());
    }
    let c = catalog(docs);
    assert_eq!(
        ids(
            &c,
            WorkQuery {
                order: WorkOrder::TitleAsc,
                ..Default::default()
            }
        ),
        ["a", "b", "guide", "old", "undated"]
    );
}

#[test]
fn a09_a11_queries_evolve_while_curation_and_order_remain_explicit() {
    let c = catalog(documents());
    let curated = WorkQuery {
        ids: Some(vec!["b".into(), "a".into()]),
        order: WorkOrder::Curated,
        ..Default::default()
    };
    let mut newer = documents();
    newer.push(document("new", Some("2026-10-30"), &["systems"]));
    newer.reverse();
    let d = catalog(newer);
    assert_ne!(ids(&c, WorkQuery::default()), ids(&d, WorkQuery::default()));
    assert_eq!(ids(&c, curated.clone()), ids(&d, curated));
}

#[test]
fn a10_source_and_mapping_pins_are_distinct() {
    let c = catalog(documents());
    let entry = c.entry("a").unwrap();
    let mut reference = c.work_ref("a").unwrap();
    reference.expected_source_hash = Some(entry.source_sha256.clone());
    reference.expected_document_digest = Some(entry.document_digest.clone());
    c.pinned_work_ref(reference.clone()).unwrap();
    let mut changed = documents();
    changed[0].metadata.description = Some("Mapped annotation".into());
    let d = catalog(changed);
    assert_eq!(
        d.resolve(&reference).unwrap_err().code,
        Code::SourceDigestMismatch
    );
    reference.expected_document_digest = None;
    d.resolve(&reference).unwrap();
    reference.expected_source_hash = Some("0".repeat(64));
    assert_eq!(
        d.resolve(&reference).unwrap_err().code,
        Code::SourceDigestMismatch
    );
}

fn spec() -> PublicationSpec {
    let card = FeaturedWork {
        source: ContentRef::new("a"),
        annotation: Some("Editorial note".into()),
    }
    .into_call();
    PublicationSpec {
        schema_version: 1,
        metadata: PublicationMetadata {
            id: PublicationId("portfolio".into()),
            title: "Selected work".into(),
            description: None,
        },
        root: Portfolio {}.into_call().with_body(ViewFragment(vec![
            card.clone().keyed("first"),
            card.keyed("second"),
        ])),
    }
}

#[test]
fn a12_b01_b02_catalog_sharing_and_separate_source_mounts() {
    let c = catalog(documents());
    let before = c.clone();
    let assembly =
        PublicationAssembly::resolve(&spec(), c, &ComponentRegistry::builtins()).unwrap();
    assert!(std::ptr::eq(
        before.entry("a").unwrap().document(),
        assembly.catalog().entry("a").unwrap().document()
    ));
    assert_eq!(assembly.sources().len(), 1);
    assert_ne!(
        assembly.placements()[1].occurrence.id,
        assembly.placements()[2].occurrence.id
    );
    assert_eq!(
        assembly
            .catalog()
            .entry("old")
            .unwrap()
            .original_url
            .as_ref()
            .unwrap()
            .as_str(),
        "https://example.test/notebook/notes/work-old.html"
    );
    let manifest = assembly
        .manifest(
            Channel::Html,
            BTreeMap::new(),
            fingerprint("config/v1", &"https://example.test/portfolio").unwrap(),
        )
        .unwrap();
    let other = assembly
        .manifest(
            Channel::Html,
            BTreeMap::from([("pages/example.tera".into(), "b".repeat(64))]),
            fingerprint("config/v1", &"https://example.test/portfolio").unwrap(),
        )
        .unwrap();
    assert_eq!(
        manifest.composition_fingerprint,
        other.composition_fingerprint
    );
    assert_ne!(
        manifest.render_input_fingerprint,
        other.render_input_fingerprint
    );
    let public = String::from_utf8(canonical_bytes(&manifest).unwrap()).unwrap();
    assert!(!public.contains("private/notes"));
    assert!(!public.contains("draft"));
    assert!(!public.contains("Work old"));
    assert_eq!(assembly.catalog().len(), 5);
}

#[test]
fn b01_source_urls_reject_local_paths_credentials_and_preserve_prefixes() {
    for base in [
        "file:///private/notes",
        "https://user:secret@example.test/",
        "https://example.test/?token=secret",
    ] {
        assert!(
            PublicationCatalog::new(
                DocumentCollection::new(documents()),
                PublicationOptions {
                    source_site_url: Some(base.parse().unwrap()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    let no_url = PublicationCatalog::new(
        DocumentCollection::new(documents()),
        PublicationOptions::default(),
    )
    .unwrap();
    assert_eq!(
        PublicationAssembly::resolve(&spec(), no_url, &ComponentRegistry::builtins())
            .unwrap_err()
            .code,
        Code::UnsafeUrl
    );
    let c = PublicationCatalog::with_original_urls(
        DocumentCollection::new(documents()),
        PublicationOptions::default(),
        BTreeMap::from([(
            "a".into(),
            "https://original.example/article".parse().unwrap(),
        )]),
    )
    .unwrap();
    PublicationAssembly::resolve(&spec(), c, &ComponentRegistry::builtins()).unwrap();
}
