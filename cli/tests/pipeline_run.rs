use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn build(root: &Path, dry_run: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bln"));
    command
        .current_dir(root)
        .env("BERLIN_DIR", root)
        .args(["build", "--pipeline", "test"]);
    if dry_run {
        command.arg("--dry-run");
    }
    command.output().unwrap()
}

fn fixture(pipeline: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("berlin.pipeline.rhai"), pipeline).unwrap();
    root
}

fn receipt(root: &Path) -> serde_json::Value {
    assert!(
        !root.join("_berlin").exists(),
        "build must use .berlin for state"
    );
    serde_json::from_slice(&fs::read(root.join(".berlin/receipts/test.json")).unwrap()).unwrap()
}

const COPY: &str = r#"
pipeline test {
    output copy_assets(load_assets("assets/*"), "public");
}
"#;

#[test]
fn successful_run_commits_outputs_and_records_success() {
    let root = fixture(COPY);
    // Legacy website configuration must not affect unrelated pipelines.
    fs::write(root.path().join("berlin.toml"), "invalid = [").unwrap();
    fs::create_dir(root.path().join("assets")).unwrap();
    fs::write(root.path().join("assets/example.txt"), "example").unwrap();
    let result = build(root.path(), false);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read_to_string(root.path().join("public/example.txt")).unwrap(),
        "example"
    );
    assert_eq!(receipt(root.path())["status"], "succeeded");
    let receipt = receipt(root.path());
    let inputs = receipt["inputs"].as_array().unwrap();
    assert!(
        inputs
            .iter()
            .any(|input| input["path"] == "berlin.pipeline.rhai")
    );
    assert!(!inputs.iter().any(|input| input["path"] == "berlin.toml"));
}

