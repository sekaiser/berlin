use berlin_document::DocumentKind;
use berlin_publication::*;
use serde_json::{Value, json};
use std::sync::Arc;

fn resource() -> ExternalResource {
    serde_json::from_value(json!({
        "id":"logging", "kind":"paper", "title":"Logging <notes> & results",
        "url":"https://example.test/Papers/Log?x=1&y=2#Section",
        "authors":["Second author", "First author"],
        "topics":["Storage","storage","durability"],
        "published":"2024-02-29", "added":"2026-10-30"
    }))
    .unwrap()
}
fn source(name: &str) -> ResourceSource {
    ResourceSource {
        namespace: "reading".into(),
        logical_name: name.into(),
        sha256: "a".repeat(64),
        record: 1,
    }
}
fn normalize(value: ExternalResource) -> Result<ExternalResource> {
    value.normalized(&ResourceLimits::default(), &PublicationLimits::default())
}

#[test]
fn a01_a02_tagged_resource_identity_is_validated_and_case_preserving() {
    let local = ResourceId::local("same.ID").unwrap();
    let external = ResourceId::external("same.ID").unwrap();
    assert_ne!(local, external);
    for id in [local, external, ResourceId::local("org:uuid").unwrap()] {
        assert_eq!(
            serde_json::from_str::<ResourceId>(&serde_json::to_string(&id).unwrap()).unwrap(),
            id
        );
        assert_eq!(
            id.leaf(),
            if id.as_str().ends_with("uuid") {
                "org:uuid"
            } else {
                "same.ID"
            }
        );
    }
    for value in [
        "",
        "local:",
        "external:",
        "remote:hello",
        "external:a/b",
        "external:a b",
        "external: bad",
        "local:a\n",
    ] {
        assert_eq!(
            value.parse::<ResourceId>().unwrap_err().code,
            Code::InvalidIdentifier
        );
    }
    assert!(ResourceId::external(&"a".repeat(256)).is_ok());
    assert!(ResourceId::external(&"a".repeat(257)).is_err());
}

#[test]
fn a03_duplicates_name_both_logical_sources_without_overwriting() {
    let error = ExternalResourceCollection::new(
        vec![
            (resource(), source("one.yaml")),
            (resource(), source("two.csv")),
        ],
        &ResourceLimits::default(),
        &PublicationLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::DuplicateResource);
    assert!(error.detail.contains("one.yaml record 1"));
    assert!(error.detail.contains("two.csv record 1"));
    assert_eq!(error.source_id.as_deref(), Some("external:logging"));
}

#[test]
fn a04_url_changes_revision_without_changing_identity() {
    let original = normalize(resource()).unwrap();
    let mut changed = original.clone();
    changed.url = "https://example.test/new#part".into();
    let changed = normalize(changed).unwrap();
    assert_eq!(
        original.resource_id().unwrap(),
        changed.resource_id().unwrap()
    );
    assert_ne!(original.revision().unwrap(), changed.revision().unwrap());
    assert_eq!(
        original.url,
        "https://example.test/Papers/Log?x=1&y=2#Section"
    );
}

#[test]
fn a05_a06_classification_is_orthogonal_and_custom_kinds_are_namespaced() {
    for (kind, expected) in [
        (DocumentKind::Article, "article"),
        (DocumentKind::Guide, "guide"),
        (DocumentKind::Note, "note"),
        (DocumentKind::SocialPost, "social_post"),
        (DocumentKind::Custom("essay".into()), "custom:essay"),
        (
            DocumentKind::Custom("research:report".into()),
            "research:report",
        ),
    ] {
        assert_eq!(
            ResourceKind::from_document(&kind).unwrap().as_str(),
            expected
        );
    }
    let paper = normalize(resource()).unwrap();
    assert_eq!(
        paper.resource_id().unwrap().origin(),
        ResourceOrigin::External
    );
    assert_eq!(paper.kind.as_str(), "paper");
    assert_eq!(paper.authors, vec!["Second author", "First author"]);
    assert_eq!(paper.topics, vec!["durability", "Storage"]);
    let record = serde_json::to_value(paper).unwrap();
    assert!(record.get("editorial_role").is_none());
    assert!(record.get("capabilities").is_none());
}

