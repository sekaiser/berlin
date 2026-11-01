//! Manual publication is local evidence, never a remote publishing command.
use super::{announcements::snapshot, fixture, note, release, run, success};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Output,
};

const POST: &str = "https://www.linkedin.com/posts/test-post";

fn export(root: &Path, before: &str, after: &str, directory: &str) {
    success(run(
        root,
        &["release-drafts", before, after, "--output", directory],
    ));
}

fn prepare() -> tempfile::TempDir {
    let root = fixture();
    let before = release(root.path());
    note(root.path(), "note", "Note", "", "Revised article.");
    let after = release(root.path());
    export(root.path(), &before, &after, "review");
    root
}

fn command(root: &Path, action: &str, extra: &[&str]) -> Output {
    let mut args = vec![
        "announcement",
        action,
        "review",
        "--document",
        "note",
        "--account",
        "personal",
    ];
    args.extend(extra);
    run(root, &args)
}

fn preview(root: &Path) -> Value {
    serde_json::from_str(&success(command(root, "review", &["--json"]))).unwrap()
}

fn approve(root: &Path) -> String {
    let id = preview(root)["review_id"].as_str().unwrap().to_owned();
    let result: Value = serde_json::from_str(&success(command(
        root,
        "review",
        &["--approve", &id, "--json"],
    )))
    .unwrap();
    assert_eq!(result["state"], "approved");
    id
}

fn record(root: &Path, id: &str, url: &str) -> Output {
    command(root, "record", &["--url", url, "--confirm", id])
}

fn status(root: &Path) -> Value {
    serde_json::from_str(&success(run(root, &["announcement", "status", "--json"]))).unwrap()
}

fn text_file(root: &Path) -> PathBuf {
    root.join("review/note.txt")
}