#[test]
fn conflicting_publication_outputs_fail_before_writing() {
    let root = fixture(
        r#"
fn draft(c) {
    publication("draft",#{title:"Article"},[view_component("berlin.excerpt@1",#{source:document_excerpt(c,"local:article")},#{})])
}
pipeline test {
    output render_text(compose(catalog([documents(load_markdown("article.md"))],publication_options(#{source_site_url:"https://example.test"})),Fn("draft")),"public",text_config(#{profile:"linkedin_draft"}));
    output compile_css(load_css("missing.css"), "public/styles.css");
}
"#,
    );
    fs::write(
        root.path().join("article.md"),
        format!(
            "---\nid: article\ntitle: Article\n---\n{}\n",
            "x".repeat(3100)
        ),
    )
    .unwrap();
    fs::create_dir(root.path().join("public")).unwrap();
    fs::write(root.path().join("public/previous.txt"), "previous").unwrap();
    assert!(!build(root.path(), false).status.success());
    assert_eq!(
        fs::read_to_string(root.path().join("public/previous.txt")).unwrap(),
        "previous"
    );
    assert!(!root.path().join("public/manifest.json").exists());
    let receipt = receipt(root.path());
    assert_eq!(receipt["status"], "failed");
    assert_eq!(receipt["outputs"], serde_json::json!([]));
    assert_eq!(receipt["diagnostics"], serde_json::json!([]));
    assert!(
        receipt["error"]
            .as_str()
            .unwrap()
            .contains("OutputConflict"),
        "{receipt}"
    );
    assert!(!fs::read_dir(root.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".berlin-stage-")
    }));
}

#[test]
fn setup_failure_records_a_receipt() {
    let root = fixture("this is not a valid pipeline");
    assert!(!build(root.path(), false).status.success());
    let receipt = receipt(root.path());
    assert_eq!(receipt["status"], "failed");
    assert!(receipt["error"].is_string());
}

#[test]
fn transaction_preparation_failure_records_a_setup_receipt() {
    let root = fixture(
        r#"
pipeline test {
    let assets = load_assets("assets/*");
    output copy_assets(assets, "one").named("one");
    output copy_assets(assets, "two").named("two");
}
"#,
    );
    assert!(!build(root.path(), false).status.success());
    let receipt = receipt(root.path());
    assert_eq!(receipt["status"], "failed");
    assert!(
        receipt["error"]
            .as_str()
            .unwrap()
            .contains("one atomic root")
    );
    assert!(!root.path().join("one").exists());
    assert!(!root.path().join("two").exists());
}

#[test]
fn commit_failure_is_reported_as_a_failed_run() {
    let root = fixture(
        r#"
pipeline test {
    output copy_assets(load_assets("assets/*"), "blocked/public");
}
"#,
    );
    fs::write(root.path().join("blocked"), "existing file").unwrap();
    assert!(!build(root.path(), false).status.success());
    assert_eq!(
        fs::read_to_string(root.path().join("blocked")).unwrap(),
        "existing file"
    );
    assert_eq!(receipt(root.path())["status"], "failed");
}

#[test]
fn dry_runs_create_neither_outputs_nor_receipts_even_on_failure() {
    for pipeline in [COPY, "this is not a valid pipeline"] {
        let root = fixture(pipeline);
        let result = build(root.path(), true);
        assert_eq!(result.status.success(), pipeline == COPY);
        assert!(!root.path().join("public").exists());
        assert!(!root.path().join(".berlin").exists());
        assert!(!root.path().join(".berlin.lock").exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
}

#[test]
fn receipt_failure_does_not_replace_the_run_outcome() {
    for (pipeline, succeeds) in [(COPY, true), ("invalid pipeline", false)] {
        let root = fixture(pipeline);
        fs::create_dir(root.path().join("assets")).unwrap();
        fs::write(root.path().join("assets/example.txt"), "example").unwrap();
        // A file prevents receipt directory creation.
        fs::write(root.path().join(".berlin"), "blocked").unwrap();
        let result = build(root.path(), false);
        assert_eq!(
            result.status.success(),
            succeeds,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if succeeds {
            assert!(root.path().join("public/example.txt").exists());
        }
    }
}

#[test]
fn themed_build_records_effective_inputs_and_preserves_output_on_failure() {
    let root = fixture(
        r#"
fn page(c) {publication("page",#{title:"Opening"},[view_component("berlin.introduction@1",#{text:"Personal opening"},#{})])}
pipeline test {
    output render_html(compose(catalog([documents(load_markdown("*.md"))]),Fn("page")),"public",html_config(#{theme:"theme",url:"https://example.test"}));
}
"#,
    );
    let write = |relative: &str, text: &str| {
        let path = root.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    };
    fs::create_dir_all(root.path().join("theme/defaults")).unwrap();
    write(
        "theme/shared/pages/publications/page.tera",
        r#"<html><head>{% for s in site.stylesheets %}<link rel="stylesheet" href="{{s}}">{% endfor %}</head><body>{{render_slot(owner=view.id,name="body")}}</body></html>"#,
    );
    write(
        "theme/shared/pages/components/berlin/portfolio.v1.tera",
        r#"<main><h1>{{publication.title}}</h1>{{render_slot(owner=view.id,name="body")}}</main>"#,
    );
    write(
        "article.md",
        "---\nid: article\ntitle: Article\n---\nSynthetic body.",
    );
    write(
        "pages/components/berlin/introduction.v1.tera",
        "<p>{{ props.text }}</p>",
    );
    write(
        "theme/shared/pages/publications/unused.tera",
        "Unused template",
    );
    write(
        "theme/shared/styles/publications/portfolio.css",
        "@import '../tokens.css';",
    );
    write("theme/shared/styles/tokens.css", ":root { --ink: green; }");
    write("styles/tokens.css", ":root { --ink: purple; }");
    let result = build(root.path(), false);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        fs::read_to_string(root.path().join("public/index.html"))
            .unwrap()
            .contains("Personal opening")
    );
    assert!(!root.path().join("assets/css/notebook.css").exists());
    assert!(!root.path().join("pages/feed.tera").exists());
    let report = receipt(root.path());
    let inputs = report["inputs"].as_array().unwrap();
    for path in [
        "pages/components/berlin/introduction.v1.tera",
        "styles/tokens.css",
        "theme/shared/pages/publications/unused.tera",
        "theme/shared/styles/publications/portfolio.css",
    ] {
        assert!(
            inputs.iter().any(|input| input["path"] == path),
            "missing {path}"
        );
    }
    assert!(
        !inputs
            .iter()
            .any(|input| input["path"] == "theme/shared/styles/tokens.css")
    );
    write(
        "theme/shared/styles/publications/portfolio.css",
        "@import 'missing.css';",
    );
    assert!(!build(root.path(), false).status.success());
    assert_eq!(receipt(root.path())["status"], "failed");
    assert!(
        fs::read_to_string(root.path().join("public/index.html"))
            .unwrap()
            .contains("Personal opening")
    );
    assert!(
        fs::read_to_string(root.path().join("public/assets/css/portfolio.css"))
            .unwrap()
            .contains("--ink:purple")
    );
}

#[test]
fn text_over_limit_keeps_full_output_and_receipt_diagnostics() {
    let root = fixture(
        r#"
fn draft(c) {publication("draft",#{title:"Article"},[view_component("berlin.excerpt@1",#{source:document_excerpt(c,"local:article")},#{})])}
pipeline test {output render_text(compose(catalog([documents(load_markdown("article.md"))],publication_options(#{source_site_url:"https://example.test"})),Fn("draft")),"public",text_config(#{profile:"linkedin_draft"}));}
"#,
    );
    let body = "x".repeat(3100);
    fs::write(
        root.path().join("article.md"),
        format!("---\nid: article\ntitle: Article\n---\n{body}\n"),
    )
    .unwrap();
    let result = build(root.path(), false);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        fs::read_to_string(root.path().join("public/publication.txt"))
            .unwrap()
            .contains(&body)
    );
    assert_eq!(
        receipt(root.path())["diagnostics"][0]["code"],
        "character_limit_exceeded"
    );
}
