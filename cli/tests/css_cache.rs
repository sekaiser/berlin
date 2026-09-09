use std::{fs, path::Path, process::Command};

fn write(root: &Path, path: &str, value: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, value).unwrap();
}

fn run(root: &Path, args: &[&str], success: bool) -> String {
    let result = Command::new(env!("CARGO_BIN_EXE_bln"))
        .current_dir(root)
        .env("BERLIN_DIR", root)
        .args(args)
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&result.stderr).into_owned();
    assert_eq!(result.status.success(), success, "{log}");
    log
}

#[test]
fn css_reuses_transitive_inputs_and_rebuilds_when_the_graph_changes() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    write(
        root,
        "berlin.pipeline.rhai",
        r#"pipeline site {
        output compile_css(load_css("styles/entry.css"), "public/style.css");
    }"#,
    );
    write(
        root,
        "styles/entry.css",
        "@import 'nested.css'; .root { color: black }",
    );
    write(
        root,
        "styles/nested.css",
        "@import 'tokens.css'; .nested { color: red }",
    );
    write(root, "styles/tokens.css", ".token { color: blue }");
    run(root, &["build", "--dry-run"], true);
    assert!(!root.join(".berlin").exists());
    assert!(run(root, &["build"], true).contains("Compiling CSS"));
    let original = fs::read(root.join("public/style.css")).unwrap();
    assert!(run(root, &["build"], true).contains("Reusing CSS"));
    assert_eq!(original, fs::read(root.join("public/style.css")).unwrap());
    write(root, "styles/unrelated.css", ".unrelated { color: pink }");
    assert!(run(root, &["build"], true).contains("Reusing CSS"));
    write(root, "styles/tokens.css", ".token { color: green }");
    assert!(run(root, &["build"], true).contains("Compiling CSS"));
    assert_ne!(original, fs::read(root.join("public/style.css")).unwrap());
    // A new import in an existing dependency must replace the old dependency hint.
    write(
        root,
        "styles/nested.css",
        "@import 'extra.css'; .nested { color: red }",
    );
    write(root, "styles/extra.css", ".extra { color: orange }");
    assert!(run(root, &["build"], true).contains("Compiling CSS"));
    assert!(run(root, &["build"], true).contains("Reusing CSS"));
    write(
        root,
        "styles/tokens.css",
        ".no-longer-imported { color: gray }",
    );
    assert!(run(root, &["build"], true).contains("Reusing CSS"));
    let valid = fs::read(root.join("public/style.css")).unwrap();
    fs::remove_file(root.join("styles/extra.css")).unwrap();
    run(root, &["build"], false);
    assert_eq!(valid, fs::read(root.join("public/style.css")).unwrap());
    write(root, "styles/extra.css", ".extra { color: orange }");
    assert!(run(root, &["build"], true).contains("Reusing CSS"));
    let cache = root.join(".berlin/cache/artifacts-v1");
    let before = fs::read_dir(&cache)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (path.clone(), fs::read(path).unwrap())
        })
        .collect::<Vec<_>>();
    assert!(!run(root, &["build", "--no-cache"], true).contains("Reusing CSS"));
    assert_eq!(valid, fs::read(root.join("public/style.css")).unwrap());
    for (path, bytes) in &before {
        assert_eq!(fs::read(path).unwrap(), *bytes);
    }
    for (path, _) in before {
        fs::write(path, "broken entry").unwrap();
    }
    assert!(run(root, &["build"], true).contains("Compiling CSS"));
    assert!(run(root, &["build"], true).contains("Reusing CSS"));
}