fn fails(output: Output, message: &str) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(message),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_loop_preserves_exact_text_and_marks_delivery_as_user_reported_only() {
    let root = prepare();
    let text = "I added a worked example.\r\n\nhttps://example.com/notes/note.html\n";
    fs::write(text_file(root.path()), text).unwrap();
    // Do not run the build lock's transaction recovery or infer live delivery.
    fs::write(
        root.path().join(".berlin-transaction.json"),
        "invalid build journal",
    )
    .unwrap();
    fs::remove_file(root.path().join("berlin.pipeline.rhai")).unwrap();
    fs::remove_dir_all(root.path().join("content")).unwrap();
    let original = snapshot(root.path());
    let review = preview(root.path());
    assert_eq!(review["state"], "review_required");
    assert_eq!(review["snapshot"]["text"], text);
    assert_eq!(
        review["snapshot"]["text_sha256"],
        super::digest(text.as_bytes())
    );
    assert_eq!(preview(root.path()), review);
    assert!(
        status(root.path())["history"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(snapshot(root.path()), original);

    let id = approve(root.path());
    let approved = snapshot(root.path());
    success(command(root.path(), "review", &["--approve", &id]));
    assert_eq!(snapshot(root.path()), approved);
    let reported: Value = serde_json::from_str(&success(record(
        root.path(),
        &id,
        "https://linkedin.com/posts/test-post/?tracking=123#share",
    )))
    .unwrap();
    assert_eq!(reported["post_url"], POST);
    assert_eq!(reported["evidence"], "user_reported");
    let published = snapshot(root.path());
    let repeat: Value = serde_json::from_str(&success(record(root.path(), &id, POST))).unwrap();
    assert_eq!(repeat, reported);
    assert_eq!(snapshot(root.path()), published);
    assert_eq!(preview(root.path())["state"], "reported_published");
    let history = status(root.path());
    assert_eq!(history["remote_verified"], false);
    assert_eq!(history["website_live_checked"], false);
    assert_eq!(history["history"][0]["approval"]["snapshot"]["text"], text);
    assert_eq!(history["history"][0]["publication"], reported);
    assert_eq!(snapshot(root.path()), published);
    assert!(!root.path().join(".berlin/publications").exists());
    assert!(!root.path().join("_site").exists());

    // Durable history remains readable without build/review material.
    fs::remove_dir_all(root.path().join("review")).unwrap();
    fs::remove_dir_all(root.path().join(".berlin/releases")).unwrap();
    assert_eq!(status(root.path()), history);
}

#[test]
fn changed_text_or_account_needs_a_new_review_and_cannot_reuse_approval() {
    let root = prepare();
    let original = snapshot(root.path());
    let id = preview(root.path())["review_id"]
        .as_str()
        .unwrap()
        .to_owned();
    fails(record(root.path(), &id, POST), "not been approved");
    fails(
        command(root.path(), "review", &["--approve", "wrong"]),
        "must match the current review ID",
    );
    assert_eq!(snapshot(root.path()), original);
    approve(root.path());
    fs::write(text_file(root.path()), "Edited after approval.\n").unwrap();
    let changed = snapshot(root.path());
    let next = preview(root.path());
    assert_ne!(next["review_id"], id);
    assert_eq!(next["state"], "review_required");
    fails(
        record(root.path(), &id, POST),
        "does not match the current text",
    );
    assert_eq!(snapshot(root.path()), changed);
    let new_id = approve(root.path());
    fails(
        run(
            root.path(),
            &[
                "announcement",
                "record",
                "review",
                "--document",
                "note",
                "--account",
                "company",
                "--url",
                POST,
                "--confirm",
                &new_id,
            ],
        ),
        "does not match the current text",
    );
    success(record(root.path(), &new_id, POST));
    let status: Value =
        serde_json::from_str(&success(command(root.path(), "status", &["--json"]))).unwrap();
    assert_eq!(status["reviews"][0]["state"], "reported_published");
    assert_eq!(status["history"].as_array().unwrap().len(), 2);
}

#[test]
fn prevents_duplicate_revision_reports_and_overwriting_known_post_urls() {
    let root = prepare();
    let first = approve(root.path());
    success(record(root.path(), &first, POST));
    let saved = snapshot(root.path());
    fails(
        record(root.path(), &first, "https://www.linkedin.com/posts/other"),
        "will not be overwritten",
    );
    assert_eq!(snapshot(root.path()), saved);
    fs::write(
        text_file(root.path()),
        "Another wording for the same article revision.\n",
    )
    .unwrap();
    let second = approve(root.path());
    assert_eq!(preview(root.path())["same_revision_reports"][0], first);
    fails(
        record(
            root.path(),
            &second,
            "https://www.linkedin.com/posts/duplicate",
        ),
        "already has a reported announcement",
    );

    let before = release(root.path());
    note(root.path(), "note", "Note", "", "A different revision.");
    let after = release(root.path());
    export(root.path(), &before, &after, "next");
    fs::rename(root.path().join("review"), root.path().join("old-review")).unwrap();
    fs::rename(root.path().join("next"), root.path().join("review")).unwrap();
    let third = approve(root.path());
    fails(
        record(root.path(), &third, POST),
        "post URL is already recorded",
    );
    success(record(
        root.path(),
        &third,
        "https://www.linkedin.com/posts/actual-update",
    ));
}

#[test]
fn theme_only_releases_do_not_hide_a_previously_reported_article_revision() {
    let root = prepare();
    let initial: Value =
        serde_json::from_slice(&fs::read(root.path().join("review/manifest.json")).unwrap())
            .unwrap();
    let id = approve(root.path());
    success(record(root.path(), &id, POST));
    fs::write(root.path().join("static/style.css"), "body {color: blue}").unwrap();
    let themed = release(root.path());
    export(
        root.path(),
        initial["before"]["id"].as_str().unwrap(),
        &themed,
        "theme-review",
    );
    let status: Value = serde_json::from_str(&success(run(
        root.path(),
        &[
            "announcement",
            "status",
            "theme-review",
            "--account",
            "personal",
            "--json",
        ],
    )))
    .unwrap();
    assert_eq!(status["reviews"][0]["state"], "review_required");
    assert_eq!(status["reviews"][0]["same_revision_reports"][0], id);
}

#[test]
fn refuses_tampered_manifest_release_and_state_without_partial_output() {
    let root = prepare();
    let manifest = root.path().join("review/manifest.json");
    let bytes = fs::read(&manifest).unwrap();
    let mut value: Value = serde_json::from_slice(&bytes).unwrap();
    value["drafts"][0]["url"] = "https://evil.example".into();
    fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
    fails(
        command(root.path(), "review", &[]),
        "does not match its verified release pair",
    );
    assert!(!root.path().join(".berlin/announcements").exists());
    fs::write(&manifest, bytes).unwrap();
    let sealed_page = root.path().join(format!(
        ".berlin/releases/{}/site/index.html",
        value["after"]["id"].as_str().unwrap()
    ));
    let original_page = fs::read(&sealed_page).unwrap();
    fs::write(&sealed_page, "Modified sealed page").unwrap();
    fails(command(root.path(), "review", &[]), "release files changed");
    assert!(!root.path().join(".berlin/announcements").exists());
    fs::write(&sealed_page, original_page).unwrap();
    let id = approve(root.path());
    let record = root
        .path()
        .join(format!(".berlin/announcements/approvals/{id}.json"));
    let mut value: Value = serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
    value["snapshot"]["text"] = "Tampered text".into();
    fs::write(&record, serde_json::to_vec(&value).unwrap()).unwrap();
    fails(
        run(root.path(), &["announcement", "status", "--json"]),
        "text hash does not match",
    );
    fails(
        command(root.path(), "review", &[]),
        "text hash does not match",
    );
}

#[test]
fn rejects_invalid_text_urls_and_account_names_and_filters_saved_history() {
    let root = prepare();
    for text in ["".to_owned(), "x".repeat(3001)] {
        fs::write(text_file(root.path()), text).unwrap();
        let review = preview(root.path());
        assert_eq!(review["approval_allowed"], false);
        fails(
            command(
                root.path(),
                "review",
                &["--approve", review["review_id"].as_str().unwrap()],
            ),
            "cannot approve empty text",
        );
    }
    fs::write(text_file(root.path()), "Control: \u{001b}[0m").unwrap();
    fails(command(root.path(), "review", &[]), "control characters");
    fs::write(text_file(root.path()), "Reviewed post\n").unwrap();
    let id = approve(root.path());
    let original = snapshot(root.path());
    for url in [
        "http://www.linkedin.com/posts/x",
        "https://www.linkedin.com.evil.test/posts/x",
        "https://user:secret@www.linkedin.com/posts/x",
        "https://www.linkedin.com/",
    ] {
        fails(record(root.path(), &id, url), "use an HTTPS linkedin.com");
    }
    assert_eq!(snapshot(root.path()), original);
    for account in ["", "My account", "../outside", "PRIVATE"] {
        fails(
            run(
                root.path(),
                &[
                    "announcement",
                    "review",
                    "review",
                    "--document",
                    "note",
                    "--account",
                    account,
                ],
            ),
            "stable lowercase local name",
        );
    }
    let status: Value = serde_json::from_str(&success(run(
        root.path(),
        &["announcement", "status", "--account", "company", "--json"],
    )))
    .unwrap();
    assert!(status["history"].as_array().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn refuses_symlinked_drafts_and_ledgers_and_does_not_evaluate_pipeline_overrides() {
    let root = prepare();
    fs::rename(text_file(root.path()), root.path().join("original.txt")).unwrap();
    std::os::unix::fs::symlink(root.path().join("original.txt"), text_file(root.path())).unwrap();
    fails(command(root.path(), "review", &[]), "regular file");
    assert!(!root.path().join(".berlin/announcements").exists());
    fs::remove_file(text_file(root.path())).unwrap();
    fs::rename(root.path().join("original.txt"), text_file(root.path())).unwrap();
    let external = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(external.path(), root.path().join(".berlin/announcements")).unwrap();
    fails(
        run(root.path(), &["announcement", "status", "--json"]),
        "real directory",
    );
    let id = "0".repeat(64);
    fails(
        command(root.path(), "review", &["--approve", &id]),
        "must match the current review ID",
    );
    assert!(fs::read_dir(external.path()).unwrap().next().is_none());
    fails(
        run(
            root.path(),
            &["--pipeline-file", "missing.rhai", "announcement", "status"],
        ),
        "do not accept pipeline files",
    );
}

#[test]
fn empty_status_is_read_only_and_writers_use_a_separate_exclusive_lock() {
    let empty = tempfile::tempdir().unwrap();
    assert!(
        status(empty.path())["history"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(fs::read_dir(empty.path()).unwrap().next().is_none());

    let root = prepare();
    let id = approve(root.path());
    let lock = fs::File::options()
        .read(true)
        .write(true)
        .open(root.path().join(".berlin/announcements/.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let original = snapshot(root.path());
    fails(
        record(root.path(), &id, POST),
        "another announcement writer is active",
    );
    assert_eq!(preview(root.path())["state"], "approved");
    assert_eq!(snapshot(root.path()), original);
    drop(lock);
    success(record(root.path(), &id, POST));
}

#[test]
fn corrupt_or_orphaned_reports_fail_closed_without_altering_history() {
    let root = prepare();
    let id = approve(root.path());
    success(record(root.path(), &id, POST));
    let path = root
        .path()
        .join(format!(".berlin/announcements/reported/{id}.json"));
    let bytes = fs::read(&path).unwrap();
    let mut value: Value = serde_json::from_slice(&bytes).unwrap();
    value["schema_version"] = 99.into();
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let damaged = snapshot(root.path());
    fails(
        run(root.path(), &["announcement", "status", "--json"]),
        "invalid manual publication record",
    );
    fails(
        record(root.path(), &id, POST),
        "invalid manual publication record",
    );
    assert_eq!(snapshot(root.path()), damaged);
    fs::write(&path, bytes).unwrap();
    fs::remove_file(
        root.path()
            .join(format!(".berlin/announcements/approvals/{id}.json")),
    )
    .unwrap();
    let orphaned = snapshot(root.path());
    fails(
        run(root.path(), &["announcement", "status", "--json"]),
        "no matching approval",
    );
    assert_eq!(snapshot(root.path()), orphaned);
}

#[cfg(unix)]
#[test]
fn symlinked_locks_and_saved_records_are_never_followed() {
    let root = prepare();
    let id = approve(root.path());
    let sentinel = root.path().join("sentinel");
    fs::write(&sentinel, "untouched").unwrap();
    let lock = root.path().join(".berlin/announcements/.lock");
    fs::remove_file(&lock).unwrap();
    std::os::unix::fs::symlink(&sentinel, &lock).unwrap();
    fails(record(root.path(), &id, POST), "regular file");
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "untouched");
    let approval = root
        .path()
        .join(format!(".berlin/announcements/approvals/{id}.json"));
    let saved = root.path().join("saved-approval.json");
    fs::rename(&approval, &saved).unwrap();
    std::os::unix::fs::symlink(&saved, &approval).unwrap();
    fails(
        run(root.path(), &["announcement", "status", "--json"]),
        "regular file",
    );
}

#[test]
fn sqlite_migration_preserves_approvals_and_uses_one_authoritative_history() {
    let root = prepare();
    fs::write(text_file(root.path()), "Exact approved text.\r\n").unwrap();
    let id = approve(root.path());
    success(record(root.path(), &id, POST));
    let before = status(root.path());
    let legacy = root
        .path()
        .join(format!(".berlin/announcements/approvals/{id}.json"));
    let bytes = fs::read(&legacy).unwrap();
    success(run(root.path(), &["state", "migrate"]));
    assert_eq!(status(root.path()), before);
    let document: Value =
        serde_json::from_str(&success(run(root.path(), &["state", "document", "note"]))).unwrap();
    let observations = document["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 2);
    let reports = observations
        .iter()
        .flat_map(|o| o["reported_announcements"].as_array().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0]["review_id"], id);
    assert_eq!(reports[0]["account"], "personal");
    assert_eq!(reports[0]["post_url"], POST);
    assert_eq!(reports[0]["evidence"], "user_reported");
    assert_eq!(document["remote_verified"], false);
    assert_eq!(fs::read(&legacy).unwrap(), bytes);
    success(run(root.path(), &["state", "migrate"]));
    success(record(root.path(), &id, POST));
    // Editing archived state cannot change SQLite's authoritative records.
    fs::write(&legacy, "broken archive").unwrap();
    assert_eq!(status(root.path()), before);
    fs::write(text_file(root.path()), "A different reviewable text.\n").unwrap();
    let next = approve(root.path());
    assert!(
        !root
            .path()
            .join(format!(".berlin/announcements/approvals/{next}.json"))
            .exists()
    );
    fails(
        record(root.path(), &next, "https://linkedin.com/posts/other"),
        "already has a reported announcement",
    );
    assert_eq!(status(root.path())["history"].as_array().unwrap().len(), 2);
    success(run(root.path(), &["state", "backup", "backup.sqlite"]));
    fails(
        run(root.path(), &["state", "restore", "backup.sqlite"]),
        "never overwritten",
    );
}
