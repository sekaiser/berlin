//! Announcement workflow tests reuse the native-release publishing fixture.

use super::{PIPELINE, fixture, note, release, run, success};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Eq, PartialEq)]
pub(super) enum Entry {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
}

pub(super) fn snapshot(root: &Path) -> BTreeMap<PathBuf, Entry> {
    fn collect(root: &Path, directory: &Path, entries: &mut BTreeMap<PathBuf, Entry>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let kind = fs::symlink_metadata(&path).unwrap();
            let value = if kind.is_symlink() {
                Entry::Symlink(fs::read_link(&path).unwrap())
            } else if kind.is_dir() {
                collect(root, &path, entries);
                Entry::Directory
            } else {
                Entry::File(fs::read(&path).unwrap())
            };
            entries.insert(path.strip_prefix(root).unwrap().into(), value);
        }
    }
    let mut entries = BTreeMap::new();
    collect(root, root, &mut entries);
    entries
}

fn prepare_pair(root: &Path) -> (String, String) {
    let before = release(root);
    note(root, "note", "Note", "", "Revised body.");
    (before, release(root))
}

#[test]
fn preview_is_deterministic_offline_and_selects_only_announcement_candidates() {
    let root = fixture();
    fs::write(
        root.path().join("berlin.pipeline.rhai"),
        PIPELINE.replace("https://example.com", "https://example.com/notebook"),
    )
    .unwrap();
    note(root.path(), "meta", "Metadata", "", "Same body.");
    note(root.path(), "removed", "Removed", "", "Removed body.");
    note(root.path(), "unchanged", "Unchanged", "", "Same body.");
    let before = release(root.path());
    note(root.path(), "note", "Note", "", "Revised body.");
    note(root.path(), "a-added", "New entry", "", "New prose.");
    note(root.path(), "z-added", "Another entry", "", "More prose.");
    note(
        root.path(),
        "meta",
        "Metadata",
        "lastmod: 2026-09-09\n",
        "Same body.",
    );
    note(
        root.path(),
        "private",
        "Private title",
        "draft: true\n",
        "Private prose.",
    );
    fs::remove_file(root.path().join("content/removed.md")).unwrap();
    let after = release(root.path());
    // Neither current authoring inputs, a recovery journal, nor local publication
    // observations may change the plan or trigger a build/recovery operation.
    fs::remove_file(root.path().join("berlin.pipeline.rhai")).unwrap();
    fs::write(
        root.path().join(".berlin-transaction.json"),
        "invalid recovery journal",
    )
    .unwrap();
    fs::create_dir_all(root.path().join(".berlin/publications")).unwrap();
    fs::write(
        root.path()
            .join(".berlin/publications")
            .join(format!("{after}.json")),
        "not a live-state oracle",
    )
    .unwrap();
    let original = snapshot(root.path());
    let args = ["release-drafts", &before, &after, "--json"];
    let output = success(run(root.path(), &args));
    assert_eq!(success(run(root.path(), &args)), output);
    let plan: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(plan["schema_version"], 1);
    assert_eq!(plan["channel"], "linkedin");
    assert_eq!(plan["before"]["id"], before);
    assert_eq!(plan["after"]["id"], after);
    assert_eq!(plan["review_required"], true);
    assert_eq!(plan["live_status_checked"], false);
    assert_eq!(
        plan["drafts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["document"]["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["a-added", "note", "z-added"]
    );
    assert_eq!(plan["drafts"][0]["kind"], "new");
    assert_eq!(plan["drafts"][1]["kind"], "updated");
    assert_eq!(
        plan["drafts"][1]["draft"]["text"],
        "Updated: Note\n\nhttps://example.com/notebook/notes/note.html"
    );
    assert_eq!(plan["skipped"]["metadata_or_route_only"], 1);
    assert_eq!(plan["skipped"]["removed"], 1);
    assert_eq!(plan["skipped"]["unchanged"], 1);
    for hidden in [
        "file:///",
        "Revised body.",
        "Private",
        "private",
        "Removed body.",
    ] {
        assert!(!output.contains(hidden));
    }
    let text = success(run(root.path(), &["release-drafts", &before, &after]));
    assert!(text.contains("review required; live status not checked"));
    assert!(text.contains("not summaries"));
    assert!(text.contains("Updated: Note"));
    assert_eq!(snapshot(root.path()), original);
}

#[test]
fn explicit_export_creates_editable_files_but_never_overwrites_a_review() {
    let root = fixture();
    let (before, after) = prepare_pair(root.path());
    let output = success(run(
        root.path(),
        &[
            "release-drafts",
            &before,
            &after,
            "--json",
            "--output",
            "review",
        ],
    ));
    let plan: Value = serde_json::from_str(&output).unwrap();
    let review = root.path().join("review");
    let saved: Value =
        serde_json::from_slice(&fs::read(review.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(saved, plan);
    let filename = format!(
        "{}.txt",
        plan["drafts"][0]["draft"]["artifact_name"]
            .as_str()
            .unwrap()
    );
    assert_eq!(
        fs::read_to_string(review.join(&filename)).unwrap(),
        format!("{}\n", plan["drafts"][0]["draft"]["text"].as_str().unwrap())
    );
    fs::write(review.join(filename), "My reviewed announcement").unwrap();
    let original = snapshot(root.path());
    let rerun = run(
        root.path(),
        &["release-drafts", &before, &after, "--output", "review"],
    );
    assert!(!rerun.status.success());
    assert!(rerun.stdout.is_empty());
    assert!(String::from_utf8_lossy(&rerun.stderr).contains("must not already exist"));
    assert_eq!(snapshot(root.path()), original);

    let external = tempfile::tempdir().unwrap();
    let path = external.path().join("review");
    success(run(
        root.path(),
        &[
            "release-drafts",
            &before,
            &after,
            "--output",
            path.to_str().unwrap(),
        ],
    ));
    assert!(path.join("manifest.json").is_file());
}

#[test]
fn export_refuses_existing_empty_directories_missing_parents_and_release_state() {
    let root = fixture();
    let (before, after) = prepare_pair(root.path());
    fs::create_dir(root.path().join("empty-review")).unwrap();
    fs::create_dir(root.path().join(".berlin/announcements")).unwrap();
    let original = snapshot(root.path());
    for output in [
        "empty-review".to_owned(),
        "missing-parent/review".into(),
        format!(".berlin/releases/{after}/review"),
        ".berlin/releases/review".into(),
        ".berlin/announcements/review".into(),
    ] {
        let result = run(
            root.path(),
            &["release-drafts", &before, &after, "--output", &output],
        );
        assert!(!result.status.success(), "{output}");
        assert!(result.stdout.is_empty());
        assert_eq!(snapshot(root.path()), original);
    }
}

#[cfg(unix)]
#[test]
fn export_does_not_follow_existing_review_links_or_aliases_into_release_state() {
    let root = fixture();
    let (before, after) = prepare_pair(root.path());
    let elsewhere = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), root.path().join("review-link")).unwrap();
    std::os::unix::fs::symlink(
        root.path().join(".berlin/releases"),
        root.path().join("release-alias"),
    )
    .unwrap();
    let original = snapshot(root.path());
    for output in ["review-link", "release-alias/review"] {
        assert!(
            !run(
                root.path(),
                &["release-drafts", &before, &after, "--output", output]
            )
            .status
            .success()
        );
    }
    assert!(fs::read_dir(elsewhere.path()).unwrap().next().is_none());
    assert_eq!(snapshot(root.path()), original);
}

#[test]
fn legacy_history_and_tampered_releases_fail_before_export_or_partial_output() {
    let root = fixture();
    let (before, after) = prepare_pair(root.path());
    let imported = success(run(
        root.path(),
        &[
            "release",
            "--from-directory",
            &format!(".berlin/releases/{before}/site"),
        ],
    ))
    .trim()
    .to_owned();
    let original = snapshot(root.path());
    for (old, new) in [(&imported, &after), (&before, &imported)] {
        let result = run(
            root.path(),
            &["release-drafts", old, new, "--output", "review", "--json"],
        );
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.stderr).contains("require document inventories"));
    }
    assert_eq!(snapshot(root.path()), original);
    for id in [&before, &after] {
        let file = root
            .path()
            .join(".berlin/releases")
            .join(id)
            .join("site/index.html");
        let bytes = fs::read(&file).unwrap();
        fs::write(&file, "Unreviewed replacement").unwrap();
        let original = snapshot(root.path());
        let result = run(
            root.path(),
            &["release-drafts", &before, &after, "--output", "review"],
        );
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert_eq!(snapshot(root.path()), original);
        fs::write(&file, bytes).unwrap();
    }
}

