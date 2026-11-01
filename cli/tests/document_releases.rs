use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

#[path = "document_releases/announcements.rs"]
mod announcements;
#[path = "document_releases/lifecycle.rs"]
mod lifecycle;
#[path = "document_releases/manual_announcements.rs"]
mod manual_announcements;
#[path = "document_releases/production.rs"]
mod production;

const PIPELINE: &str = concat!(
    include_str!("support/notebook.rhai"),
    r#"
fn edit(document) { document }
pipeline site {
    let source = map_documents(documents(load_markdown("content/*.md")), Fn("edit"));
    let selected = compose(catalog([source],publication_options(#{source_site_url:"https://example.com"})),Fn("notebook"));
    output render_html(selected, "_site", html_config(#{theme:"@notebook",url: "https://example.com"}))
        .deploy_to(github_pages("owner/site"));
}
"#
);

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for path in ["content", "pages/publications", "static"] {
        fs::create_dir_all(root.path().join(path)).unwrap();
    }
    fs::write(root.path().join("berlin.pipeline.rhai"), PIPELINE).unwrap();
    fs::write(
        root.path().join("static/style.css"),
        "body { color: green; }",
    )
    .unwrap();
    for (path, text) in [
        (
            "publications/site_page.tera",
            include_str!("support/minimal-site.tera"),
        ),
        (
            "publications/document.tera",
            include_str!("support/minimal-document.tera"),
        ),
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
        .env(
            "BERLIN_THEME_DIR",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../themes"),
        )
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
        root.path().join("pages/publications/document.tera"),
        "<main class='alternative'>{{ render_slot(owner=view.id, name=\"body\") }}</main>",
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
    // The HTML does not display lastmod; the public provenance manifest now
    // records it. Release identity must still bind the metadata revision.
    assert_eq!(
        fs::read(
            root.path()
                .join(".berlin/releases")
                .join(&body)
                .join("site/notes/note.html")
        )
        .unwrap(),
        fs::read(
            root.path()
                .join(".berlin/releases")
                .join(&metadata)
                .join("site/notes/note.html")
        )
        .unwrap()
    );
    assert_ne!(body, metadata);
    assert_eq!(release(root.path()), metadata);
    let text = success(run(root.path(), &["release-diff", &body, &metadata]));
    assert!(text.contains("1 modified"));
    assert!(text.contains("\"Note\" [metadata]"));
    assert!(
        report["files"]["modified"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["after"]["path"] == "publication.json")
    );
    assert!(!root.path().join("_site").exists());
}

#[test]
fn source_moves_and_yaml_comments_preserve_document_revisions_and_offline_comparison() {
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
    let after = release(root.path());
    // Raw consumed bytes have changed, so their provenance changes the sealed
    // bundle. The authored document and editorial composition remain identical.
    assert_ne!(after, before);
    assert_eq!(
        manifest(root.path(), &before)["documents"],
        manifest(root.path(), &after)["documents"]
    );
    let public = |id: &str| -> Value {
        serde_json::from_slice(
            &fs::read(
                root.path()
                    .join(".berlin/releases")
                    .join(id)
                    .join("site/publication.json"),
            )
            .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(
        public(&before)["composition_fingerprint"],
        public(&after)["composition_fingerprint"]
    );
    fs::remove_file(root.path().join("content/renamed.md")).unwrap();
    fs::remove_file(root.path().join("berlin.pipeline.rhai")).unwrap();
    let report = compare(root.path(), &before, &after);
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
