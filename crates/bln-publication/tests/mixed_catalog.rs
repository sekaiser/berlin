use berlin_content::DocumentCollection;
use berlin_document::{
    Block, ContentId, Document, DocumentKind, Inline, Metadata, Provenance, SourceFormat,
};
use berlin_publication::*;
use serde_json::json;
use std::sync::Arc;

fn doc(id: &str) -> Document {
    Document {
        id: ContentId(id.into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some(format!("Article {id}")),
            slug: Some(format!("work-{id}")),
            tags: vec!["Storage".into()],
            ..Default::default()
        },
        blocks: vec![],
        relations: vec![],
        provenance: Provenance {
            source: format!("file:///private/authored/{id}.md"),
            source_format: SourceFormat::Markdown,
            source_hash: "a".repeat(64),
        },
    }
}
fn external(id: &str) -> (ExternalResource, ResourceSource) {
    (serde_json::from_value(json!({"id":id,"title":"Linked article","kind":"article","url":format!("https://example.test/{id}?a=1&b=2#Part"),"topics":["storage"],"authors":["Another author"]})).unwrap(),
    ResourceSource {namespace:"reading".into(),logical_name:format!("resources/{id}.yaml"),sha256:"b".repeat(64),record:1})
}
fn collection(records: Vec<(ExternalResource, ResourceSource)>) -> ExternalResourceCollection {
    ExternalResourceCollection::new(
        records,
        &ResourceLimits::default(),
        &PublicationLimits::default(),
    )
    .unwrap()
}

#[test]
fn c12_local_document_cites_external_resource_without_a_synthetic_document() {
    let mut local = doc("a");
    local.blocks = vec![Block::Section {
        id: berlin_document::ComponentId("argument".into()),
        level: 2,
        role: None,
        title: vec![Inline::Text {
            value: "Argument".into(),
        }],
        blocks: vec![Block::Paragraph {
            content: vec![Inline::Link {
                destination: "https://example.test/paper".into(),
                title: None,
                content: vec![Inline::Text {
                    value: "Supporting paper".into(),
                }],
            }],
        }],
    }];
    let mut paper = external("paper");
    paper.0.kind = "paper".parse().unwrap();
    paper.0.url = "https://example.test/paper".into();
    let catalog = mixed(vec![local], collection(vec![paper]));
    let section = catalog.section_ref("a", "argument").unwrap();
    let root = view! {Portfolio(){
        Excerpt(source:section)
        Citation(resource:ResourceRef::external("paper").unwrap())
    }};
    let assembly = PublicationAssembly::resolve(
        &PublicationSpec::new(
            PublicationMetadata {
                id: PublicationId("argument".into()),
                title: "Argument".into(),
                description: None,
            },
            root,
        )
        .unwrap(),
        catalog,
        &ComponentRegistry::builtins(),
    )
    .unwrap();
    assert_eq!(
        assembly.catalog().by_origin(ResourceOrigin::Local).count(),
        1
    );
    assert_eq!(
        assembly
            .catalog()
            .by_origin(ResourceOrigin::External)
            .count(),
        1
    );
    assert_eq!(assembly.sources().len(), 2);
    assert!(
        assembly
            .catalog()
            .get(&ResourceId::external("paper").unwrap())
            .unwrap()
            .handle()
            .try_local_document()
            .is_err()
    );
}

#[test]
fn catalog_retained_urls_are_bounded_before_indexing() {
    let mut opts = options();
    opts.resource_limits.url_bytes = 20;
    assert_eq!(
        PublicationCatalog::with_resources(
            DocumentCollection::new(vec![doc("a")]),
            ExternalResourceCollection::default(),
            opts
        )
        .unwrap_err()
        .code,
        Code::ResourceLimit
    );
    let mut opts = options();
    opts.resource_limits.url_bytes = 70;
    let mapping = std::collections::BTreeMap::from([(
        "a".into(),
        format!("https://example.test/{}", "x".repeat(80))
            .parse()
            .unwrap(),
    )]);
    assert_eq!(
        PublicationCatalog::with_resource_urls(
            DocumentCollection::new(vec![doc("a")]),
            ExternalResourceCollection::default(),
            opts,
            mapping
        )
        .unwrap_err()
        .code,
        Code::ResourceLimit
    );
}
fn options() -> PublicationOptions {
    PublicationOptions {
        source_site_url: Some("https://example.test/notebook".parse().unwrap()),
        ..Default::default()
    }
}
fn mixed(docs: Vec<Document>, external: ExternalResourceCollection) -> PublicationCatalog {
    PublicationCatalog::with_resources(DocumentCollection::new(docs), external, options()).unwrap()
}

