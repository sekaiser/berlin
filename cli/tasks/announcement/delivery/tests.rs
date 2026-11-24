use super::*;
use std::collections::VecDeque;
struct Fake {
    responses: VecDeque<Result<Response, &'static str>>,
    calls: Vec<String>,
}
impl Transport for Fake {
    fn request(&mut self, method: &str, path: &str, body: Option<&str>) -> Result<Response, Error> {
        self.calls.push(format!("{method} {path}"));
        if let Some(body) = body {
            let p: Value = serde_json::from_str(body)?;
            assert_eq!(p["article"]["published"], false);
        }
        self.responses
            .pop_front()
            .expect("unexpected remote call")
            .map_err(anyhow::Error::msg)
    }
}
fn response(status: u16, body: Value) -> Result<Response, &'static str> {
    Ok(Response { status, body })
}
fn fixture() -> (tempfile::TempDir, Project, Snapshot) {
    let root = tempfile::tempdir().unwrap();
    let project = Project::new(root.path().into(), vec![]);
    crate::tasks::state::migrate(&project).unwrap();
    let text=serde_json::json!({"article":{"title":"Note","body_markdown":"A worked example.","canonical_url":"https://example.test/notes/note.html","published":false,"main_image":null}}).to_string();
    let snapshot = Snapshot {
        schema_version: 2,
        channel: Channel::DevTo,
        account: "alice".into(),
        before_release: "a".repeat(64),
        after_release: "b".repeat(64),
        website_url: Url::parse("https://example.test/").unwrap(),
        website_destination: berlin_core::DeploymentTarget::GitHubPages {
            repository: "owner/site".into(),
        },
        fingerprint_version: 1,
        document: release::documents::DocumentRevision {
            id: berlin_document::ContentId("note".into()),
            title: "Note".into(),
            path: "notes/note.html".into(),
            content_sha256: "c".repeat(64),
            metadata_sha256: "d".repeat(64),
        },
        kind: AnnouncementKind::New,
        article_url: Url::parse("https://example.test/notes/note.html").unwrap(),
        text_sha256: release::digest(text.as_bytes()),
        text,
        image: None,
        linkedin: None,
        warnings: vec![],
        substack: None,
    };
    snapshot.validate().unwrap();
    super::super::store::approve(
        &project,
        &Approval {
            review_id: snapshot.id().unwrap(),
            snapshot: snapshot.clone(),
            approved_at_unix_seconds: 1,
        },
    )
    .unwrap();
    (root, project, snapshot)
}
#[test]
fn successful_draft_is_durable_and_repeated_delivery_never_posts_twice() {
    let (_root, project, snapshot) = fixture();
    let mut remote = Fake {
        responses: VecDeque::from([
            response(200, serde_json::json!({"username":"alice"})),
            response(
                201,
                serde_json::json!({"id":42,"url":"https://dev.to/alice/note","user":{"username":"alice"},"published_at":null}),
            ),
        ]),
        calls: vec![],
    };
    let first = deliver_with(&project, &snapshot, &mut remote).unwrap();
    assert_eq!(first.state, "draft_created");
    assert_eq!(first.remote_id, Some(42));
    let again = deliver_with(&project, &snapshot, &mut remote).unwrap();
    assert_eq!(again.remote_id, Some(42));
    assert_eq!(
        remote.calls,
        vec!["GET /api/users/me", "POST /api/articles"]
    );
    let db = crate::tasks::state::Store::open(project.root()).unwrap();
    assert_eq!(db.delivery_events(None).unwrap().len(), 2);
}
#[test]
fn lost_response_preserves_uncertainty_and_blocks_blind_retry() {
    let (_root, project, snapshot) = fixture();
    let mut remote = Fake {
        responses: VecDeque::from([
            response(200, serde_json::json!({"username":"alice"})),
            Err("response lost"),
        ]),
        calls: vec![],
    };
    assert!(deliver_with(&project, &snapshot, &mut remote).is_err());
    assert!(
        deliver_with(&project, &snapshot, &mut remote)
            .unwrap_err()
            .to_string()
            .contains("uncertain")
    );
    let db = crate::tasks::state::Store::open(project.root()).unwrap();
    let events = db.delivery_events(None).unwrap();
    assert_eq!(events[0].state, "started");
    assert_eq!(events[1].state, "unknown");
    assert_eq!(remote.calls.len(), 2);
}
#[test]
fn another_account_is_rejected_before_intent_or_post() {
    let (_root, project, snapshot) = fixture();
    let mut remote = Fake {
        responses: VecDeque::from([response(
            200,
            serde_json::json!({"username":"work-account"}),
        )]),
        calls: vec![],
    };
    assert!(deliver_with(&project, &snapshot, &mut remote).is_err());
    assert_eq!(remote.calls.len(), 1);
    assert!(
        crate::tasks::state::Store::open(project.root())
            .unwrap()
            .delivery_events(None)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn restart_after_committed_intent_cannot_resubmit_and_other_connection_sees_it() {
    let (_root, project, snapshot) = fixture();
    let id = snapshot.id().unwrap();
    let intent = Event {
        review: id,
        state: "started".into(),
        at_unix_seconds: 1,
        remote_id: None,
        url: None,
        linkedin: None,
    };
    let mut db = crate::tasks::state::Store::open(project.root()).unwrap();
    db.begin_delivery(&intent).unwrap();
    assert!(
        crate::tasks::state::Store::open(project.root())
            .unwrap()
            .begin_delivery(&intent)
            .is_err()
    );
    drop(db);
    let mut remote = Fake {
        responses: VecDeque::new(),
        calls: vec![],
    };
    assert!(deliver_with(&project, &snapshot, &mut remote).is_err());
    assert!(remote.calls.is_empty());
}
#[test]
fn definitive_rejection_can_be_retried_but_a_server_failure_cannot() {
    let (_root, project, snapshot) = fixture();
    let mut remote = Fake {
        responses: VecDeque::from([
            response(200, serde_json::json!({"username":"alice"})),
            response(422, Value::Null),
            response(200, serde_json::json!({"username":"alice"})),
            response(503, Value::Null),
        ]),
        calls: vec![],
    };
    assert!(deliver_with(&project, &snapshot, &mut remote).is_err());
    assert!(deliver_with(&project, &snapshot, &mut remote).is_err());
    assert!(deliver_with(&project, &snapshot, &mut remote).is_err());
    assert_eq!(remote.calls.len(), 4);
    let db = crate::tasks::state::Store::open(project.root()).unwrap();
    assert_eq!(
        db.delivery_events(None)
            .unwrap()
            .iter()
            .map(|e| e.state.as_str())
            .collect::<Vec<_>>(),
        vec!["started", "rejected", "started", "unknown"]
    );
}

fn uncertain(project: &Project, snapshot: &Snapshot) {
    let mut db = crate::tasks::state::Store::open(project.root()).unwrap();
    let mut intent = Event {
        review: snapshot.id().unwrap(),
        state: "started".into(),
        at_unix_seconds: 1,
        remote_id: None,
        url: None,
        linkedin: None,
    };
    db.begin_delivery(&intent).unwrap();
    intent.state = "unknown".into();
    db.finish_delivery(&intent).unwrap();
    intent.state = "rejected".into();
    assert!(
        db.finish_delivery(&intent).is_err(),
        "uncertainty must not be cleared by treating it as rejection"
    );
}
fn matching_draft() -> Value {
    serde_json::json!({"id":42,"url":"https://dev.to/alice/note","user":{"username":"alice"},"published_at":null,"title":"Note","body_markdown":"A worked example.","canonical_url":"https://example.test/notes/note.html","cover_image":null,"tag_list":[]})
}
#[test]
fn reconciliation_checks_remote_edition_and_survives_backup_restore() {
    let (_root, project, snapshot) = fixture();
    uncertain(&project, &snapshot);
    let mut remote = Fake {
        responses: VecDeque::from([
            response(200, serde_json::json!({"username":"alice"})),
            response(200, matching_draft()),
        ]),
        calls: vec![],
    };
    let event = reconcile_with(&project, &snapshot, 42, &mut remote).unwrap();
    assert_eq!(event.state, "reconciled");
    assert_eq!(
        remote.calls,
        vec!["GET /api/users/me", "GET /api/articles/42"]
    );
    assert_eq!(
        reconcile_with(&project, &snapshot, 42, &mut remote)
            .unwrap()
            .remote_id,
        Some(42)
    );
    crate::tasks::state::backup(&project, &project.root().join("backup.sqlite")).unwrap();
    let restored = tempfile::tempdir().unwrap();
    let restored_project = Project::new(restored.path().into(), vec![]);
    crate::tasks::state::restore(&restored_project, &project.root().join("backup.sqlite")).unwrap();
    let db = crate::tasks::state::Store::open(restored_project.root()).unwrap();
    assert_eq!(
        db.delivery_events(None).unwrap().last().unwrap().state,
        "reconciled"
    );
    assert_eq!(db.history().unwrap().len(), 1);
    assert_eq!(
        crate::tasks::state::status(&restored_project).unwrap()["counts"]["delivery_events"],
        3
    );
}
#[test]
fn reconciliation_refuses_changed_content_owner_publication_image_and_tags() {
    for (field, value) in [
        ("title", Value::String("Changed title".into())),
        ("body_markdown", Value::String("Unreviewed body".into())),
        ("canonical_url", Value::String("https://other.test/".into())),
        ("published_at", Value::String("2026-11-15".into())),
        (
            "cover_image",
            Value::String("https://other.test/image.png".into()),
        ),
        ("tag_list", serde_json::json!(["unreviewed"])),
        ("tag_list", Value::Null),
        ("user", serde_json::json!({"username":"other"})),
    ] {
        let (_root, project, snapshot) = fixture();
        uncertain(&project, &snapshot);
        let mut body = matching_draft();
        body[field] = value;
        let mut remote = Fake {
            responses: VecDeque::from([
                response(200, serde_json::json!({"username":"alice"})),
                response(200, body),
            ]),
            calls: vec![],
        };
        assert!(
            reconcile_with(&project, &snapshot, 42, &mut remote).is_err(),
            "{field}"
        );
        assert_eq!(
            crate::tasks::state::Store::open(project.root())
                .unwrap()
                .delivery_events(None)
                .unwrap()
                .last()
                .unwrap()
                .state,
            "unknown"
        );
    }
}
#[test]
fn full_article_approvals_use_a_separate_bounded_record_budget() {
    let (_root, project, mut snapshot) = fixture();
    let mut payload: Value = serde_json::from_str(&snapshot.text).unwrap();
    payload["article"]["body_markdown"] = Value::String("word ".repeat(50_000));
    snapshot.text = payload.to_string();
    snapshot.text_sha256 = release::digest(snapshot.text.as_bytes());
    snapshot.validate().unwrap();
    super::super::store::approve(
        &project,
        &Approval {
            review_id: snapshot.id().unwrap(),
            snapshot,
            approved_at_unix_seconds: 2,
        },
    )
    .unwrap();
    assert_eq!(
        crate::tasks::state::Store::open(project.root())
            .unwrap()
            .history()
            .unwrap()
            .len(),
        2
    );
    crate::tasks::state::status(&project).unwrap();
}

#[test]
fn schema_six_upgrade_preserves_approvals_and_rolls_back_a_failed_step() {
    let (_root, project, snapshot) = fixture();
    let connection = rusqlite::Connection::open(crate::tasks::state::path(project.root())).unwrap();
    connection
        .execute_batch("DROP TABLE delivery_events; PRAGMA user_version=6;")
        .unwrap();
    crate::tasks::state::backup(&project, &project.root().join("schema-six.sqlite")).unwrap();
    connection
        .execute_batch("CREATE TABLE delivery_events(conflict TEXT);")
        .unwrap();
    assert!(crate::tasks::state::migrate(&project).is_err());
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 6);
    connection
        .execute_batch("DROP TABLE delivery_events;")
        .unwrap();
    drop(connection);
    crate::tasks::state::migrate(&project).unwrap();
    let db = crate::tasks::state::Store::open(project.root()).unwrap();
    assert_eq!(
        db.history().unwrap()[0].approval.review_id,
        snapshot.id().unwrap()
    );
    assert!(db.delivery_events(None).unwrap().is_empty());
    crate::tasks::state::migrate(&project).unwrap();
    let restored = tempfile::tempdir().unwrap();
    let restored_project = Project::new(restored.path().into(), vec![]);
    crate::tasks::state::restore(&restored_project, &project.root().join("schema-six.sqlite"))
        .unwrap();
    crate::tasks::state::migrate(&restored_project).unwrap();
    assert_eq!(
        crate::tasks::state::Store::open(restored_project.root())
            .unwrap()
            .history()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn schema_seven_upgrade_keeps_existing_dev_approvals_and_backup_readable() {
    let (_root, project, snapshot) = fixture();
    let connection = rusqlite::Connection::open(crate::tasks::state::path(project.root())).unwrap();
    connection.pragma_update(None, "user_version", 7).unwrap();
    drop(connection);
    crate::tasks::state::backup(&project, &project.root().join("schema-seven.sqlite")).unwrap();
    assert!(crate::tasks::state::Store::open(project.root()).is_err());
    crate::tasks::state::migrate(&project).unwrap();
    let db = crate::tasks::state::Store::open(project.root()).unwrap();
    assert_eq!(
        db.history().unwrap()[0].approval.review_id,
        snapshot.id().unwrap()
    );
    assert!(db.delivery_events(None).unwrap().is_empty());
    assert_eq!(
        crate::tasks::state::status(&project).unwrap()["schema_version"],
        9
    );
}
