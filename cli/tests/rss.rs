use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const PIPELINE: &str = r#"
let c=catalog([documents(load_markdown("content/*.md"))],publication_options(#{source_site_url:"https://example.test/notebook"}));
pipeline site {
    output render_html(compose(c,notebook()),"_site",html_config(#{
        theme:"@notebook",title:"RSS fixture",author:"Author",description:"A notebook",url:"https://example.test/notebook",rss:true
    })).deploy_to(github_pages("example/notebook"));
}
"#;
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
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("content")).unwrap();
    fs::write(root.path().join("berlin.pipeline.rhai"), PIPELINE).unwrap();
    for (id, draft, body) in [
        ("note", false, "Published body"),
        ("draft", true, "DRAFT_PRIVATE_SENTINEL"),
    ] {
        fs::write(root.path().join(format!("content/{id}.md")),format!("---\nid: {id}\ntitle: {id}\nslug: {id}\nauthor: [Author]\ndescription: Summary for {id}\ndate: 2026-10-31\ndraft: {draft}\n---\n{body}\n")).unwrap();
    }
    root
}

#[test]
fn production_feed_is_discoverable_at_a_stable_url_and_part_of_the_sealed_release() {
    let root = fixture();
    let root = root.path();
    success(run(root, &["build", "--production"]));
    let rss = fs::read(root.join("_site/rss.xml")).unwrap();
    let xml = String::from_utf8_lossy(&rss);
    assert!(xml.contains("https://example.test/notebook/notes/note.html"));
    assert!(xml.contains("Sat, 31 Oct 2026 00:00:00 GMT"));
    assert!(xml.contains("Summary for note"));
    assert!(!xml.contains("DRAFT_PRIVATE_SENTINEL"));
    assert!(!xml.contains("Published body"));
    for path in [
        "index.html",
        "notes.html",
        "feed.html",
        "notes/note.html",
        "tags/untagged.html",
    ] {
        let path = root.join("_site").join(path);
        if !path.exists() {
            continue;
        }
        let html = fs::read_to_string(&path).unwrap();
        assert!(
            html.contains("rel=\"alternate\" type=\"application/rss+xml\""),
            "{}",
            path.display()
        );
        assert!(html.contains("Subscribe via RSS"));
        assert!(html.contains("https://example.test/notebook/rss.xml\""));
        assert!(
            !html.contains("rss.xml?v="),
            "feed subscriptions must not receive cache-busting versions"
        );
    }
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("_site/publication.json")).unwrap()).unwrap();
    assert!(
        manifest["output"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == "rss.xml")
    );
    let id = success(run(root, &["release", "--production"]))
        .trim()
        .to_string();
    let bundle = root.join(".berlin/releases").join(&id);
    let sealed: Value =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap()).unwrap();
    assert!(
        sealed["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == "rss.xml")
    );
    assert_eq!(fs::read(bundle.join("site/rss.xml")).unwrap(), rss);
    success(run(root, &["build", "--production"]));
    assert_eq!(fs::read(root.join("_site/rss.xml")).unwrap(), rss);
    fs::write(
        root.join("berlin.pipeline.rhai"),
        PIPELINE.replace("rss:true", "rss:false"),
    )
    .unwrap();
    success(run(root, &["build", "--production"]));
    assert!(!root.join("_site/rss.xml").exists());
    assert!(
        !fs::read_to_string(root.join("_site/index.html"))
            .unwrap()
            .contains("application/rss+xml")
    );
}

#[test]
fn an_invalid_feed_does_not_replace_the_installed_website() {
    let root = fixture();
    let root = root.path();
    success(run(root, &["build"]));
    let before = fs::read(root.join("_site/rss.xml")).unwrap();
    let home = fs::read(root.join("_site/index.html")).unwrap();
    let path = root.join("content/note.md");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("2026-10-31", "1899-12-31"),
    )
    .unwrap();
    let failed = run(root, &["build"]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("RSS publication dates"));
    assert_eq!(fs::read(root.join("_site/rss.xml")).unwrap(), before);
    assert_eq!(fs::read(root.join("_site/index.html")).unwrap(), home);
}