#[test]
fn c01_c02_one_catalog_has_disjoint_ids_and_consistent_indexes() {
    let catalog = mixed(vec![doc("same")], collection(vec![external("same")]));
    assert_eq!(catalog.len(), 2);
    assert_eq!(catalog.by_kind(&"article".parse().unwrap()).count(), 2);
    assert_eq!(
        catalog
            .by_origin(ResourceOrigin::Local)
            .cloned()
            .collect::<Vec<_>>(),
        vec![ResourceId::local("same").unwrap()]
    );
    assert_eq!(catalog.by_origin(ResourceOrigin::External).count(), 1);
    assert_eq!(catalog.by_topic(" STORAGE ").unwrap().count(), 2);
    assert_eq!(catalog.by_topic("missing").unwrap().count(), 0);
    for entry in catalog.resources() {
        assert_eq!(
            catalog.get(&entry.descriptor().id).unwrap().descriptor(),
            entry.descriptor()
        );
        assert!(
            catalog
                .by_kind(&entry.descriptor().kind)
                .any(|id| id == &entry.descriptor().id)
        );
    }
}

#[test]
fn c03_c10_independent_input_permutation_preserves_versioned_catalog_digest() {
    let docs = vec![doc("b"), doc("a")];
    let records = vec![external("b"), external("a")];
    let a = mixed(docs.clone(), collection(records.clone()));
    let b = mixed(
        docs.into_iter().rev().collect(),
        collection(records.into_iter().rev().collect()),
    );
    assert_eq!(a.input_digest(), b.input_digest());
    assert_eq!(a.original_urls_digest(), b.original_urls_digest());
    let entries = a
        .resources()
        .map(|e| (e.descriptor(), e.source()))
        .collect::<Vec<_>>();
    assert_eq!(
        a.input_digest(),
        fingerprint("resource-catalog/v1", &(entries, a.input_sources())).unwrap()
    );
    let mut changed = external("b");
    changed.0.url = "https://example.test/changed".into();
    let changed = mixed(
        vec![doc("a"), doc("b")],
        collection(vec![external("a"), changed]),
    );
    assert_ne!(a.input_digest(), changed.input_digest());
}

#[test]
fn c04_repeated_references_share_local_and_external_source_storage() {
    let external = collection(vec![external("same")]);
    let catalog = mixed(vec![doc("same")], external.clone());
    let copy = catalog.clone();
    assert!(std::ptr::eq(
        catalog.entry("same").unwrap().document(),
        copy.entry("same").unwrap().document()
    ));
    let entry = catalog.get(&ResourceId::external("same").unwrap()).unwrap();
    let ResourceHandle::External(record) = entry.handle() else {
        panic!("not external")
    };
    assert!(Arc::ptr_eq(
        &record.resource,
        &external.records()[0].resource
    ));
    let second = copy.resolve_resource(&entry.reference()).unwrap();
    assert!(std::ptr::eq(entry, second));
    assert_eq!(catalog.index_builds(), 1);
}

#[test]
fn runtime_catalog_combines_shared_batches_without_copying_document_or_resource_payloads() {
    let documents = Arc::new(DocumentCollection::new(vec![doc("a")]));
    let first = collection(vec![external("x")]);
    let second = collection(vec![external("y")]);
    let merged = ExternalResourceCollection::merge(
        &[first.clone(), second],
        &ResourceLimits::default(),
        &PublicationLimits::default(),
    )
    .unwrap();
    assert!(Arc::ptr_eq(
        &first.records()[0].resource,
        &merged.records()[0].resource
    ));
    let catalog = PublicationCatalog::with_document_batches(
        vec![documents.clone()],
        merged,
        options(),
        Default::default(),
    )
    .unwrap();
    assert!(std::ptr::eq(
        &documents.as_slice()[0],
        catalog.entry("a").unwrap().document()
    ));
    assert_eq!(catalog.len(), 3);
    assert_eq!(
        ExternalResourceCollection::merge(
            &[first.clone(), first],
            &ResourceLimits::default(),
            &PublicationLimits::default()
        )
        .unwrap_err()
        .code,
        Code::DuplicateResource
    );
}

