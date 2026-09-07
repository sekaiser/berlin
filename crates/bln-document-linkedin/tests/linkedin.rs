use berlin_document::ComponentId;
use berlin_document::ContentId;
use berlin_document::DocumentKind;
use berlin_document::Metadata;
use berlin_document::Provenance;
use berlin_document::SourceFormat;

use berlin_content::DocumentCollection;
use berlin_document::{Block, Document, Inline, List, ListItem};
use berlin_document_linkedin::{LINKEDIN_POST_CHARACTER_LIMIT, LinkedInDraftDiagnostic, Renderer};

fn document(id: &str, draft: bool) -> Document {
    Document {
        id: ContentId(id.into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some("A semantic article".into()),
            draft,
            ..Metadata::default()
        },
        blocks: vec![
            Block::Section {
                id: ComponentId("opening".into()),
                level: 2,
                role: Some("lead".into()),
                title: vec![Inline::Text {
                    value: "Why this matters".into(),
                }],
                blocks: vec![Block::Paragraph {
                    content: vec![
                        Inline::Text {
                            value: "Read ".into(),
                        },
                        Inline::Link {
                            destination: "https://example.com".into(),
                            title: None,
                            content: vec![Inline::Strong {
                                content: vec![Inline::Text {
                                    value: "the source".into(),
                                }],
                            }],
                        },
                    ],
                }],
            },
            Block::List(List {
                ordered: false,
                start: None,
                tight: true,
                items: vec![ListItem {
                    checked: Some(true),
                    blocks: vec![Block::Paragraph {
                        content: vec![Inline::Text {
                            value: "Ship a draft".into(),
                        }],
                    }],
                }],
            }),
        ],
        relations: Vec::new(),
        provenance: Provenance {
            source: format!("file:///{id}.md"),
            source_format: SourceFormat::Markdown,
            source_hash: "0".repeat(64),
        },
    }
}

#[test]
fn projects_semantics_to_reviewable_linkedin_text() {
    let draft = Renderer::default().render(&document("article", false));

    assert_eq!(draft.source.0.0, "article");
    assert_eq!(draft.artifact_name, "article");
    assert_eq!(
        draft.text,
        "A semantic article\n\nWhy this matters\n\nRead the source (https://example.com)\n\n• \
         [x] Ship a draft"
    );
    assert_eq!(draft.character_count, draft.text.chars().count());
    assert!(draft.diagnostics.is_empty());
}

#[test]
fn collection_projection_excludes_unpublished_drafts() {
    let documents =
        DocumentCollection::new(vec![document("public", false), document("draft", true)]);

    let drafts = Renderer::default().render_collection(&documents);

    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts.as_slice()[0].source.0.0, "public");
}

#[test]
fn reports_content_that_cannot_be_published_as_a_feed_post() {
    let mut document = document("too-long", false);
    document.blocks = vec![Block::Paragraph {
        content: vec![Inline::Text {
            value: "x".repeat(LINKEDIN_POST_CHARACTER_LIMIT + 1),
        }],
    }];
    document.metadata.title = None;

    let draft = Renderer::default().render(&document);

    assert_eq!(
        draft.diagnostics,
        [LinkedInDraftDiagnostic::CharacterLimitExceeded {
            limit: LINKEDIN_POST_CHARACTER_LIMIT,
            actual: LINKEDIN_POST_CHARACTER_LIMIT + 1,
        }]
    );
}

#[test]
fn derives_portable_artifact_names_from_source_provenance() {
    let renderer = Renderer::default();
    let artifact_name = |source: &str| {
        let mut document = document("article", false);
        document.provenance.source = source.into();
        renderer.render(&document).artifact_name
    };
    assert_eq!(
        artifact_name("file:///Users/person/site/content/notes/my-post.md"),
        "my-post"
    );
    assert_eq!(
        artifact_name("https://example.com/a%20post.md?v=1"),
        "a-20post"
    );
    assert_eq!(artifact_name("file:///"), "document");
}
