use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const PIPELINE: &str = r#"
fn prepared(c) { publication("prepared",#{title:"Prepared"},[view_component("berlin.introduction@1",#{text:"Imported bytes are not rebuilt."},#{})]) }
pipeline site {
    let presentation = html_config(#{url: "https://example.com"});
    let sources = catalog([documents(load_markdown("missing/*.md"))]);
    output render_html(compose(sources,Fn("prepared")), "_site", presentation)
        .deploy_to(github_pages("owner/site"));
}
"#;

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("berlin.pipeline.rhai"), PIPELINE).unwrap();
    fs::create_dir(root.path().join("prepared")).unwrap();
    fs::write(root.path().join("prepared/index.html"), "first").unwrap();
    root
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bln"))
        .env("BERLIN_DIR", root)
        // These local operations must not need Git, gh, Emacs, or other tools.
        .env("PATH", root.join("no-external-tools"))
        .args(args)
        .output()
        .unwrap()
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn seal(root: &Path) -> String {
    success(run(root, &["release", "--from-directory", "prepared"]))
        .trim()
        .to_owned()
}

fn bundle(root: &Path, id: &str) -> PathBuf {
    root.join(".berlin/releases").join(id)
}

fn compare(root: &Path, before: &str, after: &str) -> Value {
    serde_json::from_str(&success(run(
        root,
        &["release-diff", before, after, "--json"],
    )))
    .unwrap()
}

fn paths(files: &Value) -> Vec<&str> {
    files
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect()
}

#[derive(Debug, Eq, PartialEq)]
enum Entry {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
}

// Include empty directories and links so state creation is visible too.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Entry> {
    fn collect(root: &Path, directory: &Path, entries: &mut BTreeMap<PathBuf, Entry>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            let value = if metadata.is_symlink() {
                Entry::Symlink(fs::read_link(&path).unwrap())
            } else if metadata.is_dir() {
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

#[test]
fn compares_file_bytes_in_both_directions_and_reports_renames_as_add_remove() {
    let root = fixture();
    let prepared = root.path().join("prepared");
    fs::write(prepared.join("a-removed.bin"), [0, 255]).unwrap();
    fs::write(prepared.join("z-original.bin"), [1, 254]).unwrap();
    fs::write(prepared.join("unchanged.bin"), [2, 253]).unwrap();
    let before = seal(root.path());
    let manifest: Value = serde_json::from_slice(
        &fs::read(bundle(root.path(), &before).join("manifest.json")).unwrap(),
    )
    .unwrap();

    fs::write(prepared.join("index.html"), "other").unwrap();
    fs::remove_file(prepared.join("a-removed.bin")).unwrap();
    fs::create_dir(prepared.join("nested")).unwrap();
    fs::rename(
        prepared.join("z-original.bin"),
        prepared.join("nested/moved.bin"),
    )
    .unwrap();
    fs::write(prepared.join("nested/日本語.bin"), [255, 0, 254]).unwrap();
    let after = seal(root.path());
    let state = snapshot(root.path());

    let diff = compare(root.path(), &before, &after);
    assert_eq!(diff["schema_version"], 2);
    assert_eq!(diff["documents"]["status"], "unavailable");
    assert_eq!(diff["before"]["id"], before);
    assert_eq!(diff["after"]["id"], after);
    assert_eq!(diff["metadata_changed"], false);
    let files = &diff["files"];
    assert_eq!(
        paths(&files["added"]),
        ["nested/moved.bin", "nested/日本語.bin"]
    );
    assert_eq!(
        paths(&files["removed"]),
        ["a-removed.bin", "z-original.bin"]
    );
    assert_eq!(files["modified"].as_array().unwrap().len(), 1);
    let modified = &files["modified"][0];
    assert_eq!(modified["before"]["path"], "index.html");
    assert_eq!(modified["after"]["path"], "index.html");
    assert_eq!(modified["before"]["bytes"], 5);
    assert_eq!(modified["after"]["bytes"], 5);
    assert_ne!(modified["before"]["sha256"], modified["after"]["sha256"]);
    assert_eq!(
        files["unchanged"],
        manifest["files"].as_array().unwrap().len() - 3
    );
    assert_eq!(compare(root.path(), &before, &after), diff);

    let reverse = compare(root.path(), &after, &before);
    assert_eq!(reverse["files"]["added"], files["removed"]);
    assert_eq!(reverse["files"]["removed"], files["added"]);
    assert_eq!(reverse["files"]["unchanged"], files["unchanged"]);
    assert_eq!(reverse["files"]["modified"][0]["before"], modified["after"]);
    assert_eq!(reverse["files"]["modified"][0]["after"], modified["before"]);
    let text = success(run(root.path(), &["release-diff", &before, &after]));
    assert!(text.contains("2 added, 1 modified, 2 removed"));
    assert!(text.contains("~ index.html (5 -> 5 bytes)"));
    assert!(text.contains("+ nested/moved.bin (2 bytes)"));
    assert!(text.contains("- a-removed.bin (2 bytes)"));
    assert_eq!(snapshot(root.path()), state);
}

#[test]
fn compares_identical_releases_without_authoring_inputs_or_writing_state() {
    let root = fixture();
    let id = seal(root.path());
    fs::remove_file(root.path().join("berlin.pipeline.rhai")).unwrap();
    // Taking the project lock would attempt recovery and fail on this journal.
    fs::write(root.path().join(".berlin-transaction.json"), "unreadable").unwrap();
    fs::create_dir_all(root.path().join(".berlin/publications")).unwrap();
    fs::write(
        root.path()
            .join(".berlin/publications")
            .join(format!("{id}.json")),
        "Do not consult or change recorded publication state",
    )
    .unwrap();
    let state = snapshot(root.path());
    let diff = compare(root.path(), &id, &id);
    assert_eq!(diff["metadata_changed"], false);
    for category in ["added", "modified", "removed"] {
        assert!(diff["files"][category].as_array().unwrap().is_empty());
    }
    assert!(diff["files"]["unchanged"].as_u64().unwrap() > 0);
    let text = success(run(root.path(), &["release-diff", &id, &id]));
    assert!(text.contains("No file changes."));
    assert!(text.contains("no live status checked"));
    assert_eq!(snapshot(root.path()), state);
    assert!(!root.path().join("_site").exists());
    assert!(!root.path().join(".berlin/receipts").exists());
}

#[test]
fn reports_metadata_changes_even_when_files_are_identical() {
    let root = fixture();
    let before = seal(root.path());
    let script = PIPELINE
        .replace("owner/site", "owner/other")
        .replace("https://example.com", "https://example.com/notebook")
        .replace("pipeline site", "pipeline production");
    fs::write(root.path().join("berlin.pipeline.rhai"), script).unwrap();
    let after = success(run(
        root.path(),
        &[
            "release",
            "--pipeline",
            "production",
            "--from-directory",
            "prepared",
        ],
    ))
    .trim()
    .to_owned();
    assert_ne!(before, after);
    fs::write(root.path().join("berlin.pipeline.rhai"), "invalid Rhai {").unwrap();

    let diff = compare(root.path(), &before, &after);
    assert_eq!(diff["metadata_changed"], true);
    for category in ["added", "modified", "removed"] {
        assert!(diff["files"][category].as_array().unwrap().is_empty());
    }
    let old = &diff["before"]["metadata"];
    let new = &diff["after"]["metadata"];
    assert_eq!(old["destination"]["repository"], "owner/site");
    assert_eq!(new["destination"]["repository"], "owner/other");
    assert_eq!(old["url"], "https://example.com/");
    assert_eq!(new["url"], "https://example.com/notebook");
    assert_eq!(old["pipeline"], "site");
    assert_eq!(new["pipeline"], "production");
    assert!(old["website_node"].is_string());
    assert!(old["berlin_version"].is_string());
    let text = success(run(root.path(), &["release-diff", &before, &after]));
    assert!(text.contains("Destination:"));
    assert!(text.contains("Public URL:"));
    assert!(text.contains("Pipeline: \"site\" -> \"production\""));
    assert!(text.contains("No file changes."));
}

fn rejected(root: &Path, before: &str, after: &str, message: &str) {
    let state = snapshot(root);
    for json in [false, true] {
        let mut args = vec!["release-diff", before, after];
        if json {
            args.push("--json");
        }
        let output = run(root, &args);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty(), "do not emit partial comparisons");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(message), "{error}");
    }
    assert_eq!(snapshot(root), state);
}

#[test]
fn verifies_both_bundles_and_never_reports_partial_results() {
    for target_before in [true, false] {
        for corruption in ["edit", "extra", "missing", "manifest"] {
            let root = fixture();
            let before = seal(root.path());
            fs::write(root.path().join("prepared/index.html"), "other").unwrap();
            let after = seal(root.path());
            let id = if target_before { &before } else { &after };
            let directory = bundle(root.path(), id);
            match corruption {
                "edit" => fs::write(directory.join("site/index.html"), "alter").unwrap(),
                "extra" => fs::write(directory.join("site/extra.txt"), "extra").unwrap(),
                "missing" => fs::remove_file(directory.join("site/index.html")).unwrap(),
                "manifest" => fs::write(directory.join("manifest.json"), "{}").unwrap(),
                _ => unreachable!(),
            }
            rejected(
                root.path(),
                &before,
                &after,
                if target_before {
                    "BEFORE release"
                } else {
                    "AFTER release"
                },
            );
            // Equality of IDs must never short-circuit integrity verification.
            rejected(root.path(), id, id, "BEFORE release");
        }
    }
}

#[test]
fn rejects_unknown_ids_and_pipeline_overrides_without_creating_state() {
    let empty = tempfile::tempdir().unwrap();
    rejected(
        empty.path(),
        &"a".repeat(64),
        &"b".repeat(64),
        "BEFORE release",
    );

    let root = fixture();
    let id = seal(root.path());
    for invalid in ["../escape".to_owned(), "A".repeat(64), "a".repeat(64)] {
        rejected(root.path(), &invalid, &id, "BEFORE release");
        rejected(root.path(), &id, &invalid, "AFTER release");
    }
    let state = snapshot(root.path());
    for args in [
        vec!["--pipeline-file", "override.rhai", "release-diff", &id, &id],
        vec!["release-diff", &id, &id, "--pipeline-file", "override.rhai"],
    ] {
        assert!(!run(root.path(), &args).status.success());
    }
    assert_eq!(snapshot(root.path()), state);
}

#[cfg(unix)]
#[test]
fn rejects_symlinks_in_either_bundle() {
    let root = fixture();
    let before = seal(root.path());
    fs::write(root.path().join("prepared/index.html"), "other").unwrap();
    let after = seal(root.path());
    let index = bundle(root.path(), &before).join("site/index.html");
    fs::remove_file(&index).unwrap();
    std::os::unix::fs::symlink(root.path().join("prepared/index.html"), index).unwrap();
    rejected(root.path(), &before, &after, "BEFORE release");
    rejected(root.path(), &after, &before, "AFTER release");
}
