//! Full local update-to-draft workflow, including production preparation.
use super::*;

fn production_fixture(base: &str) -> tempfile::TempDir {
    let root = fixture();
    fs::write(
        root.path().join("berlin.pipeline.rhai"),
        PIPELINE.replace("https://example.com", base),
    )
    .unwrap();
    fs::write(
        root.path().join("static/style.css"),
        "body { background: url(image.png); }",
    )
    .unwrap();
    fs::write(root.path().join("static/image.png"), "image").unwrap();
    fs::create_dir(root.path().join("_site")).unwrap();
    fs::write(root.path().join("_site/index.html"), "preview sentinel").unwrap();
    root
}

fn production_release(root: &Path) -> String {
    success(run(root, &["release", "--production"]))
        .trim()
        .to_owned()
}

#[test]
fn equations_survive_production_and_affect_revisions_without_partial_publication() {
    let root = production_fixture("https://example.com");
    fs::write(
        root.path().join("pages/search.tera"),
        "<link rel='canonical' href='{{config_site_url}}{{page_path}}'>Search",
    )
    .unwrap();
    note(
        root.path(),
        "note",
        "Note",
        "",
        r"Inline \(x_i\). Display \[\frac{1}{2}\]",
    );
    let before = production_release(root.path());
    let html = fs::read_to_string(
        root.path()
            .join(".berlin/releases")
            .join(&before)
            .join("site/notes/note.html"),
    )
    .unwrap();
    assert!(html.contains("<math display=\"inline\">"));
    assert!(html.contains("<mfrac>"));
    assert!(html.contains("Copyright 2024 Charles Edward Gagnon"));
    assert!(!html.contains("<script"));
    let search = fs::read_to_string(
        root.path()
            .join(".berlin/releases")
            .join(&before)
            .join("site/search/index.json"),
    )
    .unwrap();
    assert!(search.contains("x_i"));

    note(
        root.path(),
        "note",
        "Note",
        "",
        r"Inline \(x_j\). Display \[\frac{1}{2}\]",
    );
    let after = production_release(root.path());
    let report = compare(root.path(), &before, &after);
    assert_eq!(changes(&report)["modified"][0]["content_changed"], true);
    assert_eq!(production_release(root.path()), after);

    note(root.path(), "note", "Note", "", r"\[\input{private.tex}\]");
    let failed = run(root.path(), &["build", "--production"]);
    assert!(!failed.status.success());
    assert!(
        String::from_utf8_lossy(&failed.stderr).contains("invalid or unsupported TeX equation")
    );
    assert_eq!(
        fs::read_to_string(root.path().join("_site/index.html")).unwrap(),
        "preview sentinel"
    );
}

#[test]
fn production_revisions_export_review_drafts_without_sources_or_publication() {
    for base in ["https://example.com", "https://example.com/notebook"] {
        let root = production_fixture(base);
        note(
            root.path(),
            "private",
            "Private title",
            "draft: true\n",
            "Secret prose.",
        );
        fs::write(
            root.path().join("content/private.org"),
            "Unexported private data",
        )
        .unwrap();
        let before = production_release(root.path());
        let baseline = manifest(root.path(), &before);
        assert_eq!(baseline["schema_version"], 2);
        assert_eq!(
            baseline["documents"]["entries"].as_array().unwrap().len(),
            1
        );
        assert!(!baseline.to_string().contains("Private title"));
        let site = root
            .path()
            .join(".berlin/releases")
            .join(&before)
            .join("site");
        let html = fs::read_to_string(site.join("index.html")).unwrap();
        assert!(html.contains(&format!("{base}/static/style.css?v=")));
        assert!(
            fs::read_to_string(site.join("static/style.css"))
                .unwrap()
                .contains(&format!("{base}/static/image.png?v="))
        );
        assert!(site.join("sitemap.xml").is_file());
        assert!(site.join("licenses/berlin/LICENSE.txt").is_file());
        assert!(!site.join("content").exists());
        assert!(!site.join("manifest.json").exists());
        assert_eq!(production_release(root.path()), before);

        note(
            root.path(),
            "note",
            "Note",
            "",
            "Initial prose.\n\nA deliberate revision.",
        );
        let revised = production_release(root.path());
        let report = compare(root.path(), &before, &revised);
        assert_eq!(changes(&report)["modified"].as_array().unwrap().len(), 1);
        assert_eq!(changes(&report)["modified"][0]["content_changed"], true);
        let drafts = serde_json::from_str::<Value>(&success(run(
            root.path(),
            &[
                "release-drafts",
                &before,
                &revised,
                "--json",
                "--output",
                ".berlin/review",
            ],
        )))
        .unwrap();
        assert_eq!(drafts["drafts"].as_array().unwrap().len(), 1);
        assert_eq!(
            drafts["drafts"][0]["url"],
            format!("{base}/notes/note.html")
        );
        assert!(root.path().join(".berlin/review/manifest.json").is_file());
        assert!(!drafts.to_string().contains("deliberate revision"));

        // Theme/asset bytes change after the revised release, but documents do not.
        fs::write(root.path().join("static/image.png"), "new image").unwrap();
        let themed = production_release(root.path());
        let report = compare(root.path(), &revised, &themed);
        assert!(!report["files"]["modified"].as_array().unwrap().is_empty());
        assert!(changes(&report)["modified"].as_array().unwrap().is_empty());
        let drafts: Value = serde_json::from_str(&success(run(
            root.path(),
            &["release-drafts", &revised, &themed, "--json"],
        )))
        .unwrap();
        assert!(drafts["drafts"].as_array().unwrap().is_empty());
        assert_eq!(
            fs::read_to_string(root.path().join("_site/index.html")).unwrap(),
            "preview sentinel"
        );
        assert!(!root.path().join(".berlin/publications").exists());
        assert!(!root.path().join(".berlin/receipts").exists());
    }
}

#[test]
fn production_failures_neither_install_nor_seal_and_dry_runs_do_not_prepare_files() {
    let root = production_fixture("https://example.com");
    fs::write(root.path().join("static/unintended.org"), "Private source").unwrap();
    success(run(root.path(), &["build", "--production", "--dry-run"]));
    assert!(!root.path().join(".berlin").exists());
    for command in ["release", "build"] {
        let output = run(root.path(), &[command, "--production"]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Source/private file"));
        assert!(!root.path().join(".berlin/releases").exists());
        assert_eq!(
            fs::read_to_string(root.path().join("_site/index.html")).unwrap(),
            "preview sentinel"
        );
    }
}

#[test]
fn production_build_prepares_output_without_requiring_a_deployment_target() {
    let root = production_fixture("https://example.com");
    fs::write(
        root.path().join("berlin.pipeline.rhai"),
        PIPELINE.replace(".deploy_to(github_pages(\"owner/site\"))", ""),
    )
    .unwrap();
    success(run(root.path(), &["build", "--production"]));
    assert!(root.path().join("_site/sitemap.xml").is_file());
    assert!(
        root.path()
            .join("_site/licenses/berlin/LICENSE.txt")
            .is_file()
    );
    assert!(!root.path().join(".berlin/releases").exists());
}

#[test]
fn production_is_incompatible_with_importing_unrelated_prepared_bytes() {
    let root = production_fixture("https://example.com");
    let output = run(
        root.path(),
        &["release", "--production", "--from-directory", "_site"],
    );
    assert!(!output.status.success());
    assert!(!root.path().join(".berlin").exists());
}
