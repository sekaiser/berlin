use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}
fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}
fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    copy(
        &repository().join("support/fixtures/composable-publications"),
        root.path(),
    );
    root
}
fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bln"))
        .env("BERLIN_DIR", root)
        .env("BERLIN_THEME_DIR", repository().join("themes"))
        .args(args)
        .output()
        .unwrap()
}
fn success(output: Output) {
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(base).unwrap().into(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    walk(root, root, &mut files);
    files
}
fn json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn p07_real_html_text_and_excerpt_pipelines_are_repeatable_and_private() {
    let project = project();
    let root = project.path();
    let authored = snapshot(&root.join("content"));
    success(run(root, &["plan", "--pipeline", "portfolio", "--json"]));
    assert!(!root.join("_portfolio").exists());
    success(run(root, &["build", "--pipeline", "portfolio"]));
    let first = snapshot(&root.join("_portfolio"));
    success(run(root, &["build", "--pipeline", "portfolio"]));
    assert_eq!(first, snapshot(&root.join("_portfolio")));
    success(run(root, &["build", "--pipeline", "text"]));
    let html = json(&root.join("_portfolio/publication.json"));
    let text = json(&root.join("_text/manifest.json"));
    assert_eq!(
        html["composition_fingerprint"],
        text["publication"]["composition_fingerprint"]
    );
    assert_eq!(html["sources"].as_array().unwrap().len(), 8);
    assert!(html["render_inputs"].get("pipelines/0.rhai").is_some());
    assert!(
        html["render_inputs"]
            .get("styles/entries/portfolio.css")
            .is_some()
    );
    assert!(html["render_inputs"].get("composition/catalog").is_some());
    let index = fs::read_to_string(root.join("_portfolio/index.html")).unwrap();
    assert!(index.contains("Storage handoffs"));
    for output in ["_portfolio", "_text"] {
        for (path, bytes) in snapshot(&root.join(output)) {
            if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("html" | "json" | "txt")
            ) {
                let text = String::from_utf8_lossy(&bytes);
                for private in [
                    "file:///",
                    "DRAFT_BODY_SENTINEL",
                    "UNSELECTED_BODY_SENTINEL",
                    root.to_str().unwrap(),
                ] {
                    assert!(
                        !text.contains(private),
                        "{} leaked {private}",
                        path.display()
                    );
                }
            }
        }
    }
    success(run(root, &["build", "--pipeline", "excerpts"]));
    success(run(root, &["build", "--pipeline", "excerpt_text"]));
    let excerpts = fs::read_to_string(root.join("_excerpts/index.html")).unwrap();
    assert_eq!(excerpts.matches("class=\"publication-excerpt\"").count(), 2);
    assert!(excerpts.contains("footnote-definition"));
    assert_eq!(authored, snapshot(&root.join("content")));
}

#[test]
fn p07_template_failure_keeps_installed_output_and_alternate_theme_keeps_composition() {
    let project = project();
    let root = project.path();
    success(run(root, &["build", "--pipeline", "portfolio"]));
    let first = snapshot(&root.join("_portfolio"));
    let manifest = json(&root.join("_portfolio/publication.json"));
    let template = root.join("pages/components/berlin/introduction.v1.tera");
    fs::create_dir_all(template.parent().unwrap()).unwrap();
    fs::write(&template, "{{props.missing}}").unwrap();
    let failed = run(root, &["build", "--pipeline", "portfolio"]);
    assert!(!failed.status.success());
    assert_eq!(first, snapshot(&root.join("_portfolio")));
    assert_eq!(
        json(&root.join(".berlin/receipts/portfolio.json"))["status"],
        "failed"
    );
    fs::remove_file(template).unwrap();
    copy(&root.join("alternate"), root);
    success(run(root, &["build", "--pipeline", "portfolio"]));
    let second = json(&root.join("_portfolio/publication.json"));
    assert_eq!(
        manifest["composition_fingerprint"],
        second["composition_fingerprint"]
    );
    assert_ne!(
        manifest["render_input_fingerprint"],
        second["render_input_fingerprint"]
    );
    assert!(
        fs::read_to_string(root.join("_portfolio/index.html"))
            .unwrap()
            .contains("alternate-work")
    );
}

#[test]
fn p07_dry_run_is_write_free_and_rejects_broken_presentations() {
    let project = project();
    let root = project.path();
    let before = snapshot(root);
    success(run(
        root,
        &["build", "--pipeline", "portfolio", "--dry-run"],
    ));
    assert_eq!(before, snapshot(root));
    let template = root.join("pages/components/berlin/portfolio.v1.tera");
    fs::create_dir_all(template.parent().unwrap()).unwrap();
    fs::write(&template, "<h1>{{publication.title}}</h1>").unwrap();
    let before = snapshot(root);
    assert!(
        !run(root, &["build", "--pipeline", "portfolio", "--dry-run"])
            .status
            .success()
    );
    assert_eq!(before, snapshot(root));
}
