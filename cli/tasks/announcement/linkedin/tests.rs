use super::super::delivery::Event;
use super::{
    http::{Request, Response, Transport},
    *,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
};

struct Fake {
    responses: VecDeque<Result<Response, &'static str>>,
    calls: Vec<(String, String, Vec<u8>)>,
}
impl Transport for Fake {
    fn send(&mut self, request: Request<'_>) -> Result<Response, Error> {
        assert!(http::allowed_url(&request.url));
        if request.url.path().starts_with("/rest/") {
            assert_eq!(request.version, Some(API_VERSION));
        }
        self.calls.push((
            request.method.into(),
            request.url.to_string(),
            request.body.unwrap_or_default().to_vec(),
        ));
        self.responses
            .pop_front()
            .expect("unexpected network request")
            .map_err(anyhow::Error::msg)
    }
}
fn response(status: u16, body: Value) -> Result<Response, &'static str> {
    Ok(Response {
        status,
        body,
        headers: BTreeMap::new(),
    })
}
fn identity(member: &str) -> Result<Response, &'static str> {
    response(200, json!({"sub":member,"name":"Personal Member"}))
}
fn posted() -> Result<Response, &'static str> {
    Ok(Response {
        status: 201,
        body: Value::Null,
        headers: BTreeMap::from([("x-restli-id".into(), "urn:li:share:123456".into())]),
    })
}
fn fake(responses: Vec<Result<Response, &'static str>>) -> Fake {
    Fake {
        responses: responses.into(),
        calls: vec![],
    }
}
fn fixture(image: bool) -> (tempfile::TempDir, Project, Snapshot) {
    let root = tempfile::tempdir().unwrap();
    let project = Project::new(root.path().into(), vec![]);
    crate::tasks::state::migrate(&project).unwrap();
    let text = "I wrote about durable jobs.\n\nhttps://example.test/note.html\n".to_owned();
    let snapshot = Snapshot {
        schema_version: 2,
        channel: Channel::LinkedIn,
        account: "personal".into(),
        before_release: "a".repeat(64),
        after_release: "a".repeat(64),
        website_url: Url::parse("https://example.test/").unwrap(),
        website_destination: berlin_core::DeploymentTarget::GitHubPages {
            repository: "owner/site".into(),
        },
        fingerprint_version: 1,
        document: release::documents::DocumentRevision {
            id: berlin_document::ContentId("note".into()),
            title: "Note".into(),
            path: "note.html".into(),
            content_sha256: "b".repeat(64),
            metadata_sha256: "c".repeat(64),
        },
        kind: AnnouncementKind::New,
        article_url: Url::parse("https://example.test/note.html").unwrap(),
        text_sha256: release::digest(text.as_bytes()),
        text,
        image: image.then(|| packages::Image {
            url: Url::parse("https://example.test/cover.png").unwrap(),
            alt: "Controller and independent workers".into(),
            path: "cover.png".into(),
            sha256: release::digest(b"image bytes"),
        }),
        linkedin: Some(Target {
            member_urn: "urn:li:person:personal".into(),
            name: "Personal Member".into(),
            api_version: API_VERSION.into(),
            visibility: "PUBLIC".into(),
            organization: None,
        }),
        warnings: vec![],
    };
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
fn image_publication_uses_approved_bytes_alt_and_member_then_commits_receipt_with_report() {
    let (_root, project, snapshot) = fixture(true);
    let mut remote = fake(vec![
        identity("personal"),
        response(
            200,
            json!({"value":{"image":"urn:li:image:asset","uploadUrl":"https://www.linkedin.com/dms-uploads/asset?signature=temporary"}}),
        ),
        response(201, Value::Null),
        posted(),
    ]);
    let event = publish::deliver_with(
        &project,
        &snapshot,
        "token",
        Some(b"image bytes"),
        &mut remote,
    )
    .unwrap();
    assert_eq!(event.state, "submitted");
    assert_eq!(
        event.linkedin.as_ref().unwrap().post_urn.as_deref(),
        Some("urn:li:share:123456")
    );
    assert_eq!(remote.calls[2].2, b"image bytes");
    let payload: Value = serde_json::from_slice(&remote.calls[3].2).unwrap();
    assert_eq!(payload["author"], "urn:li:person:personal");
    assert_eq!(
        payload["content"]["media"]["altText"],
        "Controller and independent workers"
    );
    assert_eq!(payload["visibility"], "PUBLIC");
    assert_eq!(payload["lifecycleState"], "PUBLISHED");
    assert_eq!(
        publish::deliver_with(
            &project,
            &snapshot,
            "token",
            Some(b"image bytes"),
            &mut remote
        )
        .unwrap()
        .state,
        "submitted"
    );
    assert_eq!(remote.calls.len(), 4);
    let db = crate::tasks::state::Store::open(project.root()).unwrap();
    assert_eq!(
        db.delivery_events(None)
            .unwrap()
            .iter()
            .map(|e| e.state.as_str())
            .collect::<Vec<_>>(),
        vec!["started", "submitting", "submitted"]
    );
    let history = db.history().unwrap();
    assert_eq!(
        history[0].publication.as_ref().unwrap().evidence,
        Evidence::ApiAcknowledged
    );
    assert!(matches!(
        super::super::review_with_history(snapshot, &history)
            .unwrap()
            .state,
        ReviewState::SubmissionAccepted
    ));
    crate::tasks::state::backup(&project, &project.root().join("backup.sqlite")).unwrap();
    let restored = tempfile::tempdir().unwrap();
    let restored_project = Project::new(restored.path().into(), vec![]);
    crate::tasks::state::restore(&restored_project, &project.root().join("backup.sqlite")).unwrap();
    assert_eq!(
        crate::tasks::state::Store::open(restored.path())
            .unwrap()
            .history()
            .unwrap()[0]
            .publication
            .as_ref()
            .unwrap()
            .evidence,
        Evidence::ApiAcknowledged
    );
}
#[test]
fn wrong_member_and_changed_media_never_start_a_delivery() {
    let (_root, project, snapshot) = fixture(true);
    let mut remote = fake(vec![identity("work")]);
    assert!(
        publish::deliver_with(
            &project,
            &snapshot,
            "token",
            Some(b"image bytes"),
            &mut remote
        )
        .is_err()
    );
    assert_eq!(remote.calls.len(), 1);
    let mut remote = fake(vec![identity("personal")]);
    assert!(
        publish::deliver_with(&project, &snapshot, "token", Some(b"modified"), &mut remote)
            .is_err()
    );
    assert!(
        crate::tasks::state::Store::open(project.root())
            .unwrap()
            .delivery_events(None)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn lost_response_blocks_retry_and_user_reconciliation_is_not_api_verification() {
    let (_root, project, snapshot) = fixture(false);
    let mut remote = fake(vec![identity("personal"), Err("lost response")]);
    assert!(publish::deliver_with(&project, &snapshot, "token", None, &mut remote).is_err());
    assert!(publish::deliver_with(&project, &snapshot, "token", None, &mut remote).is_err());
    assert_eq!(remote.calls.len(), 2);
    let id = snapshot.id().unwrap();
    assert!(
        crate::tasks::state::Store::open(project.root())
            .unwrap()
            .history()
            .unwrap()[0]
            .publication
            .is_none()
    );
    let event = reconcile(&project, &id, "urn:li:share:123456", true, None).unwrap();
    assert_eq!(event.state, "reported");
    assert_eq!(
        crate::tasks::state::Store::open(project.root())
            .unwrap()
            .history()
            .unwrap()[0]
            .publication
            .as_ref()
            .unwrap()
            .evidence,
        Evidence::UserReported
    );
    assert_eq!(
        publish::deliver_with(&project, &snapshot, "token", None, &mut remote)
            .unwrap()
            .state,
        "reported"
    );
    crate::tasks::state::status(&project).unwrap();
}
#[test]
fn image_upload_is_restricted_to_linkedin_and_failure_never_submits_a_post() {
    for url in [
        "https://evil.test/upload",
        "https://www.linkedin.com.evil.test/dms-uploads/asset",
        "http://www.linkedin.com/dms-uploads/asset",
        "https://api.linkedin.com/v2/userinfo",
        "https://www.linkedin.com/oauth/v2/accessToken",
    ] {
        let (_root, project, snapshot) = fixture(true);
        let mut remote = fake(vec![
            identity("personal"),
            response(
                200,
                json!({"value":{"image":"urn:li:image:asset","uploadUrl":url}}),
            ),
        ]);
        assert!(
            publish::deliver_with(
                &project,
                &snapshot,
                "token",
                Some(b"image bytes"),
                &mut remote
            )
            .is_err()
        );
        assert_eq!(remote.calls.len(), 2);
        assert_eq!(
            crate::tasks::state::Store::open(project.root())
                .unwrap()
                .delivery_events(None)
                .unwrap()
                .last()
                .unwrap()
                .state,
            "rejected"
        );
    }
}
#[test]
fn client_rejection_can_retry_but_server_error_stays_uncertain() {
    let (_root, project, snapshot) = fixture(false);
    let mut remote = fake(vec![
        identity("personal"),
        response(422, Value::Null),
        identity("personal"),
        response(503, Value::Null),
    ]);
    assert!(publish::deliver_with(&project, &snapshot, "token", None, &mut remote).is_err());
    assert!(publish::deliver_with(&project, &snapshot, "token", None, &mut remote).is_err());
    assert!(publish::deliver_with(&project, &snapshot, "token", None, &mut remote).is_err());
    assert_eq!(remote.calls.len(), 4);
}
#[test]
fn readback_checks_exact_text_member_audience_state_and_uploaded_media() {
    let (_root, _project, snapshot) = fixture(true);
    let event = Event {
        review: snapshot.id().unwrap(),
        state: "unknown".into(),
        at_unix_seconds: 1,
        remote_id: None,
        url: None,
        linkedin: Some(Attempt {
            target: snapshot.linkedin.clone().unwrap(),
            image_urn: Some("urn:li:image:asset".into()),
            post_urn: None,
        }),
    };
    let mut body = publish::payload(&snapshot, Some("urn:li:image:asset")).unwrap();
    body["id"] = json!("urn:li:share:123456");
    let mut remote = fake(vec![identity("personal"), response(200, body.clone())]);
    publish::verify_remote(
        &snapshot,
        &event,
        "token",
        "urn:li:share:123456",
        &mut remote,
    )
    .unwrap();
    for (field, value) in [
        ("author", json!("urn:li:person:work")),
        ("commentary", json!("changed")),
        ("visibility", json!("CONNECTIONS")),
        ("lifecycleState", json!("PUBLISH_REQUESTED")),
        ("content", json!({"media":{"id":"urn:li:image:other"}})),
    ] {
        let mut changed = body.clone();
        changed[field] = value;
        let mut remote = fake(vec![identity("personal"), response(200, changed)]);
        assert!(
            publish::verify_remote(
                &snapshot,
                &event,
                "token",
                "urn:li:share:123456",
                &mut remote
            )
            .is_err()
        );
    }
    let mut remote = fake(vec![identity("personal"), response(403, Value::Null)]);
    assert!(
        publish::verify_remote(
            &snapshot,
            &event,
            "token",
            "urn:li:share:123456",
            &mut remote
        )
        .is_err()
    );
}
#[test]
fn target_is_part_of_approval_and_little_text_preserves_literal_characters() {
    let (_root, _project, mut snapshot) = fixture(false);
    let id = snapshot.id().unwrap();
    snapshot.linkedin.as_mut().unwrap().member_urn = "urn:li:person:work".into();
    assert_ne!(snapshot.id().unwrap(), id);
    assert_eq!(
        publish::little("a_(b) #tag @person \\ |"),
        "a\\_\\(b\\) \\#tag \\@person \\\\ \\|"
    );
}

#[test]
fn duplicate_remote_post_id_cannot_commit_conflicting_publication_evidence() {
    let (_root, project, mut snapshot) = fixture(false);
    let mut remote = fake(vec![identity("personal"), posted()]);
    publish::deliver_with(&project, &snapshot, "token", None, &mut remote).unwrap();
    snapshot.document.content_sha256 = "f".repeat(64);
    super::super::store::approve(
        &project,
        &Approval {
            review_id: snapshot.id().unwrap(),
            snapshot: snapshot.clone(),
            approved_at_unix_seconds: 2,
        },
    )
    .unwrap();
    let mut remote = fake(vec![identity("personal"), posted()]);
    assert!(publish::deliver_with(&project, &snapshot, "token", None, &mut remote).is_err());
    let db = crate::tasks::state::Store::open(project.root()).unwrap();
    assert_eq!(
        db.history()
            .unwrap()
            .iter()
            .filter(|h| h.publication.is_some())
            .count(),
        1
    );
    assert_eq!(
        db.delivery_events(Some(&snapshot.id().unwrap()))
            .unwrap()
            .last()
            .unwrap()
            .state,
        "submitting"
    );
    crate::tasks::state::status(&project).unwrap();
}
#[test]
fn oauth_start_and_callback_page_are_local_and_never_overwrite_assets() {
    let root = tempfile::tempdir().unwrap();
    let project = Project::new(root.path().into(), vec![]);
    let redirect = Url::parse("https://example.test/static/auth/linkedin.html").unwrap();
    let result = connection::auth_start(
        &project,
        "personal",
        "client123",
        &redirect,
        API_VERSION,
        None,
    )
    .unwrap();
    let url = Url::parse(result["authorization_url"].as_str().unwrap()).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
    assert_eq!(query["scope"], "openid profile w_member_social");
    assert_eq!(query["state"].len(), 64);
    let file = Path::new("static/auth/linkedin.html");
    connection::callback_page(&project, &redirect, file).unwrap();
    let html = std::fs::read_to_string(root.path().join(file)).unwrap();
    assert!(html.contains("default-src 'none'"));
    assert!(html.contains("history.replaceState"));
    assert!(html.contains("no-referrer"));
    assert!(!html.contains("@@"));
    assert!(connection::callback_page(&project, &redirect, file).is_err());
    assert!(connection::callback_page(&project, &redirect, Path::new("../escape.html")).is_err());
}

fn company_fixture(image: bool) -> (tempfile::TempDir, Project, Snapshot) {
    let (root, project, mut snapshot) = fixture(image);
    snapshot.account = "company".into();
    snapshot.linkedin.as_mut().unwrap().organization = Some(Organization {
        urn: "urn:li:organization:12345".into(),
        name: "Example Company".into(),
    });
    super::super::store::approve(
        &project,
        &Approval {
            review_id: snapshot.id().unwrap(),
            snapshot: snapshot.clone(),
            approved_at_unix_seconds: 2,
        },
    )
    .unwrap();
    (root, project, snapshot)
}
fn authorized(action: &str) -> Value {
    json!({
        "impersonator":"urn:li:person:personal",
        "organization":"urn:li:organization:12345",
        "action":{"OrganizationContentAuthorizationActionType":{"actionType":action}},
        "status":{"com.linkedin.organization.Approved":{}}
    })
}
#[test]
fn company_delivery_owns_media_and_authors_post_as_the_approved_page() {
    let (_root, project, snapshot) = company_fixture(true);
    let mut remote = fake(vec![
        identity("personal"),
        response(200, authorized("ORGANIC_SHARE_CREATE")),
        response(
            200,
            json!({"value":{"image":"urn:li:image:asset","uploadUrl":"https://www.linkedin.com/dms-uploads/asset"}}),
        ),
        response(201, Value::Null),
        posted(),
    ]);
    let event = publish::deliver_with(
        &project,
        &snapshot,
        "token",
        Some(b"image bytes"),
        &mut remote,
    )
    .unwrap();
    assert_eq!(event.state, "submitted");
    assert!(
        remote.calls[1]
            .1
            .contains("impersonator:urn%3Ali%3Aperson%3Apersonal")
    );
    assert!(
        remote.calls[1]
            .1
            .contains("organization:urn%3Ali%3Aorganization%3A12345")
    );
    assert!(remote.calls[1].1.contains("ORGANIC_SHARE_CREATE"));
    let upload: Value = serde_json::from_slice(&remote.calls[2].2).unwrap();
    let post: Value = serde_json::from_slice(&remote.calls[4].2).unwrap();
    assert_eq!(
        upload["initializeUploadRequest"]["owner"],
        "urn:li:organization:12345"
    );
    assert_eq!(post["author"], "urn:li:organization:12345");
    assert_eq!(post["visibility"], "PUBLIC");
    assert_eq!(
        post["content"]["media"]["altText"],
        snapshot.image.as_ref().unwrap().alt
    );
    assert_eq!(
        event.linkedin.as_ref().unwrap().target,
        *snapshot.linkedin.as_ref().unwrap()
    );
    // A settled delivery does not check permissions or submit a second post.
    publish::deliver_with(
        &project,
        &snapshot,
        "token",
        Some(b"image bytes"),
        &mut remote,
    )
    .unwrap();
    assert_eq!(remote.calls.len(), 5);
    crate::tasks::state::status(&project).unwrap();
}
#[test]
fn denied_missing_or_mismatched_page_authorization_never_starts_delivery() {
    let (_root, project, snapshot) = company_fixture(false);
    let approved = authorized("ORGANIC_SHARE_CREATE");
    let mut invalid = vec![Value::Null];
    for (field, value) in [
        ("organization", json!("urn:li:organization:999")),
        ("impersonator", json!("urn:li:person:another")),
        (
            "action",
            json!({"OrganizationContentAuthorizationActionType":{"actionType":"DARK_SHARE_CREATE"}}),
        ),
        (
            "status",
            json!({"com.linkedin.organization.Denied":{"reasons":["MEMBER_HAS_INSUFFICIENT_PERMISSIONS_IN_ACCESS_CONTROL"]}}),
        ),
        ("status", json!({"com.linkedin.organization.Approved":null})),
        (
            "status",
            json!({"com.linkedin.organization.Approved":{},"com.linkedin.organization.Denied":{}}),
        ),
    ] {
        let mut body = approved.clone();
        body[field] = value;
        invalid.push(body);
    }
    for body in invalid {
        let mut remote = fake(vec![identity("personal"), response(200, body)]);
        assert!(publish::deliver_with(&project, &snapshot, "token", None, &mut remote).is_err());
        assert_eq!(remote.calls.len(), 2);
    }
    let mut remote = fake(vec![identity("personal"), response(403, Value::Null)]);
    assert!(publish::deliver_with(&project, &snapshot, "token", None, &mut remote).is_err());
    let mut remote = fake(vec![identity("work")]);
    assert!(publish::deliver_with(&project, &snapshot, "token", None, &mut remote).is_err());
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
fn company_connection_uses_remote_page_identity_after_permission_check() {
    let (_root, _project, snapshot) = fixture(false);
    let member = snapshot.linkedin.unwrap();
    let mut remote = fake(vec![
        response(200, authorized("ORGANIC_SHARE_CREATE")),
        response(200, json!({"id":12345,"localizedName":"Example Company"})),
    ]);
    let target = organization::bind(
        &mut remote,
        "token",
        member.clone(),
        Some("urn:li:organization:12345"),
    )
    .unwrap();
    assert_eq!(target.author_urn(), "urn:li:organization:12345");
    assert_eq!(target.organization.unwrap().name, "Example Company");
    assert_eq!(remote.calls[0].0, "GET");
    assert!(
        remote.calls[1]
            .1
            .contains("/rest/organizations/12345?projection=")
    );
    for id in [12346, 0] {
        let mut remote = fake(vec![
            response(200, authorized("ORGANIC_SHARE_CREATE")),
            response(200, json!({"id":id,"localizedName":"Other Company"})),
        ]);
        assert!(
            organization::bind(
                &mut remote,
                "token",
                member.clone(),
                Some("urn:li:organization:12345")
            )
            .is_err()
        );
    }
}
#[test]
fn company_readback_requires_current_view_permission_and_exact_page_author() {
    let (_root, _project, snapshot) = company_fixture(false);
    let event = Event {
        review: snapshot.id().unwrap(),
        state: "unknown".into(),
        at_unix_seconds: 1,
        remote_id: None,
        url: None,
        linkedin: Some(Attempt {
            target: snapshot.linkedin.clone().unwrap(),
            image_urn: None,
            post_urn: None,
        }),
    };
    let mut body = publish::payload(&snapshot, None).unwrap();
    body["id"] = json!("urn:li:share:123456");
    let mut remote = fake(vec![
        identity("personal"),
        response(200, authorized("ORGANIC_SHARE_VIEW_AS_AUTHOR")),
        response(200, body.clone()),
    ]);
    publish::verify_remote(
        &snapshot,
        &event,
        "token",
        "urn:li:share:123456",
        &mut remote,
    )
    .unwrap();
    assert!(remote.calls[1].1.contains("ORGANIC_SHARE_VIEW_AS_AUTHOR"));
    body["author"] = json!("urn:li:person:personal");
    let mut remote = fake(vec![
        identity("personal"),
        response(200, authorized("ORGANIC_SHARE_VIEW_AS_AUTHOR")),
        response(200, body),
    ]);
    assert!(
        publish::verify_remote(
            &snapshot,
            &event,
            "token",
            "urn:li:share:123456",
            &mut remote
        )
        .is_err()
    );
    // Write permission alone does not claim permission to read a Page's post.
    let mut remote = fake(vec![
        identity("personal"),
        response(200, authorized("ORGANIC_SHARE_CREATE")),
    ]);
    assert!(
        publish::verify_remote(
            &snapshot,
            &event,
            "token",
            "urn:li:share:123456",
            &mut remote
        )
        .is_err()
    );
    assert_eq!(remote.calls.len(), 2);
}
#[test]
fn organization_selection_changes_approval_and_old_member_targets_keep_their_encoding() {
    let (_root, _project, mut snapshot) = fixture(false);
    let personal_id = snapshot.id().unwrap();
    let target = snapshot.linkedin.as_ref().unwrap();
    let serialized = serde_json::to_value(target).unwrap();
    assert!(serialized.get("organization").is_none());
    assert_eq!(
        serde_json::from_value::<Target>(serialized).unwrap(),
        *target
    );
    snapshot.linkedin.as_mut().unwrap().organization = Some(Organization {
        urn: "urn:li:organization:12345".into(),
        name: "Company".into(),
    });
    let company_id = snapshot.id().unwrap();
    assert_ne!(company_id, personal_id);
    snapshot
        .linkedin
        .as_mut()
        .unwrap()
        .organization
        .as_mut()
        .unwrap()
        .urn = "urn:li:organization:67890".into();
    assert_ne!(snapshot.id().unwrap(), company_id);
    snapshot.linkedin.as_mut().unwrap().organization = None;
    assert_eq!(snapshot.id().unwrap(), personal_id);
}
#[test]
fn company_oauth_scopes_are_explicit_and_invalid_page_ids_are_rejected_locally() {
    let root = tempfile::tempdir().unwrap();
    let project = Project::new(root.path().into(), vec![]);
    let redirect = Url::parse("https://example.test/callback").unwrap();
    for invalid in [
        "12345",
        "urn:li:person:12345",
        "urn:li:organization:0",
        "urn:li:organization:01",
        "urn:li:organization:123/other",
        "urn:li:organization:abc",
    ] {
        assert!(
            connection::auth_start(
                &project,
                "company",
                "client",
                &redirect,
                API_VERSION,
                Some(invalid)
            )
            .is_err()
        );
    }
    assert!(!root.path().join(".berlin").exists());
    let result = connection::auth_start(
        &project,
        "company",
        "client",
        &redirect,
        API_VERSION,
        Some("urn:li:organization:12345"),
    )
    .unwrap();
    let url = Url::parse(result["authorization_url"].as_str().unwrap()).unwrap();
    let fields: BTreeMap<_, _> = url.query_pairs().collect();
    assert_eq!(
        fields["scope"],
        "openid profile w_organization_social r_organization_social rw_organization_admin"
    );
    let pending: Value = serde_json::from_str(
        &std::fs::read_to_string(
            root.path()
                .join(".berlin/connections/linkedin/company/pending.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(pending["organization"], "urn:li:organization:12345");
}
