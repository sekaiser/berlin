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
fn p09_check_is_write_free_and_checks_the_actual_presentation() {
    let project = project();
    let root = project.path();
    let before = snapshot(root);
    for pipeline in ["portfolio", "text", "excerpts", "excerpt_text"] {
        let result = run(root, &["check", "--pipeline", pipeline, "--json"]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(report["report"]["kind"], "publication");
        assert_eq!(report["status"], "complete");
    }
    assert_eq!(snapshot(root), before);
    let path = root.join("pages/components/berlin");
    fs::create_dir_all(&path).unwrap();
    fs::write(
        path.join("featured_work.v1.tera"),
        "{{ render_slot(owner=view.id,name=\"nonexistent\") }}",
    )
    .unwrap();
    let before = snapshot(root);
    assert!(
        !run(root, &["check", "--pipeline", "portfolio"])
            .status
            .success()
    );
    assert_eq!(snapshot(root), before);
}

#[test]
fn f04_checks_and_dry_runs_do_not_require_a_temporary_theme_workspace() {
    let project = project();
    let root = project.path();
    let before = snapshot(root);
    for args in [
        ["check", "--pipeline", "portfolio", "--json"],
        ["build", "--pipeline", "portfolio", "--dry-run"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_bln"))
            .env("BERLIN_DIR", root)
            .env("BERLIN_THEME_DIR", repository().join("themes"))
            .env("TMPDIR", root.join("nonexistent-temporary-directory"))
            .args(args)
            .output()
            .unwrap();
        success(result);
    }
    assert_eq!(snapshot(root), before);
}

#[test]
fn f04_missing_generated_inputs_are_not_replaced_by_a_dry_run_placeholder() {
    let project = project();
    let root = project.path();
    let script = root.join("berlin.pipeline.rhai");
    fs::write(
        &script,
        fs::read_to_string(&script)
            .unwrap()
            .replace("content/*.md", "missing-generated/*.md"),
    )
    .unwrap();
    let before = snapshot(root);
    for args in [
        ["check", "--pipeline", "portfolio", "--json"],
        ["build", "--pipeline", "portfolio", "--dry-run"],
    ] {
        let result = run(root, &args);
        assert!(!result.status.success());
        let message = format!(
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(message.contains("matched no files"), "{message}");
    }
    assert_eq!(snapshot(root), before);
}

#[test]
fn composition_cannot_load_an_uncaptured_filesystem_module() {
    let project = project();
    let root = project.path();
    let module = root.join("helper.rhai");
    fs::write(&module, "fn title() { \"UNCAPTURED_MODULE_TITLE\" }").unwrap();
    let module_path = serde_json::to_string(module.with_extension("").to_str().unwrap()).unwrap();
    let path = root.join("berlin.pipeline.rhai");
    let script = fs::read_to_string(&path)
        .unwrap()
        .replace(
            "fn system_design(catalog) {",
            &format!("fn system_design(catalog) {{ import {module_path} as helper;"),
        )
        .replace(
            "title: \"Selected Work in System Design\"",
            "title: helper::title()",
        );
    fs::write(&path, script).unwrap();
    let result = run(root, &["build", "--pipeline", "portfolio"]);
    assert!(
        !result.status.success(),
        "composer read an uncaptured module and built {}",
        fs::read_to_string(root.join("_portfolio/index.html")).unwrap_or_default()
    );
    assert!(!root.join("_portfolio").exists());
}

#[test]
fn f10_f11_relocation_configuration_and_source_changes_have_distinct_effects() {
    let first = project();
    let second = project();
    success(run(first.path(), &["build", "--pipeline", "portfolio"]));
    success(run(second.path(), &["build", "--pipeline", "portfolio"]));
    assert_eq!(
        snapshot(&first.path().join("_portfolio")),
        snapshot(&second.path().join("_portfolio")),
        "project filesystem location changed public output"
    );
    let baseline = json(&first.path().join("_portfolio/publication.json"));
    let script = first.path().join("berlin.pipeline.rhai");
    fs::write(
        &script,
        fs::read_to_string(&script)
            .unwrap()
            .replace("Example Author", "Another Example Author"),
    )
    .unwrap();
    success(run(first.path(), &["build", "--pipeline", "portfolio"]));
    let configured = json(&first.path().join("_portfolio/publication.json"));
    assert_eq!(
        baseline["composition_fingerprint"],
        configured["composition_fingerprint"]
    );
    assert_ne!(
        baseline["render_input_fingerprint"],
        configured["render_input_fingerprint"]
    );
    let source = first.path().join("content/storage.md");
    fs::write(
        &source,
        fs::read_to_string(&source)
            .unwrap()
            .replace("completed prefix", "completed sequence"),
    )
    .unwrap();
    success(run(first.path(), &["build", "--pipeline", "portfolio"]));
    let changed = json(&first.path().join("_portfolio/publication.json"));
    assert_ne!(
        configured["composition_fingerprint"],
        changed["composition_fingerprint"]
    );
    assert_ne!(
        configured["render_input_fingerprint"],
        changed["render_input_fingerprint"]
    );
}

#[test]
fn p09_preview_selects_the_output_and_watches_overrides_without_output_loops() {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::time::{Duration, Instant};
    struct Server(std::process::Child);
    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let project = project();
    let root = project.path();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut server = Server(
        Command::new(env!("CARGO_BIN_EXE_bln"))
            .env("BERLIN_DIR", root)
            .env("BERLIN_THEME_DIR", repository().join("themes"))
            .args([
                "serve",
                "--pipeline",
                "portfolio",
                "--watch",
                "--port",
                &port.to_string(),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let get = |path: &str| -> String {
        let Ok(mut connection) = TcpStream::connect(("127.0.0.1", port)) else {
            return String::new();
        };
        connection
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        if write!(connection, "GET {path} HTTP/1.0\r\nHost: localhost\r\n\r\n").is_err() {
            return String::new();
        }
        let mut bytes = String::new();
        let _ = connection.read_to_string(&mut bytes);
        bytes
    };
    let wait = |expected: &str| {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if get("/").contains(expected) {
                break;
            }
            assert!(Instant::now() < deadline, "preview did not show {expected}");
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    wait("Selected Work in System Design");
    assert!(root.join("_portfolio/index.html").is_file());
    assert!(!root.join("_site").exists());
    // The HTML becomes visible just before the initial build updates revision.
    std::thread::sleep(Duration::from_secs(3));
    let revision = || {
        get("/__berlin/revision")
            .split("\r\n\r\n")
            .last()
            .unwrap_or_default()
            .to_owned()
    };
    let before = revision();
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(
        before,
        revision(),
        "output and receipt writes caused a watch loop"
    );
    let override_dir = root.join("pages/components/berlin");
    fs::create_dir_all(&override_dir).unwrap();
    let original =
        repository().join("themes/notebook/shared/pages/components/berlin/featured_work.v1.tera");
    let path = override_dir.join("featured_work.v1.tera");
    fs::write(
        &path,
        format!(
            "<p>override-created</p>{}",
            fs::read_to_string(&original).unwrap()
        ),
    )
    .unwrap();
    wait("override-created");
    fs::remove_file(path).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while get("/").contains("override-created") {
        assert!(
            Instant::now() < deadline,
            "removed override remained installed"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(server.0.try_wait().unwrap().is_none());
    let script = root.join("berlin.pipeline.rhai");
    fs::write(
        &script,
        fs::read_to_string(&script)
            .unwrap()
            .replace("Selected Work in System Design", "Edited Composer Title"),
    )
    .unwrap();
    wait("Edited Composer Title");
    let source = root.join("content/storage.md");
    fs::write(
        &source,
        fs::read_to_string(&source)
            .unwrap()
            .replace("Storage handoffs", "Edited source title"),
    )
    .unwrap();
    wait("Edited source title");
}

#[test]
fn p09_composed_websites_use_exact_sealed_bundles_without_legacy_routes() {
    let project = project();
    let root = project.path();
    let script = root.join("berlin.pipeline.rhai");
    fs::write(
        &script,
        fs::read_to_string(&script).unwrap().replace(
            "author: \"Example Author\"}));",
            "author: \"Example Author\"})).deploy_to(github_pages(\"example/notebook\"));",
        ),
    )
    .unwrap();
    let result = run(root, &["release", "--pipeline", "portfolio"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let id = String::from_utf8(result.stdout).unwrap().trim().to_owned();
    let bundle = root.join(".berlin/releases").join(&id);
    let manifest = json(&bundle.join("manifest.json"));
    assert!(manifest.get("documents").is_none());
    assert!(bundle.join("site/publication.json").is_file());
    assert!(bundle.join("site/index.html").is_file());
    assert!(!root.join(".berlin/publications").exists());
    // Import is a byte bundle, with no source/build relationship invented.
    success(run(root, &["build", "--pipeline", "portfolio"]));
    let result = run(
        root,
        &[
            "release",
            "--pipeline",
            "portfolio",
            "--from-directory",
            "_portfolio",
        ],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let imported = String::from_utf8(result.stdout).unwrap().trim().to_owned();
    let manifest = json(
        &root
            .join(".berlin/releases")
            .join(imported)
            .join("manifest.json"),
    );
    assert!(manifest.get("documents").is_none());
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
            .get("styles/publications/portfolio.css")
            .is_some()
    );
    assert!(html["render_inputs"].get("composition/catalog").is_some());
    let styles = fs::read_dir(root.join("_portfolio/assets/css"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(styles, vec![std::ffi::OsString::from("portfolio.css")]);
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
