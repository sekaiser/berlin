use super::*;

fn package() -> (tempfile::TempDir, String) {
    let root = fixture();
    fs::write(root.path().join("static/cover.png"), b"public teaser bytes").unwrap();
    note(
        root.path(),
        "note",
        "Note",
        "preview:\n  source: /static/cover.png\n  alt: A public diagram\n  width: 100\n  height: 100\n",
        "## Example\n\nA **useful** explanation.\n\n```go\nfmt.Println(1)\n```\n",
    );
    note(
        root.path(),
        "private",
        "Never distribute",
        "draft: true\n",
        "SECRET_DRAFT_BODY",
    );
    let id = release(root.path());
    success(run(
        root.path(),
        &[
            "announcement",
            "prepare",
            &id,
            "--document",
            "note",
            "--output",
            "review",
        ],
    ));
    (root, id)
}
fn review(root: &Path, channel: &str, extra: &[&str]) -> Output {
    let mut args = vec![
        "announcement",
        "review",
        "review",
        "--document",
        "note",
        "--account",
        "alice",
        "--channel",
        channel,
        "--json",
    ];
    args.extend(extra);
    run(root, &args)
}
#[test]
fn exports_sealed_public_content_and_media_without_authoring_inputs_or_network() {
    let (root, id) = package();
    let sealed = root.path().join(".berlin/releases").join(&id);
    let capture = fs::read_to_string(sealed.join("articles.json")).unwrap();
    assert!(!capture.contains("SECRET_DRAFT_BODY"));
    assert!(!capture.contains("file:///"));
    assert!(!capture.contains("provenance"));
    assert!(!sealed.join("site/articles.json").exists());
    let markdown = fs::read_to_string(root.path().join("review/article.md")).unwrap();
    assert!(markdown.contains("**useful**"));
    assert!(markdown.contains("```go"));
    assert_eq!(
        fs::read(root.path().join("review/teaser.png")).unwrap(),
        b"public teaser bytes"
    );
    let payload: Value =
        serde_json::from_slice(&fs::read(root.path().join("review/devto.json")).unwrap()).unwrap();
    assert_eq!(payload["article"]["published"], false);
    assert_eq!(
        payload["article"]["canonical_url"],
        "https://example.com/notes/note.html"
    );
    fs::remove_dir_all(root.path().join("content")).unwrap();
    fs::remove_file(root.path().join("berlin.pipeline.rhai")).unwrap();
    for channel in ["linkedin", "x", "devto", "substack"] {
        let r: Value = serde_json::from_str(&success(review(root.path(), channel, &[]))).unwrap();
        assert_eq!(r["snapshot"]["account"], "alice");
        assert_eq!(r["snapshot"]["image"]["alt"], "A public diagram");
    }
    assert!(
        !run(
            root.path(),
            &[
                "announcement",
                "prepare",
                &id,
                "--document",
                "private",
                "--output",
                "bad"
            ]
        )
        .status
        .success()
    );
}