#[test]
fn c05_c08_catalog_shapes_and_external_targets() {
    assert!(mixed(vec![], ExternalResourceCollection::default()).is_empty());
    assert_eq!(
        mixed(vec![doc("a")], ExternalResourceCollection::default()).len(),
        1
    );
    let catalog = mixed(vec![], collection(vec![external("a")]));
    assert_eq!(catalog.len(), 1);
    let resource = catalog.get(&ResourceId::external("a").unwrap()).unwrap();
    assert_eq!(
        resource.original_url(),
        Some("https://example.test/a?a=1&b=2#Part")
    );
    assert!(resource.handle().try_local_document().is_err());
    assert!(
        !resource
            .descriptor()
            .capabilities
            .contains(&ResourceCapability::Excerptable)
    );
    assert!(
        !resource
            .descriptor()
            .capabilities
            .contains(&ResourceCapability::HasSemanticBody)
    );
}

#[test]
fn a09_c09_drafts_and_host_paths_are_absent_from_eligible_metadata() {
    let mut draft = doc("draft");
    draft.metadata.draft = true;
    draft.metadata.title = Some("PRIVATE_TITLE_SENTINEL".into());
    draft.blocks = vec![Block::Paragraph {
        content: vec![Inline::Text {
            value: "PRIVATE_BODY_SENTINEL".into(),
        }],
    }];
    let catalog = mixed(
        vec![doc("a"), draft.clone()],
        collection(vec![external("x")]),
    );
    assert_eq!(catalog.len(), 2);
    let public = String::from_utf8(
        canonical_bytes(
            &catalog
                .resources()
                .map(|e| (e.descriptor(), e.source()))
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )
    .unwrap();
    for secret in [
        "PRIVATE_TITLE_SENTINEL",
        "PRIVATE_BODY_SENTINEL",
        "/private/",
        "file://",
        "local:draft",
    ] {
        assert!(!public.contains(secret));
    }
    let error = catalog
        .get(&ResourceId::local("draft").unwrap())
        .unwrap_err();
    assert_eq!(error.code, Code::DraftSelection);
    assert!(!error.to_string().contains("PRIVATE_"));
    draft.blocks.clear();
    draft.metadata.title = Some("OTHER_PRIVATE_TITLE".into());
    assert_eq!(
        catalog.input_digest(),
        mixed(vec![doc("a"), draft], collection(vec![external("x")])).input_digest()
    );
}

#[test]
fn c06_c07_full_original_link_scope_and_missing_mount_diagnostics() {
    let catalog = mixed(vec![doc("a"), doc("b")], collection(vec![external("x")]));
    assert_eq!(
        catalog
            .entry("b")
            .unwrap()
            .original_url
            .as_ref()
            .unwrap()
            .as_str(),
        "https://example.test/notebook/notes/work-b.html"
    );
    let no_mapping = PublicationCatalog::with_resources(
        DocumentCollection::new(vec![doc("a")]),
        ExternalResourceCollection::default(),
        PublicationOptions::default(),
    )
    .unwrap();
    let source = FeaturedWork::builder()
        .source(ResourceRef::local("a").unwrap())
        .build();
    let spec = PublicationSpec::new(
        PublicationMetadata {
            id: PublicationId("p".into()),
            title: "Portfolio".into(),
            description: None,
        },
        Portfolio::builder().body(source).build(),
    )
    .unwrap();
    assert_eq!(
        PublicationAssembly::resolve(&spec, no_mapping, &ComponentRegistry::builtins())
            .unwrap_err()
            .code,
        Code::UnsafeUrl
    );
}

#[test]
fn c11_catalog_count_and_canonical_descriptor_limits_are_exact() {
    let catalog = mixed(vec![doc("a")], collection(vec![external("x")]));
    let bytes = catalog
        .resources()
        .map(|e| {
            let retained = match e.handle() {
                ResourceHandle::Local(_) => {
                    canonical_bytes(&e.original_url().unwrap()).unwrap().len()
                }
                ResourceHandle::External(record) => {
                    canonical_bytes(record.resource.as_ref()).unwrap().len()
                }
            };
            retained
                + canonical_bytes(&(e.descriptor(), e.source()))
                    .unwrap()
                    .len()
        })
        .sum::<usize>()
        + catalog
            .input_sources()
            .iter()
            .map(|s| canonical_bytes(s).unwrap().len())
            .sum::<usize>();
    let mut opts = options();
    opts.resource_limits.catalog_items = 2;
    opts.resource_limits.descriptor_bytes = bytes;
    assert!(
        PublicationCatalog::with_resources(
            DocumentCollection::new(vec![doc("a")]),
            collection(vec![external("x")]),
            opts.clone()
        )
        .is_ok()
    );
    opts.resource_limits.descriptor_bytes -= 1;
    assert_eq!(
        PublicationCatalog::with_resources(
            DocumentCollection::new(vec![doc("a")]),
            collection(vec![external("x")]),
            opts.clone()
        )
        .unwrap_err()
        .code,
        Code::ResourceLimit
    );
    opts.resource_limits.descriptor_bytes = bytes;
    opts.resource_limits.catalog_items = 1;
    assert!(
        PublicationCatalog::with_resources(
            DocumentCollection::new(vec![doc("a")]),
            collection(vec![external("x")]),
            opts.clone()
        )
        .is_err()
    );
    assert!(
        PublicationCatalog::with_resources(
            DocumentCollection::default(),
            collection(vec![external("x"), external("y")]),
            opts
        )
        .is_err()
    );
    let mut opts = options();
    opts.limits.string_bytes = 4;
    assert!(
        PublicationCatalog::with_resources(
            DocumentCollection::default(),
            collection(vec![external("x")]),
            opts
        )
        .is_err()
    );
}

#[test]
fn c05_c10_empty_resource_inputs_retain_their_consumed_evidence() {
    let input = ResourceInputEvidence {
        namespace: "reading".into(),
        logical_name: "resources/empty.yaml".into(),
        sha256: "a".repeat(64),
    };
    let empty = ExternalResourceCollection::default()
        .with_input_sources(vec![input.clone()], &ResourceLimits::default())
        .unwrap();
    let a = mixed(vec![], empty);
    assert!(a.is_empty());
    assert_eq!(a.input_sources(), std::slice::from_ref(&input));
    let mut changed = input;
    changed.sha256 = "b".repeat(64);
    let b = mixed(
        vec![],
        ExternalResourceCollection::default()
            .with_input_sources(vec![changed], &ResourceLimits::default())
            .unwrap(),
    );
    assert_ne!(a.input_digest(), b.input_digest());
    assert_ne!(
        a.input_digest(),
        mixed(vec![], ExternalResourceCollection::default()).input_digest()
    );
}

#[test]
fn c07_unselected_original_link_without_mapping_fails_explicitly() {
    use berlin_document::{ComponentId, DocumentLink};
    use std::collections::BTreeMap;
    let mut a = doc("a");
    a.blocks = vec![Block::Section {
        id: ComponentId("selected".into()),
        level: 2,
        role: None,
        title: vec![],
        blocks: vec![Block::Paragraph {
            content: vec![Inline::DocumentLink(DocumentLink {
                target: ContentId("b".into()),
                fragment: None,
                anchor: ComponentId("link-b".into()),
                title: None,
                content: vec![Inline::Text {
                    value: "Read the other article".into(),
                }],
            })],
        }],
    }];
    let catalog = PublicationCatalog::with_resource_urls(
        DocumentCollection::new(vec![a.clone(), doc("b")]),
        ExternalResourceCollection::default(),
        PublicationOptions::default(),
        BTreeMap::from([(
            "a".into(),
            "https://example.test/original/a".parse().unwrap(),
        )]),
    )
    .unwrap();
    let spec = PublicationSpec::new(
        PublicationMetadata {
            id: PublicationId("p".into()),
            title: "Excerpt".into(),
            description: None,
        },
        Portfolio::builder()
            .body(
                Excerpt::builder()
                    .source(catalog.section_ref("a", "selected").unwrap())
                    .build(),
            )
            .build(),
    )
    .unwrap();
    assert_eq!(
        PublicationAssembly::resolve(&spec, catalog, &ComponentRegistry::builtins())
            .unwrap_err()
            .code,
        Code::UnsafeUrl
    );
    let catalog = mixed(vec![a, doc("b")], ExternalResourceCollection::default());
    let assembled =
        PublicationAssembly::resolve(&spec, catalog, &ComponentRegistry::builtins()).unwrap();
    assert_eq!(assembled.placements().len(), 2);
    assert_eq!(assembled.sources().len(), 2);
}
