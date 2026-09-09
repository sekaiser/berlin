use super::*;
use berlin_content::{DocumentCollection, Feed};
use berlin_document::{Inline, Metadata, Provenance, SourceFormat};

fn document() -> Document {
    Document {
        id: ContentId("note".into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some("Note".into()),
            ..Metadata::default()
        },
        blocks: vec![Block::Paragraph {
            content: vec![Inline::Text {
                value: "First.".into(),
            }],
        }],
        relations: vec![],
        provenance: Provenance {
            source: "file:///private/content/note.md".into(),
            source_format: SourceFormat::Markdown,
            source_hash: "a".repeat(64),
        },
    }
}

#[test]
fn version_one_fingerprints_have_a_fixed_serialization_contract() {
    let revision = DocumentRevision::from_document(&document()).unwrap();
    assert_eq!(revision.content_sha256, digest(br#"{"kind":"article","blocks":[{"type":"paragraph","content":[{"type":"text","value":"First."}]}],"relations":[]}"#));
    assert_eq!(revision.metadata_sha256, digest(br#"{"preview":null,"title":"Note","slug":null,"previous_slugs":[],"authors":[],"description":null,"published":null,"modified":null,"tags":[],"draft":false,"comments":false}"#));
}

#[test]
fn source_provenance_is_not_part_of_either_fingerprint() {
    let mut document = document();
    let before = DocumentRevision::from_document(&document).unwrap();
    document.provenance = Provenance {
        source: "file:///another-machine/renamed.org".into(),
        source_format: SourceFormat::Org,
        source_hash: "b".repeat(64),
    };
    assert_eq!(DocumentRevision::from_document(&document).unwrap(), before);
}

#[test]
fn content_fingerprint_includes_structure_kind_and_relations_but_not_metadata() {
    let before = DocumentRevision::from_document(&document()).unwrap();
    for mutate in [
        |document: &mut Document| document.blocks.push(Block::ThematicBreak),
        |document: &mut Document| document.kind = DocumentKind::Guide,
        |document: &mut Document| {
            document.relations.push(Relation {
                kind: "related".into(),
                target: ContentId("other".into()),
            })
        },
    ] {
        let mut changed = document();
        mutate(&mut changed);
        let after = DocumentRevision::from_document(&changed).unwrap();
        assert_ne!(before.content_sha256, after.content_sha256);
        assert_eq!(before.metadata_sha256, after.metadata_sha256);
    }
    let mut changed = document();
    changed.metadata.modified = Some("2026-09-09".parse().unwrap());
    let after = DocumentRevision::from_document(&changed).unwrap();
    assert_eq!(before.content_sha256, after.content_sha256);
    assert_ne!(before.metadata_sha256, after.metadata_sha256);
}

#[test]
fn inventory_uses_only_public_documents_in_stable_id_order() {
    let first = document();
    let mut second = first.clone();
    second.id.0 = "a-note".into();
    let mut draft = first.clone();
    draft.id.0 = "private-id".into();
    draft.metadata.draft = true;
    let website = WebsiteAssembly::new(
        DocumentCollection::new(vec![first, draft, second]),
        Feed::default(),
    )
    .unwrap();
    let inventory = DocumentInventory::from_website(&website).unwrap();
    assert_eq!(
        inventory
            .entries
            .iter()
            .map(|entry| entry.id.0.as_str())
            .collect::<Vec<_>>(),
        ["a-note", "note"]
    );
    let json = serde_json::to_string(&inventory).unwrap();
    for private in ["private-id", "file:///", "First.", "source_hash"] {
        assert!(!json.contains(private));
    }
}

#[test]
fn rejects_unsupported_fingerprints_duplicate_ids_and_unbound_paths() {
    let inventory = DocumentInventory {
        fingerprint_version: 1,
        entries: vec![DocumentRevision::from_document(&document()).unwrap()],
    };
    let files = vec![ReleaseFile {
        path: "notes/note.html".into(),
        bytes: 1,
        sha256: "0".repeat(64),
    }];
    inventory.validate(&files).unwrap();
    for mutate in [
        |inventory: &mut DocumentInventory| inventory.fingerprint_version = 2,
        |inventory: &mut DocumentInventory| inventory.entries.push(inventory.entries[0].clone()),
        |inventory: &mut DocumentInventory| inventory.entries[0].path = "../source.org".into(),
        |inventory: &mut DocumentInventory| inventory.entries[0].id.0 = "\n".into(),
        |inventory: &mut DocumentInventory| {
            inventory.entries[0].content_sha256 = "not-a-digest".into()
        },
        |inventory: &mut DocumentInventory| inventory.entries[0].metadata_sha256 = "A".repeat(64),
    ] {
        let mut invalid = inventory.clone();
        mutate(&mut invalid);
        assert!(invalid.validate(&files).is_err());
    }
}

#[test]
fn empty_inventory_is_known_empty_not_missing() {
    let empty = DocumentInventory {
        fingerprint_version: 1,
        entries: vec![],
    };
    assert!(
        matches!(DocumentComparison::between(Some(&empty), Some(&empty)), DocumentComparison::Compared { changes } if changes.unchanged == 0 && changes.added.is_empty())
    );
    assert!(matches!(
        DocumentComparison::between(None, Some(&empty)),
        DocumentComparison::Unavailable {
            before_available: false,
            after_available: true
        }
    ));
}