fn substack_preview(root: &Path) -> Output {
    run(
        root,
        &[
            "announcement",
            "preview",
            "review",
            "--document",
            "note",
            "--account",
            "alice",
        ],
    )
}
fn edit_substack(root: &Path, edit: impl FnOnce(&mut Value)) {
    let file = root.join("review/substack.json");
    let mut draft: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    edit(&mut draft);
    fs::write(file, serde_json::to_vec_pretty(&draft).unwrap()).unwrap();
}
#[test]
fn substack_preview_approval_and_manual_publication_survive_backup_without_authoring_inputs() {
    let (root, _) = package();
    success(run(root.path(), &["state", "migrate"]));
    fs::remove_dir_all(root.path().join("content")).unwrap();
    fs::remove_file(root.path().join("berlin.pipeline.rhai")).unwrap();
    let draft: Value =
        serde_json::from_slice(&fs::read(root.path().join("review/substack.json")).unwrap())
            .unwrap();
    assert_eq!(draft["send_email"], false);
    assert_eq!(draft["audience"], "everyone");
    assert_eq!(draft["publication_url"], Value::Null);
    let unbound: Value =
        serde_json::from_str(&success(review(root.path(), "substack", &[]))).unwrap();
    assert_eq!(unbound["approval_allowed"], false);
    assert!(
        !review(
            root.path(),
            "substack",
            &["--approve", unbound["review_id"].as_str().unwrap()]
        )
        .status
        .success()
    );
    edit_substack(root.path(), |draft| {
        draft["publication_url"] = serde_json::json!("https://writer.substack.com")
    });
    let reviewed: Value =
        serde_json::from_str(&success(review(root.path(), "substack", &[]))).unwrap();
    let id = reviewed["review_id"].as_str().unwrap();
    // An edition cannot be approved before its exact formatted preview exists.
    assert!(
        !review(root.path(), "substack", &["--approve", id])
            .status
            .success()
    );
    let preview: Value = serde_json::from_str(&success(substack_preview(root.path()))).unwrap();
    assert_eq!(preview["review_id"], id);
    let html = fs::read_to_string(root.path().join("review/substack.html")).unwrap();
    for expected in [
        id,
        "<strong>useful</strong>",
        "fmt.Println(1)",
        "Send email:",
        "no (web only)",
        "Cover alt text:",
        "Originally published at",
    ] {
        assert!(html.contains(expected), "missing {expected}");
    }
    assert!(!html.contains("SECRET_DRAFT_BODY"));
    // Independently check the final, fully expanded preview's CSP hashes.
    use base64::Engine;
    use sha2::Digest;
    for tag in ["style", "script"] {
        let contents = html
            .split_once(&format!("<{tag}>"))
            .unwrap()
            .1
            .split_once(&format!("</{tag}>"))
            .unwrap()
            .0;
        let digest = base64::engine::general_purpose::STANDARD
            .encode(sha2::Sha256::digest(contents.as_bytes()));
        let csp = html
            .lines()
            .find(|line| line.contains("http-equiv=\"Content-Security-Policy\""))
            .unwrap();
        assert!(html_escape::decode_html_entities(csp).contains(&format!("'sha256-{digest}'")));
    }
    success(review(root.path(), "substack", &["--approve", id]));
    let report = |url: &str| {
        run(
            root.path(),
            &[
                "announcement",
                "record",
                "review",
                "--document",
                "note",
                "--account",
                "alice",
                "--channel",
                "substack",
                "--url",
                url,
                "--confirm",
                id,
            ],
        )
    };
    assert!(!report("https://other.substack.com/p/note").status.success());
    assert!(!report("https://writer.substack.com/about").status.success());
    let recorded: Value = serde_json::from_str(&success(report(
        "https://writer.substack.com/p/note?utm_source=test#comments",
    )))
    .unwrap();
    assert_eq!(recorded["post_url"], "https://writer.substack.com/p/note");
    assert_eq!(recorded["evidence"], "user_reported");
    success(report("https://writer.substack.com/p/note"));
    let status: Value =
        serde_json::from_str(&success(review(root.path(), "substack", &[]))).unwrap();
    assert_eq!(status["state"], "reported_published");
    success(run(root.path(), &["state", "backup", "substack.sqlite"]));
    let restored = tempfile::tempdir().unwrap();
    let backup = root.path().join("substack.sqlite");
    success(run(
        restored.path(),
        &["state", "restore", backup.to_str().unwrap()],
    ));
    let history: Value = serde_json::from_str(&success(run(
        restored.path(),
        &["announcement", "status", "--json"],
    )))
    .unwrap();
    assert_eq!(history["history"].as_array().unwrap().len(), 1);
    assert_eq!(history["history"][0]["approval"]["review_id"], id);
    assert_eq!(
        history["history"][0]["publication"]["evidence"],
        "user_reported"
    );
    // No API draft, delivery attempt or email is implied by a manual report.
    let events: Value = serde_json::from_str(&success(run(
        restored.path(),
        &["announcement", "deliveries"],
    )))
    .unwrap();
    assert!(events.as_array().unwrap().is_empty());
}
#[test]
fn substack_edits_invalidate_preview_and_bind_delivery_intentions_and_rendered_html() {
    let (root, _) = package();
    edit_substack(root.path(), |draft| {
        draft["publication_url"] = serde_json::json!("https://journal.example.test/")
    });
    success(substack_preview(root.path()));
    let original: Value =
        serde_json::from_str(&success(review(root.path(), "substack", &[]))).unwrap();
    let original_id = original["review_id"].as_str().unwrap();
    let file = root.path().join("review/substack.json");
    let bytes = fs::read(&file).unwrap();
    for (field, value) in [
        ("send_email", serde_json::json!(true)),
        ("audience", serde_json::json!("paid")),
        (
            "publication_url",
            serde_json::json!("https://other.example.test/"),
        ),
        ("subtitle", serde_json::json!("A revised introduction")),
        ("body_markdown", serde_json::json!("A revised edition.")),
    ] {
        fs::write(&file, &bytes).unwrap();
        edit_substack(root.path(), |draft| draft[field] = value);
        let changed: Value =
            serde_json::from_str(&success(review(root.path(), "substack", &[]))).unwrap();
        let id = changed["review_id"].as_str().unwrap();
        assert_ne!(id, original_id, "{field} did not affect approval");
        assert!(
            !review(root.path(), "substack", &["--approve", id])
                .status
                .success()
        );
        success(substack_preview(root.path()));
    }
    // A hand-edited generated HTML file must never masquerade as the reviewed edition.
    let current: Value =
        serde_json::from_str(&success(review(root.path(), "substack", &[]))).unwrap();
    fs::write(root.path().join("review/substack.html"), "unreviewed HTML").unwrap();
    assert!(
        !review(
            root.path(),
            "substack",
            &["--approve", current["review_id"].as_str().unwrap()]
        )
        .status
        .success()
    );
    assert!(!substack_preview(root.path()).status.success());
    let output = run(
        root.path(),
        &[
            "announcement",
            "deliver",
            "review",
            "--document",
            "note",
            "--account",
            "alice",
            "--channel",
            "substack",
            "--confirm",
            current["review_id"].as_str().unwrap(),
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Substack delivery is manual"));
}
#[test]
fn schema_eight_migration_preserves_existing_manual_approval_bytes_and_publication() {
    let (root, _) = package();
    success(run(root.path(), &["state", "migrate"]));
    let original: Value = serde_json::from_str(&success(review(root.path(), "x", &[]))).unwrap();
    let id = original["review_id"].as_str().unwrap();
    success(review(root.path(), "x", &["--approve", id]));
    success(run(
        root.path(),
        &[
            "announcement",
            "record",
            "review",
            "--document",
            "note",
            "--account",
            "alice",
            "--channel",
            "x",
            "--url",
            "https://x.com/alice/status/123",
            "--confirm",
            id,
        ],
    ));
    let connection = rusqlite::Connection::open(root.path().join(".berlin/state.sqlite")).unwrap();
    let before: String = connection
        .query_row("SELECT payload FROM approvals WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .unwrap();
    connection.pragma_update(None, "user_version", 8).unwrap();
    drop(connection);
    success(run(
        root.path(),
        &["state", "backup", "before-substack.sqlite"],
    ));
    success(run(root.path(), &["state", "migrate"]));
    let connection = rusqlite::Connection::open(root.path().join(".berlin/state.sqlite")).unwrap();
    let after: String = connection
        .query_row("SELECT payload FROM approvals WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(before, after);
    let status: Value = serde_json::from_str(&success(review(root.path(), "x", &[]))).unwrap();
    assert_eq!(status["review_id"], id);
    assert_eq!(status["state"], "reported_published");
    let state: Value =
        serde_json::from_str(&success(run(root.path(), &["state", "status"]))).unwrap();
    assert_eq!(state["schema_version"], 9);
}
#[test]
fn approvals_bind_channel_text_and_sealed_image_and_refuse_frontmatter_publication() {
    let (root, _) = package();
    success(run(root.path(), &["state", "migrate"]));
    let linkedin: Value =
        serde_json::from_str(&success(review(root.path(), "linkedin", &[]))).unwrap();
    let id = linkedin["review_id"].as_str().unwrap();
    success(review(root.path(), "linkedin", &["--approve", id]));
    let x: Value = serde_json::from_str(&success(review(root.path(), "x", &[]))).unwrap();
    assert_ne!(x["review_id"], id);
    assert!(
        !review(root.path(), "x", &["--approve", id])
            .status
            .success()
    );
    fs::write(root.path().join("review/x.txt"), "x".repeat(281)).unwrap();
    let x: Value = serde_json::from_str(&success(review(root.path(), "x", &[]))).unwrap();
    assert_eq!(x["approval_allowed"], false);
    let p = root.path().join("review/devto.json");
    let original = fs::read_to_string(&p).unwrap();
    let mut payload: Value = serde_json::from_str(&original).unwrap();
    payload["article"]["published"] = Value::Bool(true);
    fs::write(&p, serde_json::to_vec(&payload).unwrap()).unwrap();
    assert!(!review(root.path(), "devto", &[]).status.success());
    payload["article"]["published"] = Value::Bool(false);
    payload["article"]["body_markdown"] = Value::String("---\npublished: true\n---\nBody".into());
    fs::write(&p, serde_json::to_vec(&payload).unwrap()).unwrap();
    assert!(!review(root.path(), "devto", &[]).status.success());
    fs::write(&p, original).unwrap();
    fs::write(root.path().join("review/teaser.png"), b"changed image").unwrap();
    assert!(!review(root.path(), "linkedin", &[]).status.success());
}
#[test]
fn changed_public_capture_and_existing_export_are_never_silently_reused() {
    let (root, id) = package();
    assert!(
        !run(
            root.path(),
            &[
                "announcement",
                "prepare",
                &id,
                "--document",
                "note",
                "--output",
                "review"
            ]
        )
        .status
        .success()
    );
    let p = root
        .path()
        .join(".berlin/releases")
        .join(&id)
        .join("articles.json");
    fs::write(p, b"{}").unwrap();
    assert!(!review(root.path(), "linkedin", &[]).status.success());
}
#[test]
fn x_manual_reports_and_dev_approvals_survive_sqlite_backup() {
    let (root, _) = package();
    success(run(root.path(), &["state", "migrate"]));
    for channel in ["x", "devto"] {
        let preview: Value =
            serde_json::from_str(&success(review(root.path(), channel, &[]))).unwrap();
        let id = preview["review_id"].as_str().unwrap();
        success(review(root.path(), channel, &["--approve", id]));
        if channel == "x" {
            success(run(
                root.path(),
                &[
                    "announcement",
                    "record",
                    "review",
                    "--document",
                    "note",
                    "--account",
                    "alice",
                    "--channel",
                    "x",
                    "--url",
                    "https://x.com/alice/status/123",
                    "--confirm",
                    id,
                ],
            ));
        }
    }
    success(run(root.path(), &["state", "backup", "backup.sqlite"]));
    success(run(root.path(), &["state", "status"]));
    let status: Value = serde_json::from_str(&success(run(
        root.path(),
        &["announcement", "status", "--json"],
    )))
    .unwrap();
    assert_eq!(status["history"].as_array().unwrap().len(), 2);
    let r = run(
        root.path(),
        &[
            "announcement",
            "deliver",
            "review",
            "--document",
            "note",
            "--account",
            "alice",
            "--channel",
            "devto",
            "--confirm",
            &"0".repeat(64),
        ],
    );
    assert!(!r.status.success());
    assert!(!root.path().join("no-external-tools").exists());
}
