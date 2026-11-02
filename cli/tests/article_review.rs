use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const ARTICLE: &str = ":PROPERTIES:\n:ID: test-article\n:END:\n#+TITLE: A bounded cache\n#+HUGO_DRAFT: true\n\nA later observation repairs a dropped update.\nBut a device may stop reporting.\n";
fn digest(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bln"))
        .env("BERLIN_DIR", root)
        .env("PATH", root.join("no-tools"))
        .env("BERLIN_EMACS", "must-not-run")
        .args(args)
        .output()
        .unwrap()
}
fn ok(root: &Path, args: &[&str]) -> String {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "{:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn value(root: &Path, args: &[&str]) -> Value {
    serde_json::from_str(&ok(root, args)).unwrap()
}
fn refused(root: &Path, args: &[&str], message: &str) {
    let output = run(root, args);
    assert!(!output.status.success(), "unexpected success: {args:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(message),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("draft.org"), ARTICLE).unwrap();
    fs::create_dir(root.path().join("_site")).unwrap();
    fs::write(root.path().join("_site/index.html"), "Existing website").unwrap();
    // State commands must not attempt to recover or execute this invalid pipeline.
    fs::write(root.path().join("berlin.pipeline.rhai"), "MUST NOT EXECUTE").unwrap();
    fs::write(
        root.path().join(".berlin-transaction.json"),
        "untouched recovery evidence",
    )
    .unwrap();
    ok(root.path(), &["state", "migrate"]);
    root
}
fn start(root: &Path, article: &str) -> Value {
    start_with_editorial(root, article, None)
}
fn start_args<'a>(article: &'a str, editorial: Option<&'a str>) -> Vec<&'a str> {
    let mut args = vec![
        "review",
        "start",
        article,
        "--audience",
        "Staff engineers familiar with queues",
        "--goal",
        "Choose an overload policy",
        "--thesis",
        "Bound the optional cache work",
        "--prerequisites",
        "Basic streaming semantics",
    ];
    if let Some(file) = editorial {
        args.extend(["--editorial-brief", file]);
    }
    args
}
fn start_with_editorial(root: &Path, article: &str, editorial: Option<&str>) -> Value {
    value(root, &start_args(article, editorial))
}
fn report(root: &Path, id: &str) -> Value {
    let packet = value(root, &["review", "packet", id]);
    assert!(
        packet["source_lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line["text"] == "But a device may stop reporting.")
    );
    let mut report = packet["report_template"].clone();
    report["reviewer"] = json!("Human reviewer (synthetic test)");
    report["summary"] = json!("The recovery assumption needs a boundary.");
    report["limitations"] = json!(
        "Reviewed the supplied source only; no code execution or independent evidence verification."
    );
    report["findings"] = json!([{
        "start_line":7,"end_line":8,
        "quote":"A later observation repairs a dropped update.\nBut a device may stop reporting.",
        "question":"How does recovery work if the device never reports again?",
        "reason":"The freshness argument assumes another observation.",
        "evidence":[{"reference":"Captured article, lines 7–8","method":"reasoning","assessment":"unresolved","details":"No reconciliation mechanism is described in the source."}]
    }]);
    report
}
fn save(root: &Path, report: &Value) {
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(report).unwrap(),
    )
    .unwrap();
}

#[test]
fn complete_dialogue_preserves_deferrals_history_and_backup_without_publishing() {
    let root = fixture();
    let root = root.path();
    let started = start(root, "draft.org");
    assert_eq!(started["id"], 1);
    assert_eq!(started["revision_status"], "current");
    assert_eq!(started["step"], "audience");
    refused(
        root,
        &["review", "advance", "1", "--reflection", "Ready"],
        "record a report",
    );
    for step in ["audience", "argument", "challenge", "verification"] {
        let report = report(root, "1");
        assert_eq!(report["step"], step);
        save(root, &report);
        let recorded = value(root, &["review", "record", "1", "report.json"]);
        assert_eq!(recorded["step"], step);
        refused(
            root,
            &["review", "advance", "1", "--reflection", "Ready"],
            "respond to every finding",
        );
        refused(
            root,
            &["review", "record", "1", "report.json"],
            "already has a report",
        );
        refused(
            root,
            &[
                "review",
                "respond",
                "1",
                "2",
                "--disposition",
                "addressed",
                "--response",
                "No second finding",
            ],
            "unknown finding",
        );
        let disposition = if step == "challenge" {
            "deferred"
        } else {
            "disputed"
        };
        value(
            root,
            &[
                "review",
                "respond",
                "1",
                "1",
                "--disposition",
                disposition,
                "--response",
                "The authoritative journal can supply a repair; the design must state that dependency.",
                "--evidence",
                "Planned source review; not yet executed.",
            ],
        );
        if step == "audience" {
            value(
                root,
                &[
                    "review",
                    "respond",
                    "1",
                    "1",
                    "--disposition",
                    "addressed",
                    "--response",
                    "Plan a paragraph explaining repair and its limits.",
                ],
            );
        }
        value(
            root,
            &[
                "review",
                "advance",
                "1",
                "--reflection",
                "Explain recovery before recommending the drop policy; retain the open operational question.",
            ],
        );
    }
    let complete = value(root, &["review", "show", "1", "--json"]);
    assert_eq!(complete["status"], "completed");
    assert_eq!(complete["step"], Value::Null);
    assert_eq!(complete["outstanding"], 1);
    assert_eq!(complete["event_count"], 13);
    assert!(ok(root, &["review", "show", "1"]).contains("Deferred"));
    refused(root, &["review", "packet", "1"], "already completed");
    refused(
        root,
        &["review", "advance", "1", "--reflection", "Again"],
        "already completed",
    );
    let list = value(
        root,
        &[
            "review",
            "list",
            "--document",
            "test-article",
            "--limit",
            "1",
        ],
    );
    assert_eq!(list["total"], 1);
    assert_eq!(list["reviews"][0]["outstanding"], 1);
    assert_eq!(
        value(root, &["review", "list", "--document", "other"])["total"],
        0
    );
    let status = value(root, &["state", "status"]);
    assert_eq!(status["schema_version"], 6);
    assert_eq!(status["counts"]["article_reviews"], 1);
    assert_eq!(status["counts"]["article_review_events"], 13);
    for table in [
        "build_runs",
        "release_attempts",
        "sealed_releases",
        "publications",
        "metric_observations",
    ] {
        assert_eq!(status["counts"][table], 0);
    }
    ok(root, &["state", "backup", "review.sqlite"]);
    let restored = tempfile::tempdir().unwrap();
    ok(
        restored.path(),
        &[
            "state",
            "restore",
            root.join("review.sqlite").to_str().unwrap(),
        ],
    );
    assert_eq!(
        value(restored.path(), &["review", "show", "1", "--json"]),
        complete
    );
    assert_eq!(fs::read_to_string(root.join("draft.org")).unwrap(), ARTICLE);
    assert_eq!(
        fs::read_to_string(root.join("_site/index.html")).unwrap(),
        "Existing website"
    );
    assert_eq!(
        fs::read_to_string(root.join(".berlin-transaction.json")).unwrap(),
        "untouched recovery evidence"
    );
}

#[test]
fn mismatched_reports_cannot_mutate_a_review_and_stale_sources_stay_historical() {
    let root = fixture();
    let root = root.path();
    start(root, "draft.org");
    let original = report(root, "1");
    for (pointer, replacement, message) in [
        ("/review_id", json!(2), "another review"),
        ("/source_sha256", json!("a".repeat(64)), "source revision"),
        ("/step", json!("verification"), "current step"),
        (
            "/findings/0/quote",
            json!("A fabricated quote"),
            "quote does not match",
        ),
        ("/findings/0/end_line", json!(5000), "line range"),
        ("/findings/0/start_line", json!(0), "line range"),
        ("/limitations", json!(" "), "nonempty"),
        (
            "/findings/0/evidence/0/assessment",
            json!("proven"),
            "unknown variant",
        ),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        save(root, &changed);
        refused(root, &["review", "record", "1", "report.json"], message);
        assert_eq!(
            value(root, &["review", "show", "1", "--json"])["event_count"],
            0
        );
    }
    let mut changed = original.clone();
    changed["findings"] = json!(vec![original["findings"][0].clone(); 6]);
    save(root, &changed);
    refused(
        root,
        &["review", "record", "1", "report.json"],
        "five consequential",
    );
    let mut changed = original.clone();
    changed["invented_quality_score"] = json!(99);
    save(root, &changed);
    refused(
        root,
        &["review", "record", "1", "report.json"],
        "unknown field",
    );
    save(root, &original);
    fs::write(
        root.join("draft.org"),
        ARTICLE.replace("repairs", "may repair"),
    )
    .unwrap();
    assert_eq!(
        value(root, &["review", "show", "1", "--json"])["revision_status"],
        "stale"
    );
    refused(
        root,
        &["review", "record", "1", "report.json"],
        "historical",
    );
    refused(root, &["review", "packet", "1"], "historical");
    let second = start(root, "draft.org");
    assert_eq!(second["id"], 2);
    assert_ne!(
        second["article"]["source_sha256"],
        original["source_sha256"]
    );
    assert_eq!(value(root, &["review", "list"])["total"], 2);
    fs::remove_file(root.join("draft.org")).unwrap();
    assert_eq!(
        value(root, &["review", "show", "1", "--json"])["revision_status"],
        "unavailable"
    );
    // Storage integrity and backup do not depend on the original source surviving.
    ok(root, &["state", "backup", "history.sqlite"]);
}

#[test]
fn markdown_identity_limits_and_explicit_storage_activation() {
    let empty = tempfile::tempdir().unwrap();
    fs::write(empty.path().join("draft.org"), ARTICLE).unwrap();
    refused(
        empty.path(),
        &[
            "review",
            "start",
            "draft.org",
            "--audience",
            "Reader",
            "--goal",
            "Understand",
            "--thesis",
            "Bound work",
        ],
        "state migrate",
    );
    assert!(!empty.path().join(".berlin/state.sqlite").exists());
    let root = fixture();
    let root = root.path();
    fs::write(
        root.join("draft.md"),
        "---\nid: markdown-id\ntitle: A draft\ndraft: true\n---\nA claim.\n",
    )
    .unwrap();
    assert_eq!(
        start(root, "draft.md")["article"]["document_id"],
        "markdown-id"
    );
    fs::write(root.join("draft.md"), "---\ntitle: No ID\n---\nA claim.\n").unwrap();
    refused(
        root,
        &[
            "review",
            "start",
            "draft.md",
            "--audience",
            "Reader",
            "--goal",
            "Understand",
            "--thesis",
            "Bound work",
        ],
        "explicit frontmatter id",
    );
    fs::write(
        root.join("not-roam.org"),
        "#+TITLE: Missing ID\n* Heading\n:PROPERTIES:\n:ID: heading-only\n:END:\n",
    )
    .unwrap();
    refused(
        root,
        &[
            "review",
            "start",
            "not-roam.org",
            "--audience",
            "Reader",
            "--goal",
            "Understand",
            "--thesis",
            "Bound work",
        ],
        "file-level property drawer",
    );
    fs::write(root.join("large.org"), "x".repeat(1024 * 1024 + 1)).unwrap();
    refused(
        root,
        &[
            "review",
            "start",
            "large.org",
            "--audience",
            "Reader",
            "--goal",
            "Understand",
            "--thesis",
            "Bound work",
        ],
        "size limit",
    );
    refused(
        root,
        &["--pipeline-file", "missing.rhai", "review", "list"],
        "does not evaluate pipelines",
    );
    refused(root, &["review", "list", "--limit", "0"], "invalid value");
    assert_eq!(value(root, &["review", "list"])["total"], 1);
}

#[test]
fn schema_five_upgrade_and_review_integrity_are_checked_on_restore() {
    let root = fixture();
    let root = root.path();
    {
        let db = rusqlite::Connection::open(root.join(".berlin/state.sqlite")).unwrap();
        db.execute_batch(
            "DROP TABLE article_review_events; DROP TABLE article_reviews; PRAGMA user_version=5;",
        )
        .unwrap();
    }
    assert_eq!(
        value(root, &["state", "status"])["migration_required"],
        true
    );
    ok(root, &["state", "backup", "schema-five.sqlite"]);
    refused(root, &["review", "list"], "needs upgrading");
    ok(root, &["state", "migrate"]);
    ok(root, &["state", "migrate"]);
    start(root, "draft.org");
    let r = report(root, "1");
    save(root, &r);
    ok(root, &["review", "record", "1", "report.json"]);
    {
        let db = rusqlite::Connection::open(root.join(".berlin/state.sqlite")).unwrap();
        db.execute(
            "UPDATE article_review_events SET ordinal=2 WHERE ordinal=0",
            [],
        )
        .unwrap();
    }
    refused(root, &["state", "status"], "event sequence mismatch");
    refused(
        root,
        &["state", "backup", "invalid.sqlite"],
        "event sequence mismatch",
    );
    assert!(!root.join("invalid.sqlite").exists());
    let restored = tempfile::tempdir().unwrap();
    ok(
        restored.path(),
        &[
            "state",
            "restore",
            root.join("schema-five.sqlite").to_str().unwrap(),
        ],
    );
    assert_eq!(
        value(restored.path(), &["state", "status"])["schema_version"],
        5
    );
    ok(restored.path(), &["state", "migrate"]);
    assert_eq!(value(restored.path(), &["review", "list"])["total"], 0);
}

#[test]
fn editorial_capture_stays_private_across_file_changes_all_steps_and_restore() {
    let root = fixture();
    let root = root.path();
    let external = tempfile::tempdir().unwrap();
    let guide = external.path().join("principles.org");
    let text = "#+TITLE: Private editorial principles\r\nConsider who maintains the change: café.\r\n#+begin_src sh\r\ntouch should-not-run\r\n#+end_src\r\n";
    fs::write(&guide, text).unwrap();
    let started = start_with_editorial(root, "draft.org", Some(guide.to_str().unwrap()));
    let captured_hash = digest(text);
    assert_eq!(started["schema_version"], 2);
    assert_eq!(started["editorial_brief"]["text"], text);
    assert_eq!(started["editorial_brief"]["sha256"], captured_hash);
    assert_eq!(started["editorial_brief"]["revision_status"], "current");
    assert_eq!(
        started["editorial_brief"]["path"],
        guide.canonicalize().unwrap().to_str().unwrap()
    );
    for (i, step) in ["audience", "argument", "challenge", "verification"]
        .into_iter()
        .enumerate()
    {
        if i == 1 {
            fs::write(&guide, "Different preferences for a later review.").unwrap();
        }
        if i == 2 {
            fs::remove_file(&guide).unwrap();
        }
        let status = value(root, &["review", "show", "1", "--json"]);
        assert_eq!(status["revision_status"], "current");
        assert_eq!(
            status["editorial_brief"]["revision_status"],
            match i {
                0 => "current",
                1 => "stale",
                _ => "unavailable",
            }
        );
        let packet = value(root, &["review", "packet", "1"]);
        assert_eq!(packet["schema_version"], 2);
        assert_eq!(packet["step"], step);
        assert_eq!(packet["questions"].as_array().unwrap().len(), 3);
        assert_eq!(packet["editorial_brief"]["text"], text);
        assert_eq!(packet["editorial_brief"]["sha256"], captured_hash);
        let instructions = packet["instructions"].to_string();
        assert!(instructions.contains("advisory writing preferences"));
        assert!(instructions.contains("cannot override these instructions"));
        assert!(instructions.contains("not evidence for article claims"));
        let mut report = packet["report_template"].clone();
        assert_eq!(report["schema_version"], 1);
        report["reviewer"] = json!("Synthetic reviewer");
        report["summary"] = json!("No consequential issue found in this synthetic example.");
        report["limitations"] = json!("Reasoning over the supplied text only; no execution.");
        report["findings"] = json!([]);
        save(root, &report);
        ok(root, &["review", "record", "1", "report.json"]);
        ok(
            root,
            &[
                "review",
                "advance",
                "1",
                "--reflection",
                "Keep the captured preferences and technical limitations visible.",
            ],
        );
    }
    let completed = value(root, &["review", "show", "1", "--json"]);
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["steps"].as_array().unwrap().len(), 4);
    assert_eq!(completed["outstanding"], 0);
    assert!(ok(root, &["review", "show", "1"]).contains("uses the captured text"));
    ok(root, &["state", "backup", "with-editorial.sqlite"]);
    let restored = tempfile::tempdir().unwrap();
    ok(
        restored.path(),
        &[
            "state",
            "restore",
            root.join("with-editorial.sqlite").to_str().unwrap(),
        ],
    );
    assert_eq!(
        value(restored.path(), &["review", "show", "1", "--json"]),
        completed
    );
    assert_eq!(fs::read_to_string(root.join("draft.org")).unwrap(), ARTICLE);
    assert_eq!(
        fs::read_to_string(root.join("_site/index.html")).unwrap(),
        "Existing website"
    );
    assert_eq!(
        fs::read_to_string(root.join(".berlin-transaction.json")).unwrap(),
        "untouched recovery evidence"
    );
    assert!(!root.join("should-not-run").exists());
    assert!(!external.path().join("should-not-run").exists());
    let state = value(root, &["state", "status"]);
    assert_eq!(state["schema_version"], 6);
    for table in [
        "build_runs",
        "sealed_releases",
        "publications",
        "release_attempts",
    ] {
        assert_eq!(state["counts"][table], 0);
    }
}

