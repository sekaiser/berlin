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
    for channel in ["linkedin", "x", "devto"] {
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
