use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

#[path = "document_releases/announcements.rs"]
mod announcements;

const PIPELINE: &str = r#"
fn edit(document) { document }
pipeline site {
    let documents = map_documents(parse_markdown(load_markdown("content/*.md")), Fn("edit"));
    let website = assemble_website(documents, parse_feed(load_data("feed.csv")));
    output render_website(website, "_site", website_config(#{url: "https://example.com"}))
        .deploy_to(github_pages("owner/site"));
    output copy_assets(load_assets("static/**/*"), "_site/static");
}
"#;

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for path in ["content", "pages/notes", "pages/tags", "static"] {
        fs::create_dir_all(root.path().join(path)).unwrap();
    }
    fs::write(root.path().join("berlin.pipeline.rhai"), PIPELINE).unwrap();
    fs::write(root.path().join("feed.csv"), "Title,URL,Date,Tags\n").unwrap();
    fs::write(
        root.path().join("static/style.css"),
        "body { color: green; }",
    )
    .unwrap();
    for (path, text) in [
        (
            "index.tera",
            "Index: {% for a in articles %}{{a.title}}{% endfor %}",
        ),
        ("notes.tera", "Notes"),
        ("feed.tera", "Reading"),
        ("about.tera", "About"),
        ("tags/base.tera", "Tag"),
        ("notes/[slug].tera", "{{render()}}"),
    ] {
        fs::write(root.path().join("pages").join(path), text).unwrap();
    }
    note(root.path(), "note", "Note", "", "Initial prose.");
    root
}

fn note(root: &Path, id: &str, title: &str, fields: &str, body: &str) {
    fs::write(root.join("content").join(format!("{id}.md")), format!("---\nid: {id}\ntitle: {title}\nauthor: [Author]\ndescription: Summary\ndate: 2026-09-08\n{fields}---\n{body}\n")).unwrap();
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bln"))
        .env("BERLIN_DIR", root)
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

fn release(root: &Path) -> String {
    success(run(root, &["release"])).trim().to_owned()
}

fn compare(root: &Path, before: &str, after: &str) -> Value {
    serde_json::from_str(&success(run(
        root,
        &["release-diff", before, after, "--json"],
    )))
    .unwrap()
}

fn manifest(root: &Path, id: &str) -> Value {
    serde_json::from_slice(
        &fs::read(root.join(".berlin/releases").join(id).join("manifest.json")).unwrap(),
    )
    .unwrap()
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn changes(report: &Value) -> &Value {
    assert_eq!(report["documents"]["status"], "compared");
    &report["documents"]["changes"]
}

#[test]
fn distinguishes_content_metadata_and_presentation_changes_in_real_releases() {
    let root = fixture();
    let first = release(root.path());
    let inventory = manifest(root.path(), &first);
    assert_eq!(inventory["schema_version"], 2);
    assert_eq!(inventory["documents"]["fingerprint_version"], 1);
    let entry = &inventory["documents"]["entries"][0];
    assert_eq!(entry["id"], "note");
    assert_eq!(entry["path"], "notes/note.html");
    for secret in ["file:///", "Initial prose.", "provenance", "source_hash"] {
        assert!(!inventory.to_string().contains(secret));
    }

    fs::write(
        root.path().join("pages/notes/[slug].tera"),
        "<main>{{render()}}</main>",
    )
    .unwrap();
    fs::write(
        root.path().join("static/style.css"),
        "body { color: brown; }",
    )
    .unwrap();
    let themed = release(root.path());
    let report = compare(root.path(), &first, &themed);
    assert!(!report["files"]["modified"].as_array().unwrap().is_empty());
    assert_eq!(changes(&report)["unchanged"], 1);
    assert!(changes(&report)["modified"].as_array().unwrap().is_empty());

    note(
        root.path(),
        "note",
        "Note",
        "",
        "Initial prose.\n\n```rust\nlet n = 42;\n```",
    );
    let body = release(root.path());
    let report = compare(root.path(), &themed, &body);
    let modified = &changes(&report)["modified"][0];
    assert_eq!(modified["content_changed"], true);
    assert_eq!(modified["metadata_changed"], false);
    assert_eq!(modified["route_changed"], false);

    note(
        root.path(),
        "note",
        "Note",
        "lastmod: 2026-09-09\n",
        "Initial prose.\n\n```rust\nlet n = 42;\n```",
    );
    let metadata = release(root.path());
    let report = compare(root.path(), &body, &metadata);
    let modified = &changes(&report)["modified"][0];
    assert_eq!(modified["content_changed"], false);
    assert_eq!(modified["metadata_changed"], true);
    assert_eq!(modified["route_changed"], false);
    // The fixture templates do not display lastmod: an invisible metadata edit
    // must still be bound by a new release ID, not lost through equal file hashes.
    assert_eq!(
        manifest(root.path(), &body)["files"],
        manifest(root.path(), &metadata)["files"]
    );
    assert_ne!(body, metadata);
    assert_eq!(release(root.path()), metadata);
    let text = success(run(root.path(), &["release-diff", &body, &metadata]));
    assert!(text.contains("1 modified"));
    assert!(text.contains("\"Note\" [metadata]"));
    assert!(text.contains("No file changes."));
    assert!(!root.path().join("_site").exists());
}

#[test]
fn ignores_source_moves_and_yaml_comments_and_compares_offline_after_sources_disappear() {
    let root = fixture();
    let before = release(root.path());
    let source = fs::read_to_string(root.path().join("content/note.md")).unwrap();
    fs::write(
        root.path().join("content/note.md"),
        source.replace("id: note", "# A private authoring comment\nid: note"),
    )
    .unwrap();
    fs::rename(
        root.path().join("content/note.md"),
        root.path().join("content/renamed.md"),
    )
    .unwrap();
    assert_eq!(release(root.path()), before);
    fs::remove_file(root.path().join("content/renamed.md")).unwrap();
    fs::remove_file(root.path().join("berlin.pipeline.rhai")).unwrap();
    let report = compare(root.path(), &before, &before);
    assert_eq!(changes(&report)["unchanged"], 1);
}

#[test]
fn identifies_title_and_route_changes_by_stable_content_id() {
    let root = fixture();
    note(root.path(), "note", "Note", "slug: stable\n", "Prose.");
    let before = release(root.path());
    note(root.path(), "note", "New title", "slug: stable\n", "Prose.");
    let titled = release(root.path());
    let report = compare(root.path(), &before, &titled);
    let modified = &changes(&report)["modified"][0];
    assert_eq!(modified["content_changed"], false);
    assert_eq!(modified["metadata_changed"], true);
    assert_eq!(modified["route_changed"], false);

    note(
        root.path(),
        "note",
        "New title",
        "slug: moved\nprevious_slugs: [stable]\n",
        "Prose.",
    );
    let moved = release(root.path());
    let report = compare(root.path(), &titled, &moved);
    let documents = changes(&report);
    assert!(documents["added"].as_array().unwrap().is_empty());
    assert!(documents["removed"].as_array().unwrap().is_empty());
    let modified = &documents["modified"][0];
    assert_eq!(modified["route_changed"], true);
    assert_eq!(modified["content_changed"], false);
    assert_eq!(modified["before"]["path"], "notes/stable.html");
    assert_eq!(modified["after"]["path"], "notes/moved.html");
    assert_eq!(modified["after"]["id"], "note");
    let reverse = compare(root.path(), &moved, &titled);
    assert_eq!(
        changes(&reverse)["modified"][0]["before"],
        modified["after"]
    );
    assert_eq!(
        changes(&reverse)["modified"][0]["after"],
        modified["before"]
    );
    let source = fs::read_to_string(root.path().join("content/note.md")).unwrap();
    fs::write(
        root.path().join("content/note.md"),
        source.replace("id: note", "id: replacement"),
    )
    .unwrap();
    let replaced = release(root.path());
    let report = compare(root.path(), &moved, &replaced);
    assert_eq!(changes(&report)["added"][0]["id"], "replacement");
    assert_eq!(changes(&report)["removed"][0]["id"], "note");
    assert!(changes(&report)["modified"].as_array().unwrap().is_empty());
}

#[test]
fn added_and_removed_documents_follow_mapped_publication_scope_not_loaded_sources() {
    let root = fixture();
    note(
        root.path(),
        "private-note",
        "Private title",
        "draft: true\n",
        "Private prose.",
    );
    let before = release(root.path());
    note(root.path(), "a-note", "Added", "", "Added prose.");
    fs::write(root.path().join("berlin.pipeline.rhai"), PIPELINE.replace(
        "fn edit(document) { document }",
        "fn edit(document) { if document.id == \"note\" { document.draft = true; } if document.id == \"a-note\" { document.title = \"Mapped title\"; } document }",
    )).unwrap();
    let after = release(root.path());
    let report = compare(root.path(), &before, &after);
    let documents = changes(&report);
    assert_eq!(documents["added"].as_array().unwrap().len(), 1);
    assert_eq!(documents["added"][0]["id"], "a-note");
    assert_eq!(documents["added"][0]["title"], "Mapped title");
    assert_eq!(documents["removed"][0]["id"], "note");
    assert!(documents["modified"].as_array().unwrap().is_empty());
    assert!(!report.to_string().contains("private-note"));
    assert!(
        !manifest(root.path(), &after)
            .to_string()
            .contains("Private")
    );
    let reverse = compare(root.path(), &after, &before);
    assert_eq!(changes(&reverse)["added"], documents["removed"]);
    assert_eq!(changes(&reverse)["removed"], documents["added"]);

    fs::write(
        root.path().join("berlin.pipeline.rhai"),
        PIPELINE.replace(
            "fn edit(document) { document }",
            "fn edit(document) { document.draft = true; document }",
        ),
    )
    .unwrap();
    let empty = release(root.path());
    let report = compare(root.path(), &after, &empty);
    assert_eq!(changes(&report)["removed"].as_array().unwrap().len(), 1);
    assert_eq!(changes(&report)["unchanged"], 0);
    assert!(
        manifest(root.path(), &empty)["documents"]["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn imported_releases_do_not_claim_the_current_sources_match_their_files() {
    let root = fixture();
    let native = release(root.path());
    note(
        root.path(),
        "note",
        "Unrelated current source",
        "",
        "Do not associate with imported bytes.",
    );
    let imported = success(run(
        root.path(),
        &[
            "release",
            "--from-directory",
            &format!(".berlin/releases/{native}/site"),
        ],
    ))
    .trim()
    .to_owned();
    assert_eq!(manifest(root.path(), &imported)["schema_version"], 1);
    assert!(manifest(root.path(), &imported).get("documents").is_none());
    let report = compare(root.path(), &native, &imported);
    assert_eq!(report["documents"]["status"], "unavailable");
    assert_eq!(report["documents"]["before_available"], true);
    assert_eq!(report["documents"]["after_available"], false);
    assert!(report["documents"].get("changes").is_none());
    for category in ["added", "modified", "removed"] {
        assert!(report["files"][category].as_array().unwrap().is_empty());
    }
    assert_eq!(
        compare(root.path(), &imported, &native)["documents"]["after_available"],
        true
    );
    let text = success(run(root.path(), &["release-diff", &native, &imported]));
    assert!(text.contains("Documents: comparison unavailable"));
}

#[test]
fn preserves_preexisting_schema_one_release_ids_and_bytes() {
    let root = fixture();
    let native = release(root.path());
    let empty_hash = digest(b"");
    let index_hash = digest(b"Index");
    // This is the original schema-1 serialization, independent of the new types.
    let legacy = format!(
        r#"{{"schema_version":1,"berlin_version":"legacy","pipeline":"site","website_node":"website","url":"https://example.com/","destination":{{"type":"github_pages","repository":"owner/site"}},"files":[{{"path":".nojekyll","bytes":0,"sha256":"{empty_hash}"}},{{"path":"index.html","bytes":5,"sha256":"{index_hash}"}}]}}"#
    );
    let id = digest(legacy.as_bytes());
    let directory = root.path().join(".berlin/releases").join(&id);
    fs::create_dir_all(directory.join("site")).unwrap();
    fs::write(directory.join("site/.nojekyll"), "").unwrap();
    fs::write(directory.join("site/index.html"), "Index").unwrap();
    fs::write(directory.join("manifest.json"), &legacy).unwrap();
    success(run(root.path(), &["release-plan", &id]));
    let report = compare(root.path(), &id, &native);
    assert_eq!(report["documents"]["status"], "unavailable");
    assert_eq!(report["before"]["id"], id);
    assert_eq!(
        fs::read_to_string(directory.join("manifest.json")).unwrap(),
        legacy
    );
}

#[test]
fn document_inventory_is_bound_to_release_identity() {
    let root = fixture();
    let id = release(root.path());
    let mut manifest = manifest(root.path(), &id);
    manifest["documents"]["entries"][0]["title"] = "Tampered".into();
    fs::write(
        root.path()
            .join(".berlin/releases")
            .join(&id)
            .join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let output = run(root.path(), &["release-diff", &id, &id, "--json"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("release manifest does not match its ID")
    );
}
