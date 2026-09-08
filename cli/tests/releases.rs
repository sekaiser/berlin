use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bln"))
        .env("BERLIN_DIR", root)
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
fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &to.join(entry.file_name()));
        } else {
            fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
}
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    copy(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../support/fixtures/publishing"),
        root.path(),
    );
    let path = root.path().join("berlin.pipeline.rhai");
    let script = fs::read_to_string(&path).unwrap().replace(".berlin/generated/org/content/notes/*.md", "content/*.md")
        .replace("render_website(website, \"_site\", presentation)", "render_website(website, \"_site\", presentation).deploy_to(github_pages(\"owner/site\"))");
    fs::write(path, script).unwrap();
    fs::create_dir_all(root.path().join("content")).unwrap();
    fs::write(root.path().join("content/note.md"), "---\nid: note\ntitle: Note\ndescription: A test note\nauthor: [Author]\ndate: 2026-09-08\n---\nOriginal article\n").unwrap();
    root
}
fn prepare(root: &Path) -> String {
    success(run(root, &["release"])).trim().to_owned()
}

fn snapshot(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn collect(
        root: &Path,
        directory: &Path,
        files: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().into(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut files = std::collections::BTreeMap::new();
    collect(root, root, &mut files);
    files
}

#[test]
fn prepared_directory_is_copied_without_changes_or_rebuilding() {
    let root = fixture();
    success(run(root.path(), &["build"]));
    let prepared = root.path().join("_site");
    let original = snapshot(&prepared);
    let id = success(run(root.path(), &["release", "--from-directory", "_site"]))
        .trim()
        .to_owned();
    assert_eq!(
        snapshot(&prepared),
        original,
        "support files must not be added to the source"
    );
    assert_eq!(
        id,
        prepare(root.path()),
        "same bytes and declaration give the same release ID"
    );

    fs::write(prepared.join("sitemap.xml"), "prepared sitemap").unwrap();
    fs::write(prepared.join("finished.bin"), [0, 255, 13, 10]).unwrap();
    fs::write(prepared.join("result.csv"), "authored,download\n").unwrap();
    fs::write(prepared.join("index.html"), "Externally prepared HTML\r\n").unwrap();
    let finished = snapshot(&prepared);
    fs::write(root.path().join("content/note.md"), "---\ninvalid: [\n---").unwrap();
    assert!(!run(root.path(), &["release"]).status.success());
    let second = success(run(root.path(), &["release", "--from-directory", "_site"]))
        .trim()
        .to_owned();
    assert_ne!(id, second);
    assert_eq!(snapshot(&prepared), finished);
    let bundle = root
        .path()
        .join(".berlin/releases")
        .join(second)
        .join("site");
    for (file, bytes) in finished {
        assert_eq!(fs::read(bundle.join(file)).unwrap(), bytes);
    }
    assert!(bundle.join(".nojekyll").is_file());
    assert!(bundle.join("licenses/berlin/LICENSE.txt").is_file());
    assert!(!root.path().join(".berlin/publications").exists());
}

#[test]
fn prepared_directory_supports_external_paths_and_selected_pipeline_files() {
    let root = fixture();
    let prepared = tempfile::tempdir().unwrap();
    fs::write(
        prepared.path().join("index.html"),
        "Prepared outside the project",
    )
    .unwrap();
    let config = root.path().join("berlin.pipeline.rhai");
    let source = fs::read_to_string(&config).unwrap();
    fs::write(
        root.path().join("production.rhai"),
        source.replace("owner/site", "owner/production"),
    )
    .unwrap();
    fs::remove_file(config).unwrap();
    let id = success(run(
        root.path(),
        &[
            "release",
            "--pipeline-file",
            "production.rhai",
            "--from-directory",
            prepared.path().to_str().unwrap(),
        ],
    ))
    .trim()
    .to_owned();
    let plan = success(run(root.path(), &["release-plan", &id]));
    assert!(plan.contains("owner/production"));
    assert_eq!(snapshot(prepared.path()).len(), 1);
    assert!(!root.path().join("_site").exists());
    assert!(!root.path().join(".berlin/receipts").exists());
}

#[test]
fn prepared_directory_requires_a_valid_website_declaration() {
    let root = fixture();
    let prepared = root.path().join("prepared");
    fs::create_dir(&prepared).unwrap();
    fs::write(prepared.join("index.html"), "Prepared").unwrap();
    let config = root.path().join("berlin.pipeline.rhai");
    let source = fs::read_to_string(&config).unwrap();
    for script in [
        source.replace(".deploy_to(github_pages(\"owner/site\"))", ""),
        source.replace("https://example.com", "http://localhost:8081"),
        source.replace("\"_site/static\"", "\"other/static\""),
    ] {
        fs::write(&config, script).unwrap();
        assert!(
            !run(root.path(), &["release", "--from-directory", "prepared"])
                .status
                .success()
        );
        assert_eq!(snapshot(&prepared).len(), 1);
        assert!(!root.path().join(".berlin/releases").exists());
    }
    fs::write(&config, source).unwrap();
    assert!(
        !run(
            root.path(),
            &[
                "release",
                "--pipeline",
                "linkedin",
                "--from-directory",
                "prepared"
            ]
        )
        .status
        .success()
    );
    assert!(
        !run(root.path(), &["release", "--from-directory", "."])
            .status
            .success()
    );
    let state = root.path().join(".berlin");
    fs::create_dir(&state).unwrap();
    fs::write(state.join("index.html"), "Not a safe input directory").unwrap();
    let original = snapshot(&state);
    assert!(
        !run(root.path(), &["release", "--from-directory", ".berlin"])
            .status
            .success()
    );
    assert_eq!(
        snapshot(&state),
        original,
        "reject state overlap before creating release directories"
    );
    assert!(!state.join("releases").exists());
}

#[test]
fn rejected_prepared_files_leave_no_bundle_and_do_not_change_the_input() {
    for (file, contents) in [
        ("CNAME", "unreviewed.example.com\n"),
        ("licenses/berlin/LICENSE.txt", "conflicting licence"),
        (".git/config", "Git control file"),
        ("index.html", "<script src=\"/__berlin/live.js\"></script>"),
        ("index.html", ""),
    ] {
        let root = fixture();
        let prepared = root.path().join("prepared");
        fs::create_dir(&prepared).unwrap();
        fs::write(prepared.join("index.html"), "Prepared").unwrap();
        let target = prepared.join(file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, contents).unwrap();
        let original = snapshot(&prepared);
        assert!(
            !run(root.path(), &["release", "--from-directory", "prepared"])
                .status
                .success()
        );
        assert_eq!(snapshot(&prepared), original);
        let releases = root.path().join(".berlin/releases");
        assert!(!releases.exists() || fs::read_dir(releases).unwrap().count() == 0);
    }
}

#[cfg(unix)]
#[test]
fn prepared_directory_rejects_symlinked_roots_and_contents() {
    let root = fixture();
    let prepared = root.path().join("prepared");
    fs::create_dir(&prepared).unwrap();
    fs::write(prepared.join("index.html"), "Prepared").unwrap();
    std::os::unix::fs::symlink(&prepared, root.path().join("linked")).unwrap();
    assert!(
        !run(root.path(), &["release", "--from-directory", "linked"])
            .status
            .success()
    );
    std::os::unix::fs::symlink(
        root.path().join("content/note.md"),
        prepared.join("private.txt"),
    )
    .unwrap();
    assert!(
        !run(root.path(), &["release", "--from-directory", "prepared"])
            .status
            .success()
    );
}

#[test]
fn release_is_sealed_separate_from_preview_and_independent_of_future_builds() {
    let root = fixture();
    success(run(root.path(), &["build"]));
    let preview = fs::read(root.path().join("_site/notes/note.html")).unwrap();
    let id = prepare(root.path());
    assert_eq!(id.len(), 64);
    assert_eq!(
        fs::read(root.path().join("_site/notes/note.html")).unwrap(),
        preview
    );
    let directory = root.path().join(".berlin/releases").join(&id);
    assert!(directory.join("site/licenses/berlin/LICENSE.txt").is_file());
    assert!(directory.join("site/.nojekyll").is_file());
    let old = fs::read(directory.join("site/notes/note.html")).unwrap();
    assert_eq!(
        id,
        prepare(root.path()),
        "identical inputs seal the same release"
    );
    fs::write(root.path().join("content/note.md"), "---\nid: note\ntitle: Note\ndescription: A test note\nauthor: [Author]\ndate: 2026-09-08\n---\nChanged article\n").unwrap();
    success(run(root.path(), &["build"]));
    assert_ne!(
        fs::read(root.path().join("_site/notes/note.html")).unwrap(),
        old
    );
    assert_eq!(
        fs::read(directory.join("site/notes/note.html")).unwrap(),
        old
    );
    assert_ne!(id, prepare(root.path()));
    fs::write(
        root.path().join("berlin.pipeline.rhai"),
        "broken current configuration",
    )
    .unwrap();
    let plan = success(run(root.path(), &["release-plan", &id]));
    assert!(plan.contains("owner/site"));
}

#[test]
fn modified_missing_and_added_release_files_are_rejected_before_publication() {
    let root = fixture();
    let id = prepare(root.path());
    let site = root.path().join(".berlin/releases").join(&id).join("site");
    let original = fs::read(site.join("index.html")).unwrap();
    fs::write(site.join("index.html"), "unreviewed").unwrap();
    assert!(
        !run(root.path(), &["publish", &id, "--confirm", &id])
            .status
            .success()
    );
    assert!(!root.path().join(".berlin/publications").exists());
    fs::write(site.join("index.html"), &original).unwrap();
    fs::write(site.join("extra.txt"), "extra").unwrap();
    assert!(!run(root.path(), &["release-plan", &id]).status.success());
    fs::remove_file(site.join("extra.txt")).unwrap();
    fs::remove_file(site.join("index.html")).unwrap();
    assert!(!run(root.path(), &["release-plan", &id]).status.success());
}

#[test]
fn destination_and_public_url_are_part_of_release_identity() {
    let root = fixture();
    let first = prepare(root.path());
    let path = root.path().join("berlin.pipeline.rhai");
    let script = fs::read_to_string(&path).unwrap();
    fs::write(&path, script.replace("owner/site", "owner/other")).unwrap();
    let second = prepare(root.path());
    assert_ne!(first, second);
    assert!(
        !run(root.path(), &["publish", &second, "--confirm", &first])
            .status
            .success()
    );
    assert!(!root.path().join(".berlin/publications").exists());
    fs::write(
        &path,
        script.replace("https://example.com", "http://localhost:8081"),
    )
    .unwrap();
    assert!(!run(root.path(), &["release"]).status.success());
}

#[test]
fn declared_deployment_does_not_change_build_behavior() {
    let root = fixture();
    success(run(root.path(), &["plan"]));
    success(run(root.path(), &["build"]));
    assert!(!root.path().join(".berlin/releases").exists());
    assert!(!root.path().join(".berlin/publications").exists());
    assert!(
        !run(root.path(), &["publish", &"a".repeat(64)])
            .status
            .success()
    );
    assert!(
        !run(root.path(), &["release-plan", "../outside"])
            .status
            .success()
    );
}

#[cfg(unix)]
#[test]
fn sealed_release_symlinks_are_rejected() {
    let root = fixture();
    let id = prepare(root.path());
    let site = root.path().join(".berlin/releases").join(&id).join("site");
    std::os::unix::fs::symlink(
        root.path().join("content/note.md"),
        site.join("private.txt"),
    )
    .unwrap();
    assert!(!run(root.path(), &["release-plan", &id]).status.success());
}
