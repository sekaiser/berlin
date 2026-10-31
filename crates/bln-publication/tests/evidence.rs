use berlin_content::DocumentCollection;
use berlin_publication::*;

fn catalog(extra: bool, matching_title: &str) -> PublicationCatalog {
    let mut values = vec![
        serde_json::json!({"id":"selected","kind":"paper","url":"https://example.test/paper","title":matching_title,"topics":["storage"]}),
    ];
    if extra {
        values.push(serde_json::json!({"id":"other","kind":"website","url":"https://example.test/other","title":"Not selected"}));
    }
    let records = values
        .into_iter()
        .enumerate()
        .map(|(i, value)| {
            (
                serde_json::from_value(value).unwrap(),
                ResourceSource {
                    namespace: "reading".into(),
                    logical_name: "reading.yaml".into(),
                    sha256: if extra {
                        "b".repeat(64)
                    } else {
                        "a".repeat(64)
                    },
                    record: i + 1,
                },
            )
        })
        .collect();
    PublicationCatalog::with_resources(
        DocumentCollection::default(),
        ExternalResourceCollection::new(records, &Default::default(), &Default::default()).unwrap(),
        PublicationOptions::default(),
    )
    .unwrap()
}
fn spec(selection: &ResourceSelection, catalog: &PublicationCatalog) -> PublicationSpec {
    PublicationSpec::new(
        PublicationMetadata {
            id: PublicationId("evidence".into()),
            title: "Evidence".into(),
            description: None,
        },
        view! {Portfolio(){Introduction(text:"An editorial decision.")}},
    )
    .unwrap()
    .with_selection(selection, catalog.limits())
    .unwrap()
}

#[test]
fn empty_selection_is_recorded_and_validated_without_inventing_a_placement() {
    let catalog = catalog(false, "Selected");
    let selection = catalog
        .select(&ResourceQuery {
            topics_all: vec!["missing".into()],
            ..Default::default()
        })
        .unwrap();
    let mut spec = spec(&selection, &catalog);
    let assembly =
        PublicationAssembly::resolve(&spec, catalog.clone(), &ComponentRegistry::builtins())
            .unwrap();
    let manifest = assembly
        .manifest(Channel::Text, Default::default(), "configuration".into())
        .unwrap();
    assert_eq!(manifest.queries.len(), 1);
    assert!(
        manifest
            .queries
            .values()
            .next()
            .unwrap()
            .selected
            .is_empty()
    );
    assert!(manifest.sources.is_empty());
    spec.queries.values_mut().next().unwrap().selection_digest = "a".repeat(64);
    assert_eq!(
        PublicationAssembly::resolve(&spec, catalog, &ComponentRegistry::builtins())
            .unwrap_err()
            .code,
        Code::InvalidQuery
    );
}

#[test]
fn matching_revisions_affect_composition_nonmatching_inputs_affect_catalog() {
    let query = ResourceQuery {
        topics_all: vec!["storage".into()],
        ..Default::default()
    };
    let assemble = |catalog: PublicationCatalog| {
        let selection = catalog.select(&query).unwrap();
        PublicationAssembly::resolve(
            &spec(&selection, &catalog),
            catalog,
            &ComponentRegistry::builtins(),
        )
        .unwrap()
    };
    let first = assemble(catalog(false, "Selected"));
    let extra = assemble(catalog(true, "Selected"));
    let changed = assemble(catalog(true, "Changed selected title"));
    assert_ne!(
        first.catalog().input_digest(),
        extra.catalog().input_digest()
    );
    assert_eq!(
        first.composition_fingerprint(),
        extra.composition_fingerprint()
    );
    assert_ne!(
        extra.composition_fingerprint(),
        changed.composition_fingerprint()
    );
}

#[test]
fn evidence_byte_budget_is_exact_and_failure_does_not_retain_the_entry() {
    let catalog = catalog(false, "Selected");
    let selected = catalog.select(&ResourceQuery::default()).unwrap();
    let mut table = QueryEvidenceTable::new();
    record_selection(&mut table, &selected, catalog.limits()).unwrap();
    let bytes = query_evidence_bytes(&table, catalog.limits()).unwrap();
    let mut limits = catalog.limits().clone();
    limits.property_bytes = bytes;
    assert_eq!(query_evidence_bytes(&table, &limits).unwrap(), bytes);
    limits.property_bytes -= 1;
    assert_eq!(
        query_evidence_bytes(&table, &limits).unwrap_err().code,
        Code::ResourceLimit
    );
    let mut rejected = QueryEvidenceTable::new();
    assert!(record_selection(&mut rejected, &selected, &limits).is_err());
    assert!(rejected.is_empty());
}
