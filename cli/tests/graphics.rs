use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
fn run(root: &Path, args: &[&str], tools: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bln"));
    command.current_dir(root).env("BERLIN_DIR", root).args(args);
    if !tools {
        command.env("PATH", root.join("no-tools"));
    }
    command.output().unwrap()
}
fn success(output: Output) -> String {
    let log = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(output.status.success(), "{log}");
    log
}
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("inputs")).unwrap();
    fs::write(
        root.path().join("plot.tex"),
        include_str!("../../support/fixtures/graphics/plot.tex"),
    )
    .unwrap();
    fs::write(
        root.path().join("inputs/preamble.tex"),
        include_str!("../../support/fixtures/graphics/inputs/preamble.tex"),
    )
    .unwrap();
    fs::write(
        root.path().join("inputs/values.csv"),
        include_str!("../../support/fixtures/graphics/inputs/values.csv"),
    )
    .unwrap();
    fs::write(
        root.path().join("berlin.pipeline.rhai"),
        r#"
pipeline diagrams {
  output compile_diagrams(load_data("plot.tex"), load_assets("inputs/**/*"), "_site/diagrams");
}
"#,
    )
    .unwrap();
    root
}
#[test]
fn diagrams_require_consent_and_typed_engine_but_dry_runs_need_no_tools() {
    let root = fixture();
    assert!(
        !run(
            root.path(),
            &["render-diagram", "plot.tex", "--output", "plot.svg"],
            false
        )
        .status
        .success()
    );
    assert!(
        !run(root.path(), &["build", "--pipeline", "diagrams"], false)
            .status
            .success()
    );
    success(run(
        root.path(),
        &["build", "--pipeline", "diagrams", "--dry-run"],
        false,
    ));
    assert!(!root.path().join("_site").exists());
    // The earlier denied real build writes a failure receipt; a dry-run itself
    // must not create compiler cache state.
    assert!(!root.path().join(".berlin/cache").exists());
    assert!(
        !run(
            root.path(),
            &[
                "render-diagram",
                "plot.tex",
                "--output",
                "plot.svg",
                "--engine",
                "arbitrary-command"
            ],
            false
        )
        .status
        .success()
    );
}
#[test]
#[ignore = "requires the expanded Nix papers shell"]
fn graphics_cache_tracks_inputs_and_failed_render_keeps_previous_image() {
    let root = fixture();
    let args = [
        "render-diagram",
        "plot.tex",
        "--output",
        "plot.svg",
        "--input-root",
        "inputs",
        "--allow-tex",
    ];
    assert!(success(run(root.path(), &args, true)).contains("Compiling LaTeX graphic"));
    let original = fs::read(root.path().join("plot.svg")).unwrap();
    assert!(success(run(root.path(), &args, true)).contains("Reusing LaTeX graphic"));
    fs::write(
        root.path().join("inputs/values.csv"),
        "x,y\n0,0\n1,3\n2,6\n3,9\n",
    )
    .unwrap();
    assert!(success(run(root.path(), &args, true)).contains("Compiling LaTeX graphic"));
    let changed = fs::read(root.path().join("plot.svg")).unwrap();
    assert_ne!(changed, original);
    fs::write(root.path().join("plot.tex"), "\\undefinedBerlinCommand").unwrap();
    assert!(!run(root.path(), &args, true).status.success());
    assert_eq!(changed, fs::read(root.path().join("plot.svg")).unwrap());
}
#[test]
#[ignore = "requires the expanded Nix papers shell"]
fn typed_pipeline_uses_the_same_renderer_and_records_assets() {
    let root = fixture();
    success(run(
        root.path(),
        &["build", "--pipeline", "diagrams", "--allow-tex"],
        true,
    ));
    let svg = fs::read(root.path().join("_site/diagrams/plot.svg")).unwrap();
    let log = success(run(
        root.path(),
        &[
            "render-diagram",
            "plot.tex",
            "--output",
            "plot.svg",
            "--input-root",
            "inputs",
            "--allow-tex",
        ],
        true,
    ));
    assert!(log.contains("Reusing LaTeX graphic"), "{log}");
    assert_eq!(svg, fs::read(root.path().join("plot.svg")).unwrap());
    let receipt = fs::read_to_string(root.path().join(".berlin/receipts/diagrams.json")).unwrap();
    assert!(receipt.contains("inputs/values.csv") && receipt.contains("inputs/preamble.tex"));
    assert_eq!(
        fs::read_dir(root.path().join("_site/diagrams"))
            .unwrap()
            .count(),
        1
    );
}
#[test]
#[ignore = "requires LuaLaTeX and TikZ graphdrawing in the expanded Nix papers shell"]
fn luatex_graph_layout_and_fonts_render_without_shell_escape() {
    let root = fixture();
    fs::write(
        root.path().join("graph.tex"),
        include_str!("../../support/fixtures/graphics/graph.tex"),
    )
    .unwrap();
    success(run(
        root.path(),
        &[
            "render-diagram",
            "graph.tex",
            "--output",
            "graph.svg",
            "--engine",
            "lualatex",
            "--allow-tex",
        ],
        true,
    ));
    let svg = fs::read_to_string(root.path().join("graph.svg")).unwrap();
    assert!(svg.contains("<svg") && svg.contains("<path"));
}