#[test]
fn editorial_inputs_are_bounded_and_optional_legacy_payloads_are_not_rewritten() {
    let root = fixture();
    let root = root.path();
    let plain = start(root, "draft.org");
    assert_eq!(plain["schema_version"], 1);
    assert!(plain.get("editorial_brief").is_none());
    assert!(
        value(root, &["review", "packet", "1"])
            .get("editorial_brief")
            .is_none()
    );
    let db = rusqlite::Connection::open(root.join(".berlin/state.sqlite")).unwrap();
    let payload = || {
        db.query_row("SELECT payload FROM article_reviews WHERE id=1", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap()
    };
    let original = payload();
    let legacy: Value = serde_json::from_str(&original).unwrap();
    assert_eq!(legacy["schema_version"], 1);
    assert!(legacy.get("editorial_brief").is_none());
    fs::write(root.join("empty.txt"), " \n\t").unwrap();
    fs::write(root.join("large.txt"), "x".repeat(64 * 1024 + 1)).unwrap();
    fs::write(root.join("binary.txt"), [0xff, 0xfe]).unwrap();
    fs::create_dir(root.join("directory.txt")).unwrap();
    for (file, message) in [
        ("empty.txt", "nonempty"),
        ("large.txt", "size limit"),
        ("binary.txt", "UTF-8"),
        ("directory.txt", "regular file"),
        ("missing.txt", "No such file"),
    ] {
        refused(root, &start_args("draft.org", Some(file)), message);
        assert_eq!(value(root, &["review", "list"])["total"], 1);
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("empty.txt"), root.join("linked.txt")).unwrap();
        refused(
            root,
            &start_args("draft.org", Some("linked.txt")),
            "regular file",
        );
    }
    // The separate brief limit permits more than the ordinary 16 KiB text field.
    // UTF-8 bytes, not characters, determine the 64 KiB boundary.
    let boundary = "é".repeat(32 * 1024);
    fs::write(root.join("boundary.txt"), &boundary).unwrap();
    let with_brief = start_with_editorial(root, "draft.org", Some("boundary.txt"));
    assert_eq!(with_brief["id"], 2);
    assert_eq!(with_brief["editorial_brief"]["text"], boundary);
    assert_eq!(value(root, &["review", "show", "1", "--json"]), plain);
    ok(root, &["state", "backup", "legacy-and-editorial.sqlite"]);
    assert_eq!(payload(), original);
}

