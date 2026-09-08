use berlin_document::{Block, Inline};
use markdown::{Parser, Source};

#[test]
fn preserves_identity_context_and_distinct_reference_locations() {
    let source = Source::new("---\nid: source\n---\nBuilds on [earlier work](id:target#details).\n\n## References\n\n- A **[second mention](id:target)**.\n\n`[not a link](id:missing)`\n", "file:///source.md").unwrap();
    let document = Parser::new().parse(&source).unwrap();
    let references = document.references();
    assert_eq!(references.len(), 2);
    assert_eq!(references[0].link.target.0, "target");
    assert_eq!(references[0].link.fragment.as_deref(), Some("details"));
    assert_eq!(references[0].excerpt, "Builds on earlier work.");
    assert_eq!(references[1].excerpt, "A second mention.");
    assert!(references[0].section.is_none());
    assert_eq!(references[1].section.unwrap().0, "references");
    assert_ne!(references[0].link.anchor, references[1].link.anchor);
    assert!(
        matches!(&document.blocks[0], Block::Paragraph { content } if matches!(&content[1], Inline::DocumentLink(_)))
    );
}

#[test]
fn upgrades_exported_relrefs_using_target_metadata_not_the_title() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target.md");
    std::fs::write(
        &target,
        "---\nid: stable-target\ntitle: Original title\n---\n",
    )
    .unwrap();
    let uri = url::Url::from_file_path(directory.path().join("source.md")).unwrap();
    let source = Source::new(
        "[Earlier work]({{< relref \"target.md#details\" >}})",
        uri.as_str(),
    )
    .unwrap();
    let first = Parser::new().parse(&source).unwrap();
    std::fs::write(&target, "---\nid: stable-target\ntitle: New title\n---\n").unwrap();
    let second = Parser::new().parse(&source).unwrap();
    assert_eq!(first.references()[0].link, second.references()[0].link);
    assert_eq!(
        first.references()[0].link.fragment.as_deref(),
        Some("details")
    );
}

#[test]
fn rejects_empty_targets_and_colliding_reference_anchors() {
    assert!(
        Parser::new()
            .parse(&Source::in_memory("[broken](id:)"))
            .is_err()
    );
    assert!(
        Parser::new()
            .parse(&Source::in_memory(
                "## Reserved {#bln-ref-1}\n\n[Link](id:target)"
            ))
            .is_err()
    );
}

#[test]
fn references_include_nested_quotes_tables_and_footnotes_but_not_code() {
    let source = Source::in_memory(
        "> A [quoted reference](id:target).\n\n| Work |\n| --- |\n| [table reference](id:target) |\n\nFootnote[^note].\n\n[^note]: A [footnote reference](id:target).\n\n```text\n[not a reference](id:missing)\n```\n",
    );
    let document = Parser::new().parse(&source).unwrap();
    let references = document.references();
    assert_eq!(references.len(), 3);
    assert!(
        references
            .iter()
            .all(|reference| reference.link.target.0 == "target")
    );
    assert!(
        references
            .iter()
            .any(|reference| reference.excerpt == "A footnote reference.")
    );
}
