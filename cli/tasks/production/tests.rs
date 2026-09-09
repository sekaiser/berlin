use super::*;

fn put(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn page(root: &Path, name: &str, url: &str, body: &str) {
    put(
        root,
        name,
        &format!(
            "<!doctype html><html><head><link rel='canonical' href='{url}'></head><body>{body}</body></html>"
        ),
    );
}

fn finish_at(root: &Path, base: &str) -> Result<(), Error> {
    finish(root, &Url::parse(base).unwrap())
}

#[test]
fn versions_final_css_dependencies_and_normalizes_root_and_repository_links() {
    for base in ["https://example.com", "https://example.com/notebook"] {
        let mut versions = Vec::new();
        for image in ["before", "after"] {
            let root = tempfile::tempdir().unwrap();
            put(root.path(), "image.png", image);
            put(root.path(), "css/other.css", ".x {color:red}");
            put(
                root.path(),
                "css/style.css",
                "@import 'other.css';:root { --image: url('../image.png'); } .x{background:var(--image)}",
            );
            page(
                root.path(),
                "index.html",
                &format!("{base}/"),
                "<link rel=stylesheet href=css/style.css><img src='/image.png'><a href='notes/note.html#details'>Note</a>",
            );
            page(
                root.path(),
                "notes/note.html",
                &format!("{base}/notes/note.html"),
                "<h2 id=details>Details</h2><a href='/'>Home</a>",
            );
            finish_at(root.path(), base).unwrap();
            let html = fs::read_to_string(root.path().join("index.html")).unwrap();
            let css = fs::read_to_string(root.path().join("css/style.css")).unwrap();
            assert!(html.contains(&format!("{base}/image.png?v=")));
            assert!(html.contains(&format!("{base}/css/style.css?v=")));
            assert!(css.contains(&format!("{base}/css/other.css?v=")));
            assert!(css.contains(&format!("{base}/image.png?v=")));
            assert!(
                fs::read_to_string(root.path().join("robots.txt"))
                    .unwrap()
                    .contains(&format!("{base}/sitemap.xml"))
            );
            assert!(root.path().join("licenses/berlin/LICENSE.txt").is_file());
            versions.push((html, css));
        }
        assert_ne!(versions[0].0, versions[1].0);
        assert_ne!(versions[0].1, versions[1].1);
    }
}

#[test]
fn html_rewriting_respects_entities_srcsets_quoted_angles_and_nonmarkup_text() {
    let root = tempfile::tempdir().unwrap();
    put(root.path(), "a&b.png", "image");
    let untouched = r#"<script>const snippet = '<img src="/missing.png">';</script><pre>&lt;a href="/missing.html"&gt;</pre><!-- <img src="/missing.png"> -->"#;
    page(
        root.path(),
        "index.html",
        "https://example.com/notebook/",
        &format!(
            r#"{untouched}<img title='a > b' src='a&amp;b.png?q=a&amp;v=old#x' srcset='data:image/png;base64,AAAA 1x, /notebook/a&amp;b.png 2x'><meta property='og:image' content='/a&amp;b.png'><a href='#a&amp;b'>Jump</a><h2 id='a&amp;b'>Title</h2>"#
        ),
    );
    finish_at(root.path(), "https://example.com/notebook").unwrap();
    let html = fs::read_to_string(root.path().join("index.html")).unwrap();
    assert!(html.contains(untouched));
    assert!(html.contains("a&amp;b.png?q=a&amp;v="));
    assert!(!html.contains("v=old"));
    assert!(!html.contains("notebook/notebook"));
    assert!(
        html.contains("data:image/png;base64,AAAA 1x, https://example.com/notebook/a&amp;b.png?v=")
    );
    assert!(html.contains("#a&amp;b"));
}

#[test]
fn rejects_broken_links_fragments_canonicals_and_preview_references() {
    for (body, expected) in [
        (
            "<a href='/missing.html'>Missing</a>",
            "Missing local destination",
        ),
        ("<a href='#missing'>Missing</a>", "Missing fragment"),
        ("<img src='http://127.0.0.1:8081/a.png'>", "Preview URL"),
        ("<a href='https://LOCALHOST/x'>Local</a>", "Preview URL"),
        ("<a href='file:///secret'>File</a>", "Unsafe URL"),
        ("<a href='java&#x73;cript:alert(1)'>Bad</a>", "Unsafe URL"),
        ("<script src='/__berlin/live.js'></script>", "Preview URL"),
        (
            "<meta name='robots' content='noindex, follow'>",
            "Preview-only markup",
        ),
        (
            "<link rel='canonical' href='https://example.com/'>",
            "Duplicate canonical",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        page(root.path(), "index.html", "https://example.com/", body);
        let error = finish_at(root.path(), "https://example.com").unwrap_err();
        assert!(format!("{error:#}").contains(expected), "{error:#}");
    }
    let root = tempfile::tempdir().unwrap();
    page(root.path(), "index.html", "https://wrong.example/", "");
    assert!(
        finish_at(root.path(), "https://example.com")
            .unwrap_err()
            .to_string()
            .contains("Wrong canonical")
    );
}

#[test]
fn rejects_sources_unused_preview_assets_symlinks_and_cyclic_css() {
    for name in [
        "secret.org",
        "private.csv",
        ".DS_Store",
        "static/js/live.js",
    ] {
        let root = tempfile::tempdir().unwrap();
        page(root.path(), "index.html", "https://example.com/", "");
        put(root.path(), name, "private");
        assert!(finish_at(root.path(), "https://example.com").is_err());
    }
    let root = tempfile::tempdir().unwrap();
    page(root.path(), "index.html", "https://example.com/", "");
    put(root.path(), "a.css", "@import 'b.css';");
    put(root.path(), "b.css", "@import 'a.css';");
    let error = finish_at(root.path(), "https://example.com").unwrap_err();
    assert!(format!("{error:#}").contains("Cyclic asset references"));
    #[cfg(unix)]
    {
        let root = tempfile::tempdir().unwrap();
        page(root.path(), "index.html", "https://example.com/", "");
        std::os::unix::fs::symlink("index.html", root.path().join("alias.html")).unwrap();
        assert!(
            finish_at(root.path(), "https://example.com")
                .unwrap_err()
                .to_string()
                .contains("symlink")
        );
    }
}

fn redirect(root: &Path, target: &str) {
    page(
        root,
        "notes/old.html",
        &format!("https://example.com/notes/{target}.html"),
        &format!(
            "<meta name='berlin-redirect' content='notes/{target}.html'><meta name='robots' content='noindex, follow'><meta http-equiv='refresh' content='0;url=https://example.com/notes/{target}.html'>"
        ),
    );
}

#[test]
fn checks_redirected_fragments_and_omits_aliases_from_sitemap() {
    let root = tempfile::tempdir().unwrap();
    page(
        root.path(),
        "index.html",
        "https://example.com/",
        "<a href='notes/old.html#details'>Old URL</a>",
    );
    page(
        root.path(),
        "notes/current.html",
        "https://example.com/notes/current.html",
        "<h2 id='details'>Details</h2>",
    );
    redirect(root.path(), "current");
    finish_at(root.path(), "https://example.com").unwrap();
    let sitemap = fs::read_to_string(root.path().join("sitemap.xml")).unwrap();
    assert!(sitemap.contains("notes/current.html"));
    assert!(!sitemap.contains("notes/old.html"));
    for target in ["old", "missing"] {
        redirect(root.path(), target);
        assert!(
            finish_at(root.path(), "https://example.com")
                .unwrap_err()
                .to_string()
                .contains("Invalid redirect")
        );
    }
}

#[test]
fn encoded_filenames_resolve_without_decoding_them_into_different_urls() {
    let root = tempfile::tempdir().unwrap();
    put(root.path(), "100%.png", "image");
    page(
        root.path(),
        "index.html",
        "https://example.com/notebook/",
        "<img src='100%25.png'>",
    );
    finish_at(root.path(), "https://example.com/notebook").unwrap();
    assert!(
        fs::read_to_string(root.path().join("index.html"))
            .unwrap()
            .contains("100%25.png?v=")
    );
}

#[test]
fn supports_links_to_generated_discovery_files_and_an_unterminated_site_base() {
    let root = tempfile::tempdir().unwrap();
    page(
        root.path(),
        "index.html",
        "https://example.com/notebook/",
        "<a href='/notebook'>Home</a><a href='https://example.com/notebook'>Home</a><a href='sitemap.xml'>Sitemap</a>",
    );
    finish_at(root.path(), "https://example.com/notebook").unwrap();
    let html = fs::read_to_string(root.path().join("index.html")).unwrap();
    assert!(html.contains("href=\"https://example.com/notebook/\""));
    assert!(html.contains("sitemap.xml?v="));
    assert!(!html.contains("notebook/notebook"));
}