#[test]
fn metadata_theme_and_route_only_changes_do_not_generate_update_posts() {
    let root = fixture();
    let before = release(root.path());
    note(
        root.path(),
        "note",
        "Renamed title",
        "slug: renamed\nlastmod: 2026-09-09\n",
        "Initial prose.",
    );
    fs::write(root.path().join("static/style.css"), "body {color: brown}").unwrap();
    let after = release(root.path());
    let plan: Value = serde_json::from_str(&success(run(
        root.path(),
        &["release-drafts", &before, &after, "--json"],
    )))
    .unwrap();
    assert!(plan["drafts"].as_array().unwrap().is_empty());
    assert_eq!(plan["skipped"]["metadata_or_route_only"], 1);
    let unchanged: Value = serde_json::from_str(&success(run(
        root.path(),
        &["release-drafts", &after, &after, "--json"],
    )))
    .unwrap();
    assert!(unchanged["drafts"].as_array().unwrap().is_empty());
    assert_eq!(unchanged["skipped"]["unchanged"], 1);
}

#[test]
fn changed_destinations_and_pipeline_overrides_require_explicit_different_workflows() {
    let root = fixture();
    let before = release(root.path());
    for source in [
        PIPELINE.replace("owner/site", "owner/other"),
        PIPELINE.replace("https://example.com", "https://example.com/new-prefix"),
    ] {
        fs::write(root.path().join("berlin.pipeline.rhai"), source).unwrap();
        let after = release(root.path());
        let original = snapshot(root.path());
        let result = run(
            root.path(),
            &["release-drafts", &before, &after, "--output", "review"],
        );
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("same website URL and destination")
        );
        assert_eq!(snapshot(root.path()), original);
    }
    for args in [
        vec![
            "--pipeline-file",
            "override.rhai",
            "release-drafts",
            &before,
            &before,
        ],
        vec![
            "release-drafts",
            &before,
            &before,
            "--pipeline-file",
            "override.rhai",
        ],
    ] {
        assert!(!run(root.path(), &args).status.success());
    }
}
