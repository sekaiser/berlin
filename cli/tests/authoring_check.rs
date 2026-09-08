use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const PIPELINE: &str = r#"
fn load_documents() { parse_markdown(load_markdown("content/*.md")) }
pipeline site {
    let website = assemble_website(load_documents(), parse_feed(load_data("missing-feed.csv")));
    output render_website(website, "_site", website_config(#{}));
    output compile_css(load_css("missing.css"), "_site/css/styles.css");
    output copy_assets(load_assets("missing-assets/**/*"), "_site/static");
}
"#;

fn project(pipeline: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("berlin.pipeline.rhai"), pipeline).unwrap();
    fs::create_dir(root.path().join("content")).unwrap();
    root
}

fn note(root: &Path, id: &str, fields: &str, body: &str) {
    fs::write(
        root.join("content").join(format!("{id}.md")),
        format!("---\nid: {id}\ntitle: {id}\n{fields}---\n{body}\n"),
    )
    .unwrap();
}

fn check(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bln"))
        .env("BERLIN_DIR", root)
        .env("BERLIN_EMACS", "must-not-run-emacs")
        .arg("check")
        .args(arguments)
        .output()
        .unwrap()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {}", String::from_utf8_lossy(&output.stdout)))
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_owned();
            if path.is_dir() {
                result.insert(relative, None);
                visit(root, &path, result);
            } else {
                result.insert(relative, Some(fs::read(path).unwrap()));
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[test]
fn reports_all_broken_links_in_json_and_text_without_touching_outputs() {
    let root = project(PIPELINE);
    note(
        root.path(),
        "a",
        "",
        "Read [one](id:missing-one).\n\nAlso [two](id:missing-two).",
    );
    fs::create_dir(root.path().join("_site")).unwrap();
    fs::write(root.path().join("_site/index.html"), "existing website").unwrap();
    fs::create_dir(root.path().join(".berlin")).unwrap();
    fs::write(root.path().join(".berlin/keep"), "existing receipt").unwrap();
    let before = snapshot(root.path());
    let output = check(root.path(), &["--json"]);
    assert_eq!(output.status.code(), Some(1));
    let report = json(&output);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["status"], "complete");
    assert_eq!(report["pipeline"], "site");
    let errors: Vec<_> = report["report"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|finding| finding["severity"] == "error")
        .collect();
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0]["code"], "unresolved_reference");
    assert_eq!(errors[0]["location"]["anchor"], "bln-ref-1");
    assert_eq!(errors[1]["location"]["excerpt"], "Also two.");
    assert!(
        errors[0]["location"]["source"]
            .as_str()
            .unwrap()
            .ends_with("/content/a.md")
    );
    let text = check(root.path(), &[]);
    assert_eq!(text.status.code(), Some(1));
    let stdout = String::from_utf8(text.stdout).unwrap();
    for expected in [
        "Publication errors",
        "Editorial observations (optional)",
        "missing-one",
        "missing-two",
        "bln-ref-2",
        "Also two.",
    ] {
        assert!(stdout.contains(expected), "{stdout}");
    }
    assert_eq!(snapshot(root.path()), before);
}

#[test]
fn observations_are_successful_and_drafts_are_not_inspected_for_connections() {
    let root = project(PIPELINE);
    note(root.path(), "alone", "", "A standalone note.");
    note(
        root.path(),
        "draft",
        "draft: true\n",
        "Secret [missing](id:private-target).",
    );
    let before = snapshot(root.path());
    let output = check(root.path(), &["--json"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let report = json(&output);
    assert_eq!(report["report"]["published_documents"], 1);
    assert_eq!(report["report"]["excluded_drafts"], 1);
    assert_eq!(report["report"]["findings"].as_array().unwrap().len(), 2);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private-target"));
    assert_eq!(snapshot(root.path()), before);
}

#[test]
fn checks_the_mapped_publication_not_intermediate_or_unrelated_documents() {
    let pipeline = PIPELINE.replace("pipeline site {", "fn publish(doc) { if doc.id == \"guide\" { doc.draft = false; } doc }\npipeline site {")
        .replace("assemble_website(load_documents(),", "assemble_website(map_documents(load_documents(), Fn(\"publish\")),");
    let root = project(&pipeline);
    note(
        root.path(),
        "guide",
        "kind: guide\ndraft: true\n",
        "Start with [the note](id:a).",
    );
    note(root.path(), "a", "", "An example.");
    fs::write(
        root.path().join("not-published.md"),
        "---\nid: excluded\n---\n[link](id:missing)",
    )
    .unwrap();
    let output = check(root.path(), &["--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = json(&output);
    assert_eq!(report["report"]["published_documents"], 2);
    assert_eq!(report["report"]["guides"], 1);
    assert_eq!(report["report"]["findings"], serde_json::json!([]));
}

#[test]
fn incomplete_checks_are_structured_failures_not_empty_successes() {
    let root = project(PIPELINE);
    let before = snapshot(root.path());
    let empty = check(root.path(), &["--json"]);
    assert_eq!(empty.status.code(), Some(1));
    assert_eq!(json(&empty)["status"], "incomplete");
    assert!(
        json(&empty)["message"]
            .as_str()
            .unwrap()
            .contains("matched no files")
    );
    assert_eq!(snapshot(root.path()), before);
    note(root.path(), "bad", "kind: typo\n", "Body");
    let malformed = check(root.path(), &["--json"]);
    assert_eq!(malformed.status.code(), Some(1));
    assert_eq!(json(&malformed)["status"], "incomplete");
    assert!(
        json(&malformed)["message"]
            .as_str()
            .unwrap()
            .contains("bad.md")
    );
    let unknown = check(root.path(), &["--json", "--pipeline", "missing"]);
    assert_eq!(unknown.status.code(), Some(1));
    assert_eq!(json(&unknown)["pipeline"], "missing");
    assert_eq!(json(&unknown)["status"], "incomplete");
}

#[test]
fn refuses_org_execution_and_ambiguous_publication_scopes() {
    let inline_org = PIPELINE.replace(
        "parse_markdown(load_markdown(\"content/*.md\"))",
        "parse_markdown(export_org(load_org(\"data/*.org\"), \"ox-hugo\", \".berlin/generated/org\", \"notes\"))",
    );
    let root = project(&inline_org);
    let before = snapshot(root.path());
    let output = check(root.path(), &["--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        json(&output)["message"]
            .as_str()
            .unwrap()
            .contains("check cannot execute")
    );
    assert_eq!(snapshot(root.path()), before);

    let two_sites = PIPELINE.replace("output render_website(website, \"_site\", website_config(#{}));", "output render_website(website, \"_site\", website_config(#{})); output render_website(website, \"other-site\", website_config(#{})).named(\"other\");");
    fs::write(root.path().join("berlin.pipeline.rhai"), two_sites).unwrap();
    let output = check(root.path(), &["--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        json(&output)["message"]
            .as_str()
            .unwrap()
            .contains("found 2")
    );
}
