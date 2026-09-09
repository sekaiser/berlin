use super::*;
use crate::tasks::release::{
    diff::ReleaseMetadata,
    documents::{DocumentChanges, ModifiedDocument},
};
use berlin_core::DeploymentTarget;
use berlin_document::ContentId;

fn revision(id: &str) -> DocumentRevision {
    DocumentRevision {
        id: ContentId(id.into()),
        title: format!("Title {id}"),
        path: format!("notes/{id}.html"),
        content_sha256: "a".repeat(64),
        metadata_sha256: "b".repeat(64),
    }
}

fn summary(id: &str, url: &str) -> ReleaseSummary {
    ReleaseSummary {
        id: id.repeat(64),
        metadata: ReleaseMetadata {
            berlin_version: "test".into(),
            pipeline: "site".into(),
            website_node: "website".into(),
            url: url.parse().unwrap(),
            destination: DeploymentTarget::GitHubPages {
                repository: "owner/site".into(),
            },
        },
    }
}

fn plan(changes: DocumentChanges) -> AnnouncementPlan {
    AnnouncementPlan::new(
        summary("a", "https://example.com/notebook"),
        summary("b", "https://example.com/notebook"),
        DocumentComparison::Compared { changes },
    )
    .unwrap()
}

#[test]
fn urls_preserve_site_prefixes_and_encode_literal_paths() {
    for base in [
        "https://example.com/notebook",
        "https://example.com/notebook/",
    ] {
        assert_eq!(
            document_url(&base.parse().unwrap(), "notes/a b#c?.html")
                .unwrap()
                .as_str(),
            "https://example.com/notebook/notes/a%20b%23c%3F.html"
        );
    }
    let base = "https://example.com/".parse().unwrap();
    assert_eq!(
        document_url(&base, "notes/100%.html").unwrap().as_str(),
        "https://example.com/notes/100%25.html"
    );
    for path in [
        "/notes/x.html",
        "../x",
        "notes/../x",
        "notes/./x",
        "//other.test/x",
        "notes\\x",
        "notes/\nx",
        "",
    ] {
        assert!(document_url(&base, path).is_err(), "{path:?}");
    }
}

#[test]
fn only_new_and_content_modified_documents_become_candidates_in_id_order() {
    let changes = DocumentChanges {
        added: vec![revision("z")],
        modified: vec![
            ModifiedDocument {
                before: revision("a"),
                after: revision("a"),
                content_changed: true,
                metadata_changed: false,
                route_changed: false,
            },
            ModifiedDocument {
                before: revision("b"),
                after: revision("b"),
                content_changed: false,
                metadata_changed: true,
                route_changed: true,
            },
        ],
        removed: vec![revision("removed")],
        unchanged: 4,
    };
    let plan = plan(changes);
    assert_eq!(
        plan.drafts
            .iter()
            .map(|item| item.document.id.0.as_str())
            .collect::<Vec<_>>(),
        ["a", "z"]
    );
    assert_eq!(plan.drafts[0].kind, AnnouncementKind::Updated);
    assert_eq!(plan.drafts[1].kind, AnnouncementKind::New);
    assert_eq!(
        plan.drafts[0].draft.text,
        "Updated: Title a\n\nhttps://example.com/notebook/notes/a.html"
    );
    assert_eq!(plan.skipped.metadata_or_route_only, 1);
    assert_eq!(plan.skipped.removed, 1);
    assert_eq!(plan.skipped.unchanged, 4);
    assert!(plan.review_required);
    assert!(!plan.live_status_checked);
}

#[test]
fn missing_history_and_cross_site_comparisons_do_not_produce_empty_successes() {
    assert!(
        AnnouncementPlan::new(
            summary("a", "https://example.com"),
            summary("b", "https://example.com"),
            DocumentComparison::Unavailable {
                before_available: false,
                after_available: true
            }
        )
        .is_err()
    );
    assert!(
        AnnouncementPlan::new(
            summary("a", "https://example.com"),
            summary("b", "https://different.test"),
            DocumentComparison::Compared {
                changes: DocumentChanges::default()
            }
        )
        .is_err()
    );
    let mut after = summary("b", "https://example.com");
    after.metadata.destination = DeploymentTarget::GitHubPages {
        repository: "owner/other".into(),
    };
    assert!(
        AnnouncementPlan::new(
            summary("a", "https://example.com"),
            after,
            DocumentComparison::Compared {
                changes: DocumentChanges::default()
            }
        )
        .is_err()
    );
}

#[test]
fn empty_comparisons_are_valid_plans_with_no_drafts() {
    let plan = plan(DocumentChanges::default());
    assert!(plan.drafts.is_empty());
}

#[test]
fn filename_collisions_are_rejected_before_creating_a_review_directory() {
    let root = tempfile::tempdir().unwrap();
    let project = Project::new(root.path().into(), vec![]);
    let mut other = revision("b");
    other.path = "other/a.html".into();
    let plan = plan(DocumentChanges {
        added: vec![revision("a"), other],
        ..DocumentChanges::default()
    });
    assert!(
        export(&project, &plan, Path::new("review"))
            .unwrap_err()
            .to_string()
            .contains("collide")
    );
    assert!(!root.path().join("review").exists());
}
