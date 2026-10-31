use std::collections::BTreeMap;
use std::io::Write;

use berlin_publication::*;

fn call(name: &str) -> ComponentCall {
    ComponentCall::new(format!("berlin.{name}@1").parse().unwrap())
}
fn spec() -> PublicationSpec {
    let mut root = call("portfolio");
    root.slots.insert(
        "body".into(),
        ViewFragment(vec![call("introduction").keyed("opening")]),
    );
    PublicationSpec {
        schema_version: 1,
        metadata: PublicationMetadata {
            id: PublicationId("portfolio α".into()),
            title: "Work".into(),
            description: None,
        },
        root,
    }
}

#[test]
fn a01_round_trip_and_canonical_encoding() {
    let first = spec();
    let bytes = canonical_bytes(&first).unwrap();
    let second: PublicationSpec = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first.normalized_digest().unwrap(),
        second.normalized_digest().unwrap()
    );
    assert_eq!(
        canonical_bytes(&serde_json::json!({"z":1,"a":{"c":2,"b":3}})).unwrap(),
        br#"{"a":{"b":3,"c":2},"z":1}"#
    );
    assert!(canonical_bytes(&1.5f64).is_err());
    assert_ne!(
        fingerprint("spec/v1", &first).unwrap(),
        fingerprint("composition/v1", &first).unwrap()
    );
    assert_ne!(
        fingerprint("text/v1", &"é").unwrap(),
        fingerprint("text/v1", &"e\u{301}").unwrap()
    );
}

#[test]
fn a02_versions_and_identifiers() {
    let mut s = spec();
    s.schema_version = 2;
    assert_eq!(
        s.occurrences(&PublicationLimits::default())
            .unwrap_err()
            .code,
        Code::UnknownVersion
    );
    s.schema_version = 1;
    for id in ["", " ", "bad\nidentity"] {
        s.metadata.id = PublicationId(id.into());
        assert_eq!(
            s.occurrences(&PublicationLimits::default())
                .unwrap_err()
                .code,
            Code::InvalidIdentifier
        );
    }
    for key in [
        "Portfolio",
        "berlin.portfolio@0",
        "../x@1",
        "berlin.portfolio@01",
        "berlin.X@1",
    ] {
        assert!(key.parse::<ComponentKey>().is_err(), "{key}");
    }
    // Unknown but syntactically valid versions are retained for the registry to
    // reject: parsing is not a claim that an implementation exists.
    assert_eq!(
        "berlin.portfolio@2"
            .parse::<ComponentKey>()
            .unwrap()
            .version(),
        2
    );
}

#[test]
fn a04_occurrence_keys_are_unique_and_stable_across_reordering() {
    let mut s = spec();
    let limits = PublicationLimits::default();
    let before = s.occurrences(&limits).unwrap();
    s.root
        .slots
        .get_mut("body")
        .unwrap()
        .0
        .insert(0, call("introduction"));
    let after = s.occurrences(&limits).unwrap();
    assert_eq!(before[1], after[2]);
    assert_ne!(after[1].id, after[2].id);
    s.root
        .slots
        .get_mut("body")
        .unwrap()
        .0
        .push(call("introduction").keyed("opening"));
    let error = s.occurrences(&limits).unwrap_err();
    assert_eq!(error.code, Code::DuplicateOccurrenceKey);
    assert_eq!(error.publication_id.as_deref(), Some("portfolio α"));
    // Arbitrary punctuation is length-delimited, never concatenated ambiguously.
    let mut a = spec();
    a.root.slots.insert(
        "body/x".into(),
        ViewFragment(vec![call("introduction").keyed("y")]),
    );
    let ids = a.occurrences(&limits).unwrap();
    assert_ne!(ids[1].id, ids[2].id);
}

#[test]
fn f17_each_configured_limit_rejects_values_above_supported_maximum() {
    let max = serde_json::to_value(PublicationLimits::default()).unwrap();
    for (name, value) in max.as_object().unwrap() {
        for invalid in [0, value.as_u64().unwrap() + 1] {
            let mut changed = max.clone();
            changed[name] = invalid.into();
            let limits: PublicationLimits = serde_json::from_value(changed).unwrap();
            assert_eq!(
                limits.validate().unwrap_err().code,
                Code::ResourceLimit,
                "{name}"
            );
        }
    }
    let mut limits = PublicationLimits {
        occurrences: 2,
        depth: 2,
        ..Default::default()
    };
    assert_eq!(spec().occurrences(&limits).unwrap().len(), 2);
    limits.occurrences = 1;
    assert!(spec().occurrences(&limits).is_err());
    limits.occurrences = 2;
    limits.depth = 1;
    assert!(spec().occurrences(&limits).is_err());
    let mut s = spec();
    s.root.properties = BTreeMap::from([("text".into(), Property::Text("12345".into()))]);
    assert!(
        s.occurrences(&PublicationLimits {
            string_bytes: 4,
            ..Default::default()
        })
        .is_err()
    );
    assert!(
        s.occurrences(&PublicationLimits {
            property_bytes: 4,
            ..Default::default()
        })
        .is_err()
    );
}

#[test]
fn f19_bounded_sink_stops_before_growth_and_propagates_write_failure() {
    let mut sink = BoundedWriter::new(4);
    sink.write_all(b"1234").unwrap();
    assert!(sink.write_all(b"5").is_err());
    assert_eq!(sink.len(), 4);
    assert_eq!(sink.into_bytes(), b"1234");
    let mut sink = BoundedWriter::new(3);
    assert!(sink.write_all(b"1234").is_err());
    assert!(sink.is_empty());
}

#[test]
fn f17_structural_byte_and_slot_boundaries_are_exact() {
    let mut s = spec();
    // Occurrence accounting includes metadata, property names and slot names.
    let (_, bytes) = s.root.measure(&PublicationLimits::default()).unwrap();
    assert!(
        s.root
            .measure(&PublicationLimits {
                property_bytes: bytes,
                ..Default::default()
            })
            .is_ok()
    );
    assert!(
        s.root
            .measure(&PublicationLimits {
                property_bytes: bytes - 1,
                ..Default::default()
            })
            .is_err()
    );
    s.root.slots.insert("aside".into(), ViewFragment::default());
    assert!(
        s.root
            .measure(&PublicationLimits {
                slots: 2,
                ..Default::default()
            })
            .is_ok()
    );
    assert!(
        s.root
            .measure(&PublicationLimits {
                slots: 1,
                ..Default::default()
            })
            .is_err()
    );
    s.root
        .properties
        .insert("text".into(), Property::Text("x".repeat(20)));
    assert!(
        s.root
            .measure(&PublicationLimits {
                string_bytes: 20,
                ..Default::default()
            })
            .is_ok()
    );
    assert!(
        s.root
            .measure(&PublicationLimits {
                string_bytes: 19,
                ..Default::default()
            })
            .is_err()
    );
}

#[test]
fn primitive_storage_accounting_does_not_consume_the_per_string_limit() {
    let limits = PublicationLimits {
        string_bytes: 1,
        property_bytes: 9,
        ..Default::default()
    };
    for value in [
        Property::Boolean(true),
        Property::Integer(42),
        Property::None,
    ] {
        let mut call = ComponentCall::new("x.c@1".parse().unwrap());
        call.properties.insert("b".into(), value);
        assert_eq!(call.measure(&limits).unwrap(), (1, 9));
        assert!(
            call.measure(&PublicationLimits {
                property_bytes: 8,
                ..limits.clone()
            })
            .is_err()
        );
    }
}