fn themed_site(root: &Path) {
    fs::create_dir_all(root.join("theme/defaults")).unwrap();
    write(
        root,
        "berlin.pipeline.rhai",
        r#"pipeline site {
        output render_website(assemble_website(parse_markdown(load_markdown("content/*.md")),
            parse_feed(load_data("feed.csv"))), "_site", website_config(#{theme: "theme", url: "https://notebook.example.org"}));
    }"#,
    );
    write(
        root,
        "content/note.md",
        "---\nid: note\ntitle: Note\nauthor: [Author]\ndescription: A synthetic note.\ndate: 2026-09-09\n---\nBody.\n",
    );
    write(root, "feed.csv", "Title,URL,Date,Tags\n");
    for page in [
        "index.tera",
        "notes.tera",
        "feed.tera",
        "about.tera",
        "tags/base.tera",
    ] {
        write(root, &format!("theme/shared/pages/{page}"), "Page");
    }
    write(
        root,
        "theme/shared/pages/notes/[slug].tera",
        "{{ render() }}",
    );
    write(
        root,
        "theme/shared/styles/entries/main.css",
        "@import '../tokens.css'; .note { color: red }",
    );
    write(
        root,
        "theme/shared/styles/entries/independent.css",
        ".other { color: blue }",
    );
    write(
        root,
        "theme/shared/styles/tokens.css",
        ":root { --ink: green }",
    );
}

#[test]
fn theme_cache_survives_temporary_workspaces_and_honors_project_overrides() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    themed_site(root);
    assert_eq!(
        run(root, &["build"], true).matches("Compiling CSS").count(),
        2
    );
    assert_eq!(
        run(root, &["build"], true).matches("Reusing CSS").count(),
        2
    );
    // New override at the same logical path invalidates only its consumers.
    write(root, "styles/tokens.css", ":root { --ink: purple }");
    let log = run(root, &["build"], true);
    assert_eq!(log.matches("Reusing CSS").count(), 1, "{log}");
    assert_eq!(log.matches("Compiling CSS").count(), 1, "{log}");
    let path = root.join("_site/assets/css/main.css");
    assert!(fs::read_to_string(&path).unwrap().contains("--ink:purple"));
    write(
        root,
        "content/note.md",
        "---\nid: note\ntitle: Note\nauthor: [Author]\ndescription: A synthetic note.\ndate: 2026-09-09\n---\nEdited content.\n",
    );
    assert_eq!(
        run(root, &["build"], true).matches("Reusing CSS").count(),
        2
    );
    assert!(
        fs::read_to_string(root.join("_site/notes/note.html"))
            .unwrap()
            .contains("Edited content.")
    );
    write(
        root,
        "pages/notes/[slug].tera",
        "<article>{{ render() }}</article>",
    );
    assert_eq!(
        run(root, &["build"], true).matches("Reusing CSS").count(),
        2
    );
    assert!(
        fs::read_to_string(root.join("_site/notes/note.html"))
            .unwrap()
            .contains("<article>")
    );
    fs::remove_file(root.join("styles/tokens.css")).unwrap();
    run(root, &["build"], true);
    assert!(fs::read_to_string(path).unwrap().contains("--ink:green"));
}

#[test]
#[cfg(unix)]
fn changing_an_import_symlink_invalidates_cached_resolution() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    write(
        root,
        "berlin.pipeline.rhai",
        r#"pipeline site {
        output compile_css(load_css("styles/entry.css"), "public/style.css");
    }"#,
    );
    write(root, "styles/entry.css", "@import 'alias.css';");
    write(root, "styles/a.css", ".a { color: red }");
    write(root, "styles/b.css", ".b { color: blue }");
    std::os::unix::fs::symlink("a.css", root.join("styles/alias.css")).unwrap();
    run(root, &["build"], true);
    assert!(run(root, &["build"], true).contains("Reusing CSS"));
    fs::remove_file(root.join("styles/alias.css")).unwrap();
    std::os::unix::fs::symlink("b.css", root.join("styles/alias.css")).unwrap();
    assert!(run(root, &["build"], true).contains("Compiling CSS"));
    assert!(
        fs::read_to_string(root.join("public/style.css"))
            .unwrap()
            .contains(".b")
    );
}
