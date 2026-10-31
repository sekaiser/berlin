use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("content")).unwrap();
    fs::create_dir_all(root.path().join("pages/publications")).unwrap();
    fs::write(
        root.path().join("berlin.pipeline.rhai"),
        concat!(include_str!("support/notebook.rhai"),r#"
fn rename(doc) { if doc.id == "target" { doc.title = "Mapped title"; } doc }
pipeline site {
    let source = map_documents(documents(load_markdown("content/*.md")), Fn("rename"));
    let selected = compose(catalog([source],publication_options(#{source_site_url:"https://example.com/notebook"})),Fn("notebook"));
    output render_html(selected, "_site", html_config(#{theme:"@notebook",url: "https://example.com/notebook"}));
}
"#),
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
    note(
        root.path(),
        "source",
        "Source",
        "",
        "See [the explanation](id:target#details).",
    );
    root
}

fn note(root: &Path, id: &str, title: &str, fields: &str, body: &str) {
    fs::write(root.join("content").join(format!("{id}.md")), format!("---\nid: {id}\ntitle: {title}\nauthor: [Ada]\ndescription: Summary\ndate: 2026-09-08\n{fields}---\n{body}")).unwrap();
}

fn build(root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bln"))
        .env("BERLIN_DIR", root)
        .env(
            "BERLIN_THEME_DIR",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../themes"),
        )
        .args(["build", "--pipeline", "site"])
        .output()
        .unwrap()
}

fn html(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join("_site").join(path))
        .unwrap()
        .replace("&#x2F;", "/")
}

#[test]
fn mapped_titles_do_not_move_explicit_routes_and_all_projections_use_the_same_url() {
    let root = project();
    note(
        root.path(),
        "target",
        "Original title",
        "slug: stable-name\nprevious_slugs: [old-name, oldest-name]\n",
        "## Details {#details}\nBody.",
    );
    let result = build(root.path());
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        html(root.path(), "notes/stable-name.html")
            .contains("https://example.com/notebook/notes/stable-name.html")
    );
    assert!(
        html(root.path(), "index.html")
            .contains("https://example.com/notebook/notes/stable-name.html")
    );
    assert!(
        html(root.path(), "notes/source.html")
            .contains("https://example.com/notebook/notes/stable-name.html#details")
    );
    assert!(html(root.path(), "notes/stable-name.html").contains("notes/source.html#bln-ref-1"));
    for old in ["old-name", "oldest-name"] {
        let redirect = html(root.path(), &format!("notes/{old}.html"));
        assert!(redirect.contains("name=\"berlin-redirect\" content=\"notes/stable-name.html\""));
        assert!(redirect.contains("http-equiv=\"refresh\" content=\"0;url=https://example.com/notebook/notes/stable-name.html\""));
        assert!(redirect.contains("noindex, follow"));
        assert!(!redirect.contains("#details"));
    }
    let index: serde_json::Value =
        serde_json::from_str(&html(root.path(), "search/index.json")).unwrap();
    assert_eq!(index["documents"].as_array().unwrap().len(), 2);
    assert!(
        index["documents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|doc| doc["title"] == "Mapped title" && doc["path"] == "notes/stable-name.html")
    );
    assert!(!index.to_string().contains("old-name"));
    fs::rename(
        root.path().join("content/target.md"),
        root.path().join("content/renamed-file.md"),
    )
    .unwrap();
    let result = build(root.path());
    assert!(result.status.success());
    assert!(root.path().join("_site/notes/stable-name.html").exists());
    assert!(!root.path().join("_site/notes/mapped-title.html").exists());
}

#[test]
fn invalid_or_colliding_routes_leave_the_previous_site_untouched() {
    let root = project();
    note(
        root.path(),
        "target",
        "Target",
        "slug: stable-name\n",
        "Body",
    );
    assert!(build(root.path()).status.success());
    let previous = fs::read(root.path().join("_site/index.html")).unwrap();
    for fields in [
        "slug: ../escape\n",
        "slug: Mixed-Case\n",
        "slug: bad--slug\n",
        "slug: ''\n",
        "slug: stable-name\nprevious_slugs: [source]\n",
        "slug: source\n",
        "slug: stable-name\nprevious_slugs: [stable-name]\n",
        "slug: stable-name\nprevious_slugs: [old, old]\n",
        "previous_slugs: [old]\n",
        "slug: stable-name\nprevious_slugs: ['https://other.test']\n",
    ] {
        note(root.path(), "target", "Target", fields, "Body");
        let result = build(root.path());
        assert!(!result.status.success(), "{fields}");
        assert_eq!(
            fs::read(root.path().join("_site/index.html")).unwrap(),
            previous
        );
    }
}

#[test]
fn draft_routes_and_redirects_are_not_published() {
    let root = project();
    note(
        root.path(),
        "target",
        "Target",
        "slug: stable-name\n",
        "Body",
    );
    note(
        root.path(),
        "draft",
        "Private",
        "draft: true\nslug: private-route\nprevious_slugs: [private-old]\n",
        "Private body",
    );
    assert!(build(root.path()).status.success());
    assert!(!root.path().join("_site/notes/private-route.html").exists());
    assert!(!root.path().join("_site/notes/private-old.html").exists());
    assert!(!html(root.path(), "search/index.json").contains("private"));
}

#[test]
fn missing_destination_url_is_refused_and_redirect_titles_are_escaped() {
    let root = project();
    let pipeline = root.path().join("berlin.pipeline.rhai");
    let source = fs::read_to_string(&pipeline)
        .unwrap()
        .replace("if doc.id == \"target\"", "if doc.id == \"unused\"");
    fs::write(&pipeline, &source).unwrap();
    note(
        root.path(),
        "target",
        "'<script>alert(1)</script>'",
        "slug: stable-name\nprevious_slugs: [old-name]\n",
        "Body",
    );
    let result = build(root.path());
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let redirect = html(root.path(), "notes/old-name.html");
    assert!(redirect.contains("https://example.com/notebook/notes/stable-name.html"));
    assert!(redirect.contains("&lt;script&gt;"));
    assert!(!redirect.contains("<script>alert"));
    let installed = fs::read(root.path().join("_site/notes/old-name.html")).unwrap();
    fs::write(
        &pipeline,
        source.replace(
            "theme:\"@notebook\",url: \"https://example.com/notebook\"",
            "theme:\"@notebook\"",
        ),
    )
    .unwrap();
    let rejected = build(root.path());
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("destination URL"));
    assert_eq!(
        fs::read(root.path().join("_site/notes/old-name.html")).unwrap(),
        installed
    );
}
