use berlin_document::{ContentId, Metadata, Provenance, SourceFormat};
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
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for dir in [
        "content",
        "paper/assets/papers",
        "pages/notes",
        "pages/tags",
    ] {
        fs::create_dir_all(root.path().join(dir)).unwrap();
    }
    for (name, body) in [
        (
            "index.tera",
            "{% for a in articles %}{{a.title}}{% endfor %}",
        ),
        ("notes.tera", "Notes"),
        ("feed.tera", "Reading"),
        ("about.tera", "About"),
        ("tags/base.tera", "Tags"),
        ("notes/[slug].tera", "{{render()}}"),
    ] {
        fs::write(root.path().join("pages").join(name), format!("<link rel='canonical' href='{{{{config_site_url | safe}}}}{{{{page_path | safe}}}}'>{body}")).unwrap();
    }
    fs::write(root.path().join("feed.csv"), "Title,URL,Date,Tags\n").unwrap();
    fs::write(root.path().join("content/note.md"),"---\nid: note\ntitle: Ordinary note\nauthor: [Author]\ndescription: Ordinary note\ndate: 2026-09-09\n---\nExisting content.\n").unwrap();
    fs::write(root.path().join("berlin.pipeline.rhai"),r#"
pipeline site {
    let paper = parse_documents(named(load_data("paper/document.json"), "paper_sources"));
    let notes = parse_markdown(load_markdown("content/*.md"));
    let documents = merge_documents(notes, paper);
    output render_website(assemble_website(documents, parse_feed(load_data("feed.csv"))), "_site", website_config(#{url: "https://example.com/notebook"}));
    output copy_assets(load_assets("paper/assets/papers/*.pdf"), "_site/papers");
}
"#).unwrap();
    root
}

fn paper() -> berlin_document::Document {
    berlin_paper::import(
        include_str!("../../crates/bln-paper/tests/fixtures/paper.json"),
        &berlin_paper::Labels::parse(include_str!(
            "../../crates/bln-paper/tests/fixtures/paper.aux"
        ))
        .unwrap(),
        &[berlin_paper::BibliographyEntry {
            key: "example".into(),
            pandoc_json: include_str!("../../crates/bln-paper/tests/fixtures/reference.json")
                .into(),
        }],
        berlin_paper::Publication {
            id: ContentId("paper".into()),
            metadata: Metadata {
                published: Some("2026-09-09".parse().unwrap()),
                ..Default::default()
            },
            provenance: Provenance {
                source: "file:///paper.tex".into(),
                source_format: SourceFormat::Latex,
                source_hash: "0".repeat(64),
            },
            pdf_url: "/papers/paper.pdf".into(),
        },
    )
    .unwrap()
}

#[test]
fn prepared_papers_merge_with_notes_and_build_without_external_tools() {
    let root = fixture();
    let doc = paper();
    fs::write(
        root.path().join("paper/document.json"),
        serde_json::to_vec(&doc).unwrap(),
    )
    .unwrap();
    // This test checks opaque attachment transport, not PDF validity. The opt-in
    // test below exercises an actual TeX-generated PDF.
    let pdf = b"%PDF-transport-test";
    fs::write(root.path().join("paper/assets/papers/paper.pdf"), pdf).unwrap();
    success(run(root.path(), &["build", "--production"], false));
    let article = root
        .path()
        .join("_site/notes/a-small-study-of-incremental-publishing.html");
    let html = fs::read_to_string(&article).unwrap();
    assert!(html.contains("#eq:work"));
    assert!(html.contains("#ref-example"));
    assert!(html.contains("/notebook/papers/paper.pdf"));
    assert!(html.contains("Test fixture; not a real publication."));
    assert_eq!(
        fs::read(root.path().join("_site/papers/paper.pdf")).unwrap(),
        pdf
    );
    assert!(root.path().join("_site/notes/ordinary-note.html").exists());
    success(run(root.path(), &["check", "--json"], false));
    let mut duplicate = doc.clone();
    duplicate.id = ContentId("note".into());
    fs::write(
        root.path().join("paper/document.json"),
        serde_json::to_vec(&duplicate).unwrap(),
    )
    .unwrap();
    assert!(!run(root.path(), &["build"], false).status.success());
    assert_eq!(fs::read_to_string(&article).unwrap(), html);
    let mut broken = doc;
    broken.blocks.clear();
    broken.blocks.push(berlin_document::Block::Paragraph {
        content: vec![berlin_document::Inline::CrossReference {
            target: berlin_document::ComponentId("missing".into()),
            content: vec![berlin_document::Inline::Text {
                value: "Missing".into(),
            }],
        }],
    });
    fs::write(
        root.path().join("paper/document.json"),
        serde_json::to_vec(&broken).unwrap(),
    )
    .unwrap();
    assert!(!run(root.path(), &["build"], false).status.success());
    assert_eq!(fs::read_to_string(&article).unwrap(), html);
}

fn latex_fixture() -> tempfile::TempDir {
    let root = fixture();
    fs::create_dir(root.path().join("papers")).unwrap();
    fs::write(
        root.path().join("papers/paper.tex"),
        include_str!("../../support/fixtures/papers/paper.tex"),
    )
    .unwrap();
    fs::write(
        root.path().join("papers/references.bib"),
        include_str!("../../support/fixtures/papers/references.bib"),
    )
    .unwrap();
    fs::write(
        root.path().join("berlin.pipeline.rhai"),
        r#"
pipeline site {
    let papers = compile_papers(load_latex("papers/*.tex"), "_site/papers");
    let notes = parse_markdown(load_markdown("content/*.md"));
    output render_website(
        assemble_website(merge_documents(notes, papers), parse_feed(load_data("feed.csv"))),
        "_site", website_config(#{url: "https://notebook.example.org"}))
        .deploy_to(github_pages("example/notebook"));
}

"#,
    )
    .unwrap();
    root
}
fn advanced_fixture() -> tempfile::TempDir {
    let root = latex_fixture();
    fs::create_dir(root.path().join("papers/sections")).unwrap();
    for (path, text) in [
        (
            "paper.tex",
            include_str!("../../support/fixtures/papers/advanced/paper.tex"),
        ),
        (
            "notebook.cls",
            include_str!("../../support/fixtures/papers/advanced/notebook.cls"),
        ),
        (
            "sections/model.tex",
            include_str!("../../support/fixtures/papers/advanced/sections/model.tex"),
        ),
        (
            "more.bib",
            include_str!("../../support/fixtures/papers/advanced/more.bib"),
        ),
    ] {
        fs::write(root.path().join("papers").join(path), text).unwrap();
    }
    root
}

#[test]
fn multi_file_preflight_detects_cycles_missing_dependencies_and_unsafe_macros() {
    let root = advanced_fixture();
    success(run(root.path(), &["build", "--dry-run"], false));
    let section = root.path().join("papers/sections/model.tex");
    fs::write(&section, "\\input{paper}").unwrap();
    assert!(
        !run(root.path(), &["build", "--dry-run"], false)
            .status
            .success()
    );
    fs::write(&section, "\\input{missing}").unwrap();
    assert!(
        !run(root.path(), &["build", "--dry-run"], false)
            .status
            .success()
    );
    fs::write(&section, "\\newcommand{\\sneak}{\\input{private}}").unwrap();
    assert!(
        !run(root.path(), &["build", "--dry-run"], false)
            .status
            .success()
    );
}

#[test]
#[ignore = "requires expanded Nix papers shell with TikZ and dvisvgm"]
fn multi_file_class_macros_tikz_and_bibliographies_publish_together() {
    let root = advanced_fixture();
    success(run(
        root.path(),
        &["build", "--allow-tex", "--production"],
        true,
    ));
    let path = root
        .path()
        .join("_site/notes/a-synthetic-dependency-study.html");
    let html = fs::read_to_string(&path).unwrap();
    assert!(html.contains("#eq:vector"));
    assert!(html.contains("#fig:pipeline"));
    assert!(html.contains("class-defined word dependency"));
    assert!(html.contains("A Second Synthetic Reference"));
    assert!(html.contains("directed dependency"));
    assert_eq!(
        fs::read_dir(root.path().join("_site/papers/assets"))
            .unwrap()
            .count(),
        1
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join(".berlin/receipts/site.json")).unwrap())
            .unwrap();
    for name in [
        "papers/sections/model.tex",
        "papers/notebook.cls",
        "papers/more.bib",
    ] {
        assert!(
            receipt["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|input| input["path"] == name)
        );
    }
    let warm = run(root.path(), &["build", "--allow-tex"], true);
    assert!(String::from_utf8_lossy(&warm.stderr).contains("Reusing paper"));
    success(warm);
    let section = root.path().join("papers/sections/model.tex");
    fs::write(
        &section,
        fs::read_to_string(&section)
            .unwrap()
            .replace("This included section", "This edited section"),
    )
    .unwrap();
    let updated = run(root.path(), &["build", "--allow-tex"], true);
    assert!(String::from_utf8_lossy(&updated.stderr).contains("Compiling paper"));
    success(updated);
    assert!(
        fs::read_to_string(path)
            .unwrap()
            .contains("This edited section")
    );
}

#[test]
fn paper_pipeline_requires_permission_but_dry_run_needs_no_tools() {
    let root = latex_fixture();
    let denied = run(root.path(), &["build"], false);
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("--allow-tex"));
    assert!(!root.path().join("_site").exists());
    success(run(root.path(), &["build", "--dry-run"], false));
    assert!(!root.path().join("_site").exists());
    assert!(!run(root.path(), &["check"], false).status.success());
    assert!(!success(run(root.path(), &["--help"], false)).contains("import-paper"));
}

#[test]
#[ignore = "requires Nix papers shell"]
fn standard_classes_includes_and_bibliography_styles_keep_compiled_numbers() {
    for (class, style) in [
        ("article", "plain"),
        ("report", "unsrt"),
        ("book", "abbrv"),
        ("article", "alpha"),
    ] {
        let root = latex_fixture();
        let source = root.path().join("papers/paper.tex");
        let mut text = fs::read_to_string(&source)
            .unwrap()
            .replace("{article}", &format!("{{{class}}}"))
            .replace("{plain}", &format!("{{{style}}}"));
        if class != "article" {
            text = text.replace(
                "\\maketitle",
                "\\maketitle\n\\chapter{Study}\\label{ch:study}\n",
            );
        }
        if class == "book" {
            // Standard book.cls has no abstract environment.
            text = text
                .replace("\\begin{abstract}", "")
                .replace("\\end{abstract}", "");
        }
        text = text.replace("\\end{document}", "\\include{appendix}\n\\end{document}");
        fs::write(
            root.path().join("papers/appendix.tex"),
            "A literal included observation.",
        )
        .unwrap();
        // Select main papers, not the included section itself.
        let config = root.path().join("berlin.pipeline.rhai");
        fs::write(
            &config,
            fs::read_to_string(&config)
                .unwrap()
                .replace("papers/*.tex", "papers/paper.tex"),
        )
        .unwrap();
        fs::write(source, text).unwrap();
        success(run(
            root.path(),
            &["build", "--allow-tex", "--production"],
            true,
        ));
        let html = fs::read_to_string(
            root.path()
                .join("_site/notes/a-small-study-of-incremental-publishing.html"),
        )
        .unwrap();
        assert!(
            html.contains("A literal included observation."),
            "{class}/{style}"
        );
        assert!(html.contains("#eq:work"), "{class}/{style}");
    }
}

#[test]
fn source_metadata_and_bibliography_are_validated_without_execution() {
    let root = latex_fixture();
    let path = root.path().join("papers/paper.tex");
    let original = fs::read_to_string(&path).unwrap();
    for invalid in [
        original.replace("% berlin:", "% berlin: {}\n% berlin:"),
        original.replace(
            "\"published\":\"2026-09-09\"",
            "\"published\":\"yesterday\"",
        ),
        original.replace("\\bibliography{references}", "\\bibliography{missing}"),
        original.replace("\\maketitle", "\\input{private}"),
    ] {
        fs::write(&path, invalid).unwrap();
        assert!(
            !run(root.path(), &["build", "--dry-run"], false)
                .status
                .success()
        );
    }
}

#[test]
#[cfg(unix)]
fn bibliography_cannot_escape_the_source_root() {
    let root = latex_fixture();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("private.bib"), "private").unwrap();
    fs::remove_file(root.path().join("papers/references.bib")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("private.bib"),
        root.path().join("papers/references.bib"),
    )
    .unwrap();
    assert!(
        !run(root.path(), &["build", "--dry-run"], false)
            .status
            .success()
    );
}

fn published_pdf(root: &Path) -> (std::path::PathBuf, Vec<u8>) {
    let files = fs::read_dir(root.join("_site/papers"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(files.len(), 1);
    let path = files[0].path();
    let bytes = fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    (path, bytes)
}

#[test]
#[ignore = "requires Nix papers shell including Poppler and dvisvgm"]
fn raster_and_pdf_figures_are_published_and_invalidate_the_cache() {
    let root = latex_fixture();
    success(run(root.path(), &["build", "--allow-tex"], true));
    let (pdf, bytes) = published_pdf(root.path());
    let image = root.path().join("papers/specimen");
    fs::write(image.with_extension("pdf"), bytes).unwrap();
    success(
        Command::new("pdftoppm")
            .args(["-f", "1", "-singlefile", "-scale-to", "320", "-png"])
            .arg(pdf)
            .arg(&image)
            .output()
            .unwrap(),
    );
    let source = root.path().join("papers/paper.tex");
    let original = fs::read_to_string(&source).unwrap();
    let figures = r"
\begin{figure}
\includegraphics[width=0.4\textwidth]{specimen.png}
\caption{A raster specimen of the synthetic paper.}\label{fig:raster}
\end{figure}
\begin{figure}
\includegraphics[width=0.4\textwidth]{specimen.pdf}
\caption{A vector specimen of the same synthetic paper.}\label{fig:vector}
\end{figure}
See Figures~\ref{fig:raster} and~\ref{fig:vector}.
";
    fs::write(
        &source,
        original
            .replace(
                "\\begin{document}",
                "\\usepackage{graphicx}\n\\begin{document}",
            )
            .replace("\\end{document}", &format!("{figures}\n\\end{{document}}")),
    )
    .unwrap();
    success(run(
        root.path(),
        &["build", "--allow-tex", "--production"],
        true,
    ));
    let html = fs::read_to_string(
        root.path()
            .join("_site/notes/a-small-study-of-incremental-publishing.html"),
    )
    .unwrap();
    assert!(html.contains(".png") && html.contains(".svg"));
    assert!(html.contains("alt=\"A raster specimen"));
    let warm = run(root.path(), &["build", "--allow-tex"], true);
    assert!(String::from_utf8_lossy(&warm.stderr).contains("Reusing paper"));
    success(warm);
    success(
        Command::new("pdftoppm")
            .args(["-f", "1", "-singlefile", "-scale-to", "400", "-png"])
            .arg(image.with_extension("pdf"))
            .arg(&image)
            .output()
            .unwrap(),
    );
    let changed = run(root.path(), &["build", "--allow-tex"], true);
    assert!(String::from_utf8_lossy(&changed.stderr).contains("Compiling paper"));
    success(changed);
}

#[test]
#[ignore = "requires the pinned Nix papers shell without custom TeX search paths"]
fn paper_cache_reuses_verified_results_and_invalidates_dependencies() {
    let root = latex_fixture();
    let build = |extra: &[&str]| {
        let args = [&["build", "--allow-tex"][..], extra].concat();
        let output = run(root.path(), &args, true);
        let log = String::from_utf8_lossy(&output.stderr).into_owned();
        success(output);
        log
    };
    let cold = build(&[]);
    assert!(cold.contains("Compiling paper"), "{cold}");
    assert!(!cold.contains("cache disabled"), "{cold}");
    let (_, pdf) = published_pdf(root.path());
    let article = root
        .path()
        .join("_site/notes/a-small-study-of-incremental-publishing.html");
    let original_html = fs::read(&article).unwrap();
    let warm = build(&[]);
    assert!(warm.contains("Reusing paper"), "{warm}");
    assert!(!warm.contains("Compiling paper"), "{warm}");
    assert_eq!(published_pdf(root.path()).1, pdf);
    assert_eq!(fs::read(&article).unwrap(), original_html);
    assert!(!run(root.path(), &["build"], true).status.success());

    let cached_release = success(run(root.path(), &["release", "--allow-tex"], true));
    let rebuilt_release = success(run(
        root.path(),
        &["release", "--allow-tex", "--no-cache"],
        true,
    ));
    assert_eq!(cached_release, rebuilt_release);

    // Unrelated presentation edits do not recompile the paper.
    let template = root.path().join("pages/notes/[slug].tera");
    let original = fs::read_to_string(&template).unwrap();
    fs::write(
        &template,
        format!("{original}<footer>New presentation</footer>"),
    )
    .unwrap();
    assert!(build(&[]).contains("Reusing paper"));
    assert_eq!(published_pdf(root.path()).1, pdf);
    let prior_html = fs::read(&article).unwrap();
    fs::write(&template, "{% invalid %}").unwrap();
    assert!(
        !run(root.path(), &["build", "--allow-tex"], true)
            .status
            .success()
    );
    assert_eq!(fs::read(&article).unwrap(), prior_html);
    fs::write(&template, original).unwrap();

    // Corruption is a miss and gets replaced only by a successful computation.
    let cache = root.path().join(".berlin/cache/artifacts-v1");
    let entry = fs::read_dir(&cache)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&entry, b"interrupted write").unwrap();
    assert!(build(&[]).contains("Compiling paper"));
    assert!(build(&[]).contains("Reusing paper"));
    let saved = fs::read(&entry).unwrap();
    assert!(build(&["--no-cache"]).contains("Compiling paper"));
    assert_eq!(fs::read(&entry).unwrap(), saved);

    let bib = root.path().join("papers/references.bib");
    fs::write(
        &bib,
        fs::read_to_string(&bib)
            .unwrap()
            .replace("not a real publication", "an edited synthetic citation"),
    )
    .unwrap();
    assert!(build(&[]).contains("Compiling paper"));
    assert_ne!(published_pdf(root.path()).1, pdf);
    assert!(build(&[]).contains("Reusing paper"));

    // Each paper has its own key, rather than a hash of the entire collection.
    let paper = root.path().join("papers/paper.tex");
    let text = fs::read_to_string(&paper).unwrap();
    fs::write(
        root.path().join("papers/second.tex"),
        text.replace("synthetic-paper", "second-paper")
            .replace("A Small Study of Incremental Publishing", "A Second Paper"),
    )
    .unwrap();
    let added = build(&[]);
    assert_eq!(added.matches("Compiling paper").count(), 1, "{added}");
    assert_eq!(added.matches("Reusing paper").count(), 1, "{added}");
    fs::write(
        &paper,
        text.replace("This synthetic paper", "This edited paper"),
    )
    .unwrap();
    let edited = build(&[]);
    assert_eq!(edited.matches("Compiling paper").count(), 1, "{edited}");
    assert_eq!(edited.matches("Reusing paper").count(), 1, "{edited}");

    fs::write(
        root.path().join("papers/third.tex"),
        r"\documentclass{article}
\usepackage{hyperref}\title{No Bibliography}\author{Author}\date{2026-09-09}
\begin{document}\maketitle\section{Note}\label{sec:note}Standalone text.\end{document}",
    )
    .unwrap();
    let mixed = build(&[]);
    assert_eq!(mixed.matches("Compiling paper").count(), 1, "{mixed}");
    assert_eq!(mixed.matches("Reusing paper").count(), 2, "{mixed}");
    fs::write(
        root.path().join("papers/fourth.tex"),
        text.replace("synthetic-paper", "fourth-paper")
            .replace("A Small Study of Incremental Publishing", "A Fourth Paper"),
    )
    .unwrap();
    let mixed = build(&[]);
    assert_eq!(mixed.matches("Compiling paper").count(), 1, "{mixed}");
    assert_eq!(mixed.matches("Reusing paper").count(), 3, "{mixed}");
}

#[test]
#[ignore = "requires the optional Nix papers shell"]
fn pipeline_compiles_rebuilds_and_rolls_back_papers_with_the_website() {
    let root = latex_fixture();
    let source_path = root.path().join("papers/paper.tex");
    let source = fs::read_to_string(&source_path).unwrap();
    success(run(
        root.path(),
        &["build", "--allow-tex", "--production"],
        true,
    ));
    let article = root
        .path()
        .join("_site/notes/a-small-study-of-incremental-publishing.html");
    let html = fs::read_to_string(&article).unwrap();
    assert!(html.contains("#eq:work"));
    assert!(html.contains("#ref-example"));
    assert!(html.contains("Test fixture; not a real publication."));
    assert!(root.path().join("_site/notes/ordinary-note.html").exists());
    let (_, pdf) = published_pdf(root.path());
    assert_eq!(fs::read_to_string(&source_path).unwrap(), source);
    for extension in ["aux", "log", "pdf", "bbl"] {
        assert!(!source_path.with_extension(extension).exists());
    }

    let bibliography = root.path().join("papers/references.bib");
    let bib = fs::read_to_string(&bibliography).unwrap();
    fs::write(
        &bibliography,
        bib.replace(
            "Test fixture; not a real publication",
            "Updated synthetic bibliography",
        ),
    )
    .unwrap();
    success(run(
        root.path(),
        &["build", "--allow-tex", "--production"],
        true,
    ));
    let updated_html = fs::read_to_string(&article).unwrap();
    assert!(updated_html.contains("Updated synthetic bibliography."));
    let (pdf_path, updated_pdf) = published_pdf(root.path());
    assert_ne!(updated_pdf, pdf);

    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join(".berlin/receipts/site.json")).unwrap())
            .unwrap();
    let inputs = receipt["inputs"].as_array().unwrap();
    assert!(inputs.iter().any(
        |input| input["path"] == "papers/paper.tex" && input["content_id"] == "synthetic-paper"
    ));
    assert!(
        inputs
            .iter()
            .any(|input| input["path"] == "papers/references.bib")
    );

    let edited = source.replace(
        "This synthetic paper tests conversion fidelity.",
        "This edited paper tests conversion fidelity.",
    );
    fs::write(&source_path, &edited).unwrap();
    let release = success(run(
        root.path(),
        &["release", "--allow-tex", "--production"],
        true,
    ));
    assert!(!release.trim().is_empty());
    assert_eq!(fs::read_to_string(&article).unwrap(), updated_html);
    assert_eq!(fs::read(&pdf_path).unwrap(), updated_pdf);
    success(run(root.path(), &["build", "--allow-tex"], true));
    let updated_html = fs::read_to_string(&article).unwrap();
    assert!(updated_html.contains("This edited paper"));
    let (pdf_path, updated_pdf) = published_pdf(root.path());

    fs::write(
        &source_path,
        source.replace("\\maketitle", "\\undefinedcommand"),
    )
    .unwrap();
    assert!(
        !run(root.path(), &["build", "--allow-tex"], true)
            .status
            .success()
    );
    assert_eq!(fs::read_to_string(&article).unwrap(), updated_html);
    assert_eq!(fs::read(pdf_path).unwrap(), updated_pdf);
}

#[test]
#[ignore = "requires the optional Nix papers shell"]
fn iso_date_without_metadata_and_custom_pdf_destination_work() {
    let root = latex_fixture();
    fs::write(
        root.path().join("papers/paper.tex"),
        r"\documentclass{article}
\usepackage{hyperref}
\title{A Minimal Paper}\author{Example Author}\date{2026-09-09}
\begin{document}\maketitle
\section{A note}\label{sec:note}
This synthetic paper has no bibliography.
\end{document}",
    )
    .unwrap();
    let config = root.path().join("berlin.pipeline.rhai");
    let script = fs::read_to_string(&config)
        .unwrap()
        .replace("\"_site/papers\"", "\"public/downloads\", \"/downloads\"")
        .replace("\"_site\"", "\"public\"");
    fs::write(config, script).unwrap();
    success(run(
        root.path(),
        &["build", "--allow-tex", "--production"],
        true,
    ));
    let html = fs::read_to_string(root.path().join("public/notes/a-minimal-paper.html")).unwrap();
    assert!(html.contains("/downloads/"));
    assert_eq!(
        fs::read_dir(root.path().join("public/downloads"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
#[ignore = "requires native filesystem watching, local HTTP binding and the Nix papers shell"]
fn watch_rebuilds_saved_sources_and_bibliographies_without_feedback_loops() {
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
    let root = latex_fixture();
    // Deliberately watch the project root: receipts, staging and publication
    // must not recursively trigger another build.
    fs::rename(
        root.path().join("papers/paper.tex"),
        root.path().join("paper.tex"),
    )
    .unwrap();
    fs::rename(
        root.path().join("papers/references.bib"),
        root.path().join("references.bib"),
    )
    .unwrap();
    let config = root.path().join("berlin.pipeline.rhai");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("papers/*.tex", "*.tex"),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let log_directory = tempfile::tempdir().unwrap();
    let log_path = log_directory.path().join("server.log");
    let log = fs::File::create(&log_path).unwrap();
    let _server = Server(
        Command::new(env!("CARGO_BIN_EXE_bln"))
            .current_dir(root.path())
            .env("BERLIN_DIR", root.path())
            .args([
                "serve",
                "--watch",
                "--allow-tex",
                "--port",
                &port.to_string(),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let wait_for = |needle: &str| {
        let deadline = Instant::now() + Duration::from_secs(40);
        loop {
            if let Ok(mut stream) = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)) {
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let _ = stream.write_all(b"GET /notes/a-small-study-of-incremental-publishing.html HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
                let mut response = String::new();
                if stream.read_to_string(&mut response).is_ok() && response.contains(needle) {
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "waiting for {needle}: {}",
                fs::read_to_string(&log_path).unwrap()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    let settled = || {
        // Let any in-flight native notification arrive before measuring.
        std::thread::sleep(Duration::from_secs(3));
        let path = root.path().join(".berlin/receipts/site.json");
        let receipt = fs::read(&path).unwrap();
        std::thread::sleep(Duration::from_secs(4));
        assert!(
            fs::read(path).unwrap() == receipt,
            "build fed back into watcher: {}",
            fs::read_to_string(&log_path).unwrap()
        );
    };
    wait_for("Test fixture; not a real publication.");
    settled();
    let source = root.path().join("paper.tex");
    fs::write(
        &source,
        fs::read_to_string(&source).unwrap().replace(
            "This synthetic paper tests conversion fidelity.",
            "A saved edit triggered this rebuild.",
        ),
    )
    .unwrap();
    wait_for("A saved edit triggered this rebuild.");
    let bibliography = root.path().join("references.bib");
    fs::write(
        &bibliography,
        fs::read_to_string(&bibliography).unwrap().replace(
            "Test fixture; not a real publication",
            "A saved bibliography edit",
        ),
    )
    .unwrap();
    wait_for("A saved bibliography edit.");
    settled();
}
