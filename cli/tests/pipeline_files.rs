use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bln"))
        // File selection, sources, and outputs are relative to BERLIN_DIR, not cwd.
        .current_dir(root.parent().unwrap())
        .env("BERLIN_DIR", root)
        .args(args)
        .output()
        .unwrap()
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn multiple_files_share_helpers_constants_and_named_roots_and_record_inputs() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path().join("site");
    let data = workspace.path().join("data");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&data).unwrap();
    fs::write(data.join("example.txt"), "example").unwrap();
    fs::write(
        root.join("berlin.pipeline.rhai"),
        "invalid default must not be loaded",
    )
    .unwrap();
    fs::write(
        root.join("shared.rhai"),
        r#"
        const SOURCE_ROOTS = #{data: "../data"};
        const DESTINATION = "public";
        fn assets() { load_assets("@data/*") }
    "#,
    )
    .unwrap();
    fs::write(
        root.join("site.rhai"),
        "pipeline site { output copy_assets(assets(), DESTINATION); }",
    )
    .unwrap();
    let files = [
        "--pipeline-file",
        "shared.rhai",
        "--pipeline-file",
        "site.rhai",
    ];
    success(&run(&root, &[&["plan", "--json"][..], &files].concat()));
    success(&run(&root, &[&["build"][..], &files].concat()));
    assert_eq!(
        fs::read_to_string(root.join("public/example.txt")).unwrap(),
        "example"
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(".berlin/receipts/site.json")).unwrap())
            .unwrap();
    let inputs = receipt["inputs"].as_array().unwrap();
    for path in ["shared.rhai", "site.rhai"] {
        assert!(inputs.iter().any(|input| input["path"] == path));
    }
    assert!(
        !inputs
            .iter()
            .any(|input| input["path"] == "berlin.pipeline.rhai")
    );
}

#[test]
fn explicit_single_file_works_without_default_and_errors_identify_sources() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("custom.rhai"), "pipeline site {}").unwrap();
    success(&run(
        root.path(),
        &["plan", "--pipeline-file", "custom.rhai"],
    ));
    let missing = run(root.path(), &["plan", "--pipeline-file", "missing.rhai"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("missing.rhai"));
    fs::write(root.path().join("broken.rhai"), "pipeline site { invalid").unwrap();
    let invalid = run(
        root.path(),
        &[
            "plan",
            "--pipeline-file",
            "custom.rhai",
            "--pipeline-file",
            "broken.rhai",
        ],
    );
    assert!(!invalid.status.success());
    let error = String::from_utf8_lossy(&invalid.stderr);
    assert!(error.contains("broken.rhai") && error.contains("starts at line"));
}

#[test]
fn duplicate_files_and_pipeline_definitions_are_rejected_and_receipted() {
    let root = tempfile::tempdir().unwrap();
    for path in ["one.rhai", "two.rhai"] {
        fs::write(root.path().join(path), "pipeline site {}").unwrap();
    }
    for (second, expected) in [
        ("./one.rhai", "selected more than once"),
        ("two.rhai", "defined twice"),
    ] {
        let result = run(
            root.path(),
            &[
                "build",
                "--pipeline-file",
                "one.rhai",
                "--pipeline-file",
                second,
            ],
        );
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains(expected));
    }
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join(".berlin/receipts/site.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["status"], "failed");
    assert_eq!(receipt["inputs"].as_array().unwrap().len(), 2);
}

#[test]
fn check_uses_mappers_and_source_roots_from_selected_files() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("notes")).unwrap();
    fs::write(
        root.path().join("notes/note.md"),
        "---\nid: note\ntitle: Note\n---\nContent\n",
    )
    .unwrap();
    fs::write(
        root.path().join("shared.rhai"),
        r#"
        const SOURCE_ROOTS = #{notes: "notes"};
        fn hide(document) { document.draft = true; document }
        fn documents() { map_documents(parse_markdown(load_markdown("@notes/*.md")), Fn("hide")) }
    "#,
    )
    .unwrap();
    fs::write(root.path().join("site.rhai"), r#"
        pipeline site {
            let config = website_config(#{url: "https://example.com", title: "Test", author: "Author", description: "Test"});
            output render_website(assemble_website(documents(), parse_feed(load_data("missing.csv"))), "public", config);
        }
    "#).unwrap();
    let result = run(
        root.path(),
        &[
            "check",
            "--json",
            "--pipeline-file",
            "shared.rhai",
            "--pipeline-file",
            "site.rhai",
        ],
    );
    success(&result);
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["report"]["excluded_drafts"], 1);
}