#[test]
fn stored_editorial_text_is_validated_without_reading_its_original_file() {
    let root = fixture();
    let root = root.path();
    fs::write(
        root.join("principles.txt"),
        "Explain evidence and consequences.",
    )
    .unwrap();
    start_with_editorial(root, "draft.org", Some("principles.txt"));
    fs::remove_file(root.join("principles.txt")).unwrap();
    ok(root, &["state", "backup", "valid.sqlite"]);
    let db = rusqlite::Connection::open(root.join(".berlin/state.sqlite")).unwrap();
    let raw: String = db
        .query_row("SELECT payload FROM article_reviews WHERE id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    let mut payload: Value = serde_json::from_str(&raw).unwrap();
    payload["editorial_brief"]["text"] = json!("Different uncaptured preferences.");
    let altered = serde_json::to_string(&payload).unwrap();
    let digest = digest(&altered);
    db.execute(
        "UPDATE article_reviews SET payload=?1,digest=?2 WHERE id=1",
        rusqlite::params![altered, digest],
    )
    .unwrap();
    refused(root, &["state", "status"], "editorial brief hash mismatch");
    refused(
        root,
        &["state", "backup", "invalid.sqlite"],
        "editorial brief hash mismatch",
    );
    assert!(!root.join("invalid.sqlite").exists());
}
