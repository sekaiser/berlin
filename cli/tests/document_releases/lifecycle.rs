use super::*;

fn state(root: &Path, args: &[&str]) -> Value {
    let mut command = vec!["state"];
    command.extend(args);
    serde_json::from_str(&success(run(root, &command))).unwrap()
}

#[test]
fn metrics_queries_are_offline_read_only_and_do_not_manufacture_engagement_data() {
    let root = fixture();
    state(root.path(), &["migrate"]);
    let before = state(root.path(), &["status"]);
    let empty = state(root.path(), &["metrics", "--document", "note"]);
    assert_eq!(empty["total"], 0);
    assert_eq!(empty["observations"], serde_json::json!([]));
    assert_eq!(empty["collectors_enabled"], false);
    assert_eq!(empty["remote_verified"], false);
    assert_eq!(
        state(root.path(), &["metric-definitions"])["definitions"],
        serde_json::json!([])
    );
    fs::write(
        root.path().join("berlin.pipeline.rhai"),
        "invalid configuration",
    )
    .unwrap();
    assert_eq!(
        state(root.path(), &["metrics", "--document", "note"]),
        empty
    );
    assert_eq!(state(root.path(), &["status"]), before);
}

#[test]
fn indexes_existing_and_new_releases_without_changing_bundles_or_fabricating_publications() {
    let root = fixture();
    let first = release(root.path());
    let before = fs::read(
        root.path()
            .join(".berlin/releases")
            .join(&first)
            .join("manifest.json"),
    )
    .unwrap();
    let publications = root.path().join(".berlin/publications");
    fs::create_dir(&publications).unwrap();
    fs::write(
        publications.join(format!("{first}.json")),
        serde_json::to_vec(&serde_json::json!({
            "schema_version":1,"release":first,"commit":{"sha":"a".repeat(40),"parent":null},
            "status":"uploaded","updated_at_unix_seconds":1,"live_observed_at_unix_seconds":null
        }))
        .unwrap(),
    )
    .unwrap();
    state(root.path(), &["migrate"]);
    assert_eq!(
        state(root.path(), &["status"])["counts"]["sealed_releases"],
        1
    );
    assert_eq!(release(root.path()), first);
    assert_eq!(
        fs::read(
            root.path()
                .join(".berlin/releases")
                .join(&first)
                .join("manifest.json")
        )
        .unwrap(),
        before
    );
    note(root.path(), "note", "Note", "", "Edited prose.");
    let second = release(root.path());
    note(root.path(), "note", "New title", "", "Edited prose.");
    let third = release(root.path());
    assert_ne!(first, second);
    assert_ne!(second, third);
    let list = state(root.path(), &["documents"]);
    assert_eq!(list["documents"][0]["id"], "note");
    assert_eq!(
        list["documents"][0]["titles"],
        serde_json::json!(["New title", "Note"])
    );
    assert_eq!(list["documents"][0]["observed_releases"], 3);
    assert_eq!(list["documents"][0]["content_revisions"], 2);
    assert_eq!(list["documents"][0]["metadata_revisions"], 2);
    let detail = state(root.path(), &["document", "note"]);
    assert_eq!(detail["metrics"]["status"], "no_observations");
    assert_eq!(detail["metrics"]["observation_count"], 0);
    assert!(detail["metrics"]["last_observed_at_unix_seconds"].is_null());
    assert_eq!(detail["remote_verified"], false);
    assert_eq!(detail["bundles_reverified"], false);
    let observations = detail["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 3);
    for observation in observations {
        if observation["release"] == first {
            assert_eq!(observation["website_publication"]["status"], "uploaded");
        } else {
            assert!(observation["website_publication"].is_null());
        }
        assert_eq!(observation["reported_announcements"], serde_json::json!([]));
        assert_eq!(observation["website_url"], "https://example.com/");
    }
    let limited = state(root.path(), &["document", "note", "--limit", "1"]);
    assert_eq!(limited["total"], 3);
    assert_eq!(limited["observations"].as_array().unwrap().len(), 1);
    // Queries need neither pipeline evaluation nor authoring files or bundle bytes.
    fs::write(
        root.path().join("berlin.pipeline.rhai"),
        "invalid configuration",
    )
    .unwrap();
    fs::remove_dir_all(root.path().join(".berlin/releases")).unwrap();
    assert_eq!(state(root.path(), &["document", "note"]), detail);
    assert_eq!(state(root.path(), &["documents"]), list);
    let backup = root.path().join("history.sqlite");
    state(root.path(), &["backup", backup.to_str().unwrap()]);
    let restored = tempfile::tempdir().unwrap();
    state(restored.path(), &["restore", backup.to_str().unwrap()]);
    assert_eq!(state(restored.path(), &["document", "note"]), detail);
}

#[test]
fn indexing_failure_preserves_the_sealed_bundle_and_explicit_indexing_recovers() {
    let root = fixture();
    state(root.path(), &["migrate"]);
    let db = rusqlite::Connection::open(root.path().join(".berlin/state.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER refuse_release BEFORE INSERT ON sealed_releases BEGIN SELECT RAISE(ABORT,'test refusal'); END;").unwrap();
    let result = run(root.path(), &["release"]);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("bundle is sealed but could not be indexed")
    );
    assert_eq!(
        state(root.path(), &["status"])["counts"]["sealed_releases"],
        0
    );
    let releases = fs::read_dir(root.path().join(".berlin/releases"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(releases.len(), 1);
    let original = fs::read(releases[0].join("manifest.json")).unwrap();
    db.execute_batch("DROP TRIGGER refuse_release").unwrap();
    assert_eq!(
        state(root.path(), &["index-releases"])["counts"]["sealed_releases"],
        1
    );
    assert_eq!(
        state(root.path(), &["index-releases"])["counts"]["sealed_releases"],
        1
    );
    assert_eq!(
        fs::read(releases[0].join("manifest.json")).unwrap(),
        original
    );
    // A forged projection cannot silently change what document history reports.
    db.execute("UPDATE release_documents SET title='forged'", [])
        .unwrap();
    assert!(
        !run(root.path(), &["state", "document", "note"])
            .status
            .success()
    );
    assert!(!run(root.path(), &["state", "status"]).status.success());
}

#[test]
fn imported_bytes_have_no_invented_document_inventory_and_bad_bundles_prevent_partial_indexing() {
    let root = fixture();
    success(run(root.path(), &["build"]));
    let imported = success(run(root.path(), &["release", "--from-directory", "_site"]))
        .trim()
        .to_owned();
    state(root.path(), &["migrate"]);
    assert!(manifest(root.path(), &imported).get("documents").is_none());
    assert_eq!(
        state(root.path(), &["documents"])["documents"],
        serde_json::json!([])
    );
    let first = release(root.path());
    let old = state(root.path(), &["status"]);
    let bad = root
        .path()
        .join(".berlin/releases")
        .join(&first)
        .join("site/index.html");
    fs::write(&bad, "changed after sealing").unwrap();
    assert!(
        !run(root.path(), &["state", "index-releases"])
            .status
            .success()
    );
    assert_eq!(state(root.path(), &["status"]), old);
}