#[test]
fn a07_strict_fields_versions_kinds_and_dates() {
    for (key, value) in [
        ("unknown", json!(true)),
        ("published", json!("2025-02-29")),
        ("kind", json!("mystery")),
        ("capabilities", json!(["excerptable"])),
    ] {
        let mut input = serde_json::to_value(resource()).unwrap();
        input[key] = value;
        assert!(
            serde_json::from_value::<ExternalResource>(input).is_err(),
            "{key}"
        );
    }
    let mut wrong = resource();
    wrong.schema_version = 2;
    assert_eq!(normalize(wrong).unwrap_err().code, Code::UnknownVersion);
    for title in ["", "  \t"] {
        let mut wrong = resource();
        wrong.title = title.into();
        assert!(normalize(wrong).is_err());
    }
    for kind in ["", "Repository", "article/no", "bad:one:two"] {
        assert_eq!(
            kind.parse::<ResourceKind>().unwrap_err().code,
            Code::UnknownResourceKind
        );
    }
}

#[test]
fn a08_a10_optional_metadata_is_not_inferred_and_external_has_no_body() {
    let mut input = serde_json::to_value(resource()).unwrap();
    input.as_object_mut().unwrap().remove("authors");
    input.as_object_mut().unwrap().remove("published");
    let value = normalize(serde_json::from_value(input).unwrap()).unwrap();
    assert!(value.authors.is_empty());
    assert!(value.publisher.is_none());
    assert!(value.license_note.is_none());
    assert_eq!(
        value.capabilities(),
        [ResourceCapability::Linkable].into_iter().collect()
    );
    assert!(
        !value
            .capabilities()
            .contains(&ResourceCapability::Excerptable)
    );
    let mut raw = serde_json::to_value(&value).unwrap();
    raw["blocks"] = json!([]);
    assert!(serde_json::from_value::<ExternalResource>(raw).is_err());
}

#[test]
fn a11_previews_need_authored_public_url_and_nonempty_alt() {
    let mut value = resource();
    value.preview = Some(ResourcePreview {
        url: "https://example.test/cover.png".into(),
        alt: "A book cover".into(),
    });
    assert!(
        normalize(value.clone())
            .unwrap()
            .capabilities()
            .contains(&ResourceCapability::HasPreview)
    );
    value.preview.as_mut().unwrap().alt = " \t".into();
    assert!(normalize(value.clone()).is_err());
    value.preview.as_mut().unwrap().alt = "Cover".into();
    value.preview.as_mut().unwrap().url = "http://127.0.0.1/cover".into();
    assert_eq!(normalize(value).unwrap_err().code, Code::UnsafeUrl);
    let raw = json!({"url":"https://example.test/cover.png"});
    assert!(serde_json::from_value::<ResourcePreview>(raw).is_err());
}

#[test]
fn a12_duplicate_targets_warn_without_merging_and_clones_share_storage() {
    let one = resource();
    let mut two = one.clone();
    two.id = "second".into();
    let collection = ExternalResourceCollection::new(
        vec![(one, source("one.yaml")), (two, source("two.yaml"))],
        &ResourceLimits::default(),
        &PublicationLimits::default(),
    )
    .unwrap();
    assert_eq!(collection.len(), 2);
    assert_eq!(collection.duplicate_targets().len(), 1);
    let copy = collection.clone();
    assert!(Arc::ptr_eq(
        &copy.records()[0].resource,
        &collection.records()[0].resource
    ));
}

#[test]
fn i01_private_unsafe_and_ambiguous_urls_are_rejected_without_dns() {
    for url in [
        "javascript:alert(1)",
        "data:text/html,no",
        "file:///private/key",
        "/etc/passwd",
        "https://user:pass@example.test/",
        "https://@example.test/",
        "http://localhost/",
        "http://foo.localhost/",
        "http://localhost./",
        "http://machine.local/",
        "http://machine/",
        "http://127.0.0.1/",
        "http://127.1/",
        "http://2130706433/",
        "http://0.0.0.0/",
        "http://10.0.0.1/",
        "http://172.16.1.1/",
        "http://192.168.1.1/",
        "http://169.254.169.254/",
        "http://100.64.1.1/",
        "http://[::1]/",
        "http://[::ffff:127.0.0.1]/",
        "http://[fc00::1]/",
        "http://[fe80::1]/",
        "https://example.test/\nfoo",
        "https://example.test/a b",
        "https:\\example.test/x",
        "https://example.test/\"quoted",
        "https://",
    ] {
        assert!(validate_resource_url(url, 8192).is_err(), "accepted {url}");
    }
    for url in [
        "https://example.test/A?q=a&b=c#X",
        "http://8.8.8.8/a",
        "https://github.com/example/repo",
        "https://例え.test/資料",
    ] {
        assert!(validate_resource_url(url, 8192).is_ok(), "rejected {url}");
    }
}

#[test]
fn p01_exact_downward_only_budgets_and_logical_source_validation() {
    let normal = normalize(resource()).unwrap();
    let mut limits = ResourceLimits {
        id_bytes: 7,
        ..Default::default()
    };
    assert!(
        resource()
            .normalized(&limits, &PublicationLimits::default())
            .is_ok()
    );
    limits.id_bytes = 6;
    assert_eq!(
        resource()
            .normalized(&limits, &PublicationLimits::default())
            .unwrap_err()
            .code,
        Code::ResourceLimit
    );
    limits = ResourceLimits {
        catalog_items: 1,
        ..Default::default()
    };
    assert!(
        ExternalResourceCollection::new(
            vec![(resource(), source("one.yaml"))],
            &limits,
            &PublicationLimits::default()
        )
        .is_ok()
    );
    let mut second = resource();
    second.id = "two".into();
    assert!(
        ExternalResourceCollection::new(
            vec![
                (resource(), source("one.yaml")),
                (second, source("two.yaml"))
            ],
            &limits,
            &PublicationLimits::default()
        )
        .is_err()
    );
    limits.catalog_items = 10_001;
    assert!(limits.validate().is_err());
    let max = [&normal.id, &normal.url, &normal.title]
        .map(String::len)
        .into_iter()
        .max()
        .unwrap();
    let mut publication = PublicationLimits {
        string_bytes: max,
        ..Default::default()
    };
    assert!(
        resource()
            .normalized(&ResourceLimits::default(), &publication)
            .is_ok()
    );
    publication.string_bytes = max - 1;
    assert!(
        resource()
            .normalized(&ResourceLimits::default(), &publication)
            .is_err()
    );
    for path in [
        "/private/secret",
        "../secret",
        "a/../secret",
        "C:/private",
        "a\\b",
        "a//b",
        "./a",
    ] {
        assert!(source(path).validate().is_err());
    }
    assert!(source("data/reading.yaml").validate().is_ok());
    let raw: Value =
        serde_json::to_value(ResourceRef::new(ResourceId::external("logging").unwrap())).unwrap();
    assert_eq!(raw["id"], "external:logging");
}

#[test]
fn p01_exact_url_topic_and_collection_storage_boundaries() {
    let value = normalize(resource()).unwrap();
    let mut limits = ResourceLimits {
        url_bytes: value.url.len(),
        ..Default::default()
    };
    assert!(
        resource()
            .normalized(&limits, &PublicationLimits::default())
            .is_ok()
    );
    limits.url_bytes -= 1;
    assert_eq!(
        resource()
            .normalized(&limits, &PublicationLimits::default())
            .unwrap_err()
            .code,
        Code::ResourceLimit
    );
    limits = ResourceLimits {
        topics_per_resource: 3,
        ..Default::default()
    };
    assert!(
        resource()
            .normalized(&limits, &PublicationLimits::default())
            .is_ok()
    );
    limits.topics_per_resource = 2;
    assert!(
        resource()
            .normalized(&limits, &PublicationLimits::default())
            .is_err()
    );
    let evidence = source("one.yaml");
    let bytes = canonical_bytes(&(&value, &evidence)).unwrap().len();
    limits = ResourceLimits {
        descriptor_bytes: bytes,
        ..Default::default()
    };
    assert!(
        ExternalResourceCollection::new(
            vec![(resource(), evidence.clone())],
            &limits,
            &PublicationLimits::default()
        )
        .is_ok()
    );
    limits.descriptor_bytes -= 1;
    assert!(
        ExternalResourceCollection::new(
            vec![(resource(), evidence)],
            &limits,
            &PublicationLimits::default()
        )
        .is_err()
    );
    assert_eq!(topic_key(" RUST ").unwrap(), "rust");
    assert_ne!(topic_key("ÄRUST").unwrap(), topic_key("ärust").unwrap());
    for topic in ["", "\n", "bad\u{0000}topic"] {
        assert!(topic_key(topic).is_err());
    }
}

#[test]
fn p01_duplicate_warning_order_is_independent_of_input_order() {
    let a = resource();
    let mut b = a.clone();
    b.id = "second".into();
    let records = vec![(a, source("a.yaml")), (b, source("b.yaml"))];
    let forward = ExternalResourceCollection::new(
        records.clone(),
        &ResourceLimits::default(),
        &PublicationLimits::default(),
    )
    .unwrap();
    let reverse = ExternalResourceCollection::new(
        records.into_iter().rev().collect(),
        &ResourceLimits::default(),
        &PublicationLimits::default(),
    )
    .unwrap();
    assert_eq!(forward.duplicate_targets(), reverse.duplicate_targets());
}
