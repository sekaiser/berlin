use std::net::IpAddr;
use std::net::Ipv4Addr;

use anyhow::Error;
use berlin_core::{Operation, PipelineLoader as _};

use crate::args::ServeFlags;
use crate::project::Project;
use crate::tasks::{ExecutionOptions, run_pipeline_with_options};
use crate::util::file_watcher::PrintConfig;

mod preview;

const WATCH_PATHS: [&str; 7] = [
    "pages",
    "content",
    ".berlin/generated",
    "css",
    "styles",
    "static",
    "data",
];

async fn start_server(
    dir: std::path::PathBuf,
    ip_addr: IpAddr,
    port: u16,
    revision: Option<preview::Revision>,
) {
    println!("Starting server at http://localhost:{port}");
    warp::serve(preview::routes(dir, revision))
        .run(std::net::SocketAddr::new(ip_addr, port))
        .await;
}

pub async fn serve(
    serve_flags: ServeFlags,
    pipeline_files: Vec<std::path::PathBuf>,
) -> Result<(), Error> {
    let project = Project::load(pipeline_files)?;
    let options = ExecutionOptions {
        allow_tex: serve_flags.allow_tex,
        no_cache: serve_flags.no_cache,
        ..Default::default()
    };
    if serve_flags.watch {
        return serve_with_watch(project, serve_flags.pipeline, serve_flags.port, options).await;
    }

    // Regular serve mode - build once and serve
    let target = website_output_for(&project, &serve_flags.pipeline)?;
    run_pipeline_with_options(&project, &serve_flags.pipeline, options)?;
    let ip_addr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let port = serve_flags.port;

    println!("Server running on http://{}:{}", ip_addr, port);
    start_server(target, ip_addr, port, None).await;

    Ok(())
}

async fn serve_with_watch(
    project: Project,
    pipeline: String,
    port: u16,
    options: ExecutionOptions,
) -> Result<(), Error> {
    let target = website_output_for(&project, &pipeline)?;
    let watched_output = target.clone();
    let ip_addr = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let revision = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as u64,
    ));
    let build_revision = revision.clone();
    let build_project = project.clone();
    let ignored_root = project.root().to_owned();
    let canonical_root = ignored_root.canonicalize()?;
    let ignored_output = watched_output.clone();
    let canonical_output = canonical_root.join(watched_output.strip_prefix(project.root())?);
    let watch_pipeline = pipeline.clone();
    let watcher = crate::util::file_watcher::watch_recv(
        move || watch_paths_for(&project, &watch_pipeline),
        PrintConfig::new("Watcher", "Build", true),
        move |path| {
            ignored_watch_path(path, &ignored_root, &ignored_output)
                || ignored_watch_path(path, &canonical_root, &canonical_output)
        },
        move |_changed_paths| {
            let project = build_project.clone();
            let revision = build_revision.clone();
            let expected_output = watched_output.clone();
            let pipeline = pipeline.clone();
            Ok(async move {
                tokio::task::spawn_blocking(move || {
                    if website_output_for(&project, &pipeline)? != expected_output {
                        anyhow::bail!(
                            "Website output changed; restart serve to use the new directory"
                        );
                    }
                    run_pipeline_with_options(&project, &pipeline, options)?;
                    revision.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Ok::<_, Error>(())
                })
                .await?
            })
        },
    );

    println!("Server running on http://{}:{}", ip_addr, port);
    tokio::select! {
        result = watcher => result?,
        _ = start_server(target, ip_addr, port, Some(revision)) => {}
    }

    Ok(())
}

/// Use the renderer's declared destination rather than a separate server setting.
fn website_output_for(project: &Project, pipeline: &str) -> Result<std::path::PathBuf, Error> {
    let root = project.root();
    let plan = crate::pipeline::load_pipeline_program(project)?
        .load(pipeline)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    plan.validate()?;
    let websites = plan
        .nodes()
        .iter()
        .filter(|node| node.operation.website_config().is_some())
        .collect::<Vec<_>>();
    let [website] = websites.as_slice() else {
        anyhow::bail!(
            "serve requires exactly one website renderer in pipeline '{pipeline}'; found {}",
            websites.len()
        );
    };
    let output = website
        .output_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Website renderer has no output directory"))?;
    Ok(root.join(output))
}

#[cfg(test)]
fn website_output(project: &Project) -> Result<std::path::PathBuf, Error> {
    website_output_for(project, "site")
}
#[cfg(test)]
fn watch_paths(project: &Project) -> Result<Vec<std::path::PathBuf>, Error> {
    watch_paths_for(project, "site")
}
fn watch_paths_for(project: &Project, pipeline: &str) -> Result<Vec<std::path::PathBuf>, Error> {
    let root = project.root();
    let mut paths = WATCH_PATHS
        .iter()
        .map(|path| root.join(path))
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
    paths.extend(project.pipeline_files().iter().cloned());
    // Observe creation of project override directories, not only existing files.
    // Generated roots remain excluded by ignored_watch_path.
    paths.push(root.to_owned());
    let program = crate::pipeline::load_pipeline_program(project)?;
    let captured_project = project.with_captured_source_roots(program.source_roots());
    let project = &captured_project;
    for directory in program.source_roots().values() {
        paths.push(root.join(directory).canonicalize()?);
    }
    let plan = program
        .load(pipeline)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    for node in plan.nodes() {
        if let Operation::LoadMarkdown { pattern }
        | Operation::LoadOrg { pattern }
        | Operation::LoadLatex { pattern }
        | Operation::LoadData { pattern }
        | Operation::LoadAssets { pattern } = &node.operation
        {
            let (source_root, relative) = crate::util::fs::source_location(project, pattern)?;
            let base = crate::tasks::pattern_base(
                &source_root,
                relative
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("LaTeX pattern is not UTF-8"))?,
            );
            // Watching the parent handles atomic editor saves and new .bib files.
            let existing = base
                .ancestors()
                .find(|path| path.is_dir())
                .ok_or_else(|| anyhow::anyhow!("LaTeX watch path has no existing parent"))?;
            paths.push(existing.canonicalize()?);
            if matches!(node.operation, Operation::LoadLatex { .. }) {
                paths.extend(crate::tasks::paper::dependency_directories(
                    project, pattern,
                ));
            }
        }
        if let Some(config) = node.operation.website_config()
            && let Some(selected) = &config.theme
        {
            paths.push(crate::tasks::theme::theme_root(root, selected)?);
        }
    }
    Ok(paths)
}

fn ignored_watch_path(
    path: &std::path::Path,
    root: &std::path::Path,
    output: &std::path::Path,
) -> bool {
    // Directory notifications for the root also accompany staged-output swaps.
    // Actual input additions/removals carry their own descendant paths.
    if path == root || path.starts_with(output) {
        return true;
    }
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let mut components = relative.components();
    let Some(first) = components.next().and_then(|part| part.as_os_str().to_str()) else {
        return false;
    };
    match first {
        ".git" => true,
        ".berlin" => components
            .next()
            .is_none_or(|part| part.as_os_str() != "generated"),
        name => {
            name.starts_with(".berlin-stage-")
                || name.starts_with(".berlin-backup-")
                || name.starts_with(".berlin-journal-")
                || name == ".berlin-transaction.json"
                || name == ".berlin.lock"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watcher_ignores_output_and_state_but_keeps_org_exports() {
        let root = std::path::Path::new("/project");
        let output = root.join("public");
        for path in [
            "public/papers/new.pdf",
            ".berlin-stage-abc/papers/new.pdf",
            ".berlin-backup-abc",
            ".berlin-journal-abc",
            ".berlin/receipts/site.json",
            ".berlin/state.sqlite",
            ".berlin/state.sqlite-wal",
            ".berlin/state.sqlite-shm",
            ".git/index",
            ".berlin-transaction.json",
            ".berlin.lock",
        ] {
            assert!(
                ignored_watch_path(&root.join(path), root, &output),
                "{path}"
            );
        }
        for path in [
            "papers/paper.tex",
            "papers/references.bib",
            "berlin.pipeline.rhai",
            ".berlin/generated/org/content/paper.md",
        ] {
            assert!(
                !ignored_watch_path(&root.join(path), root, &output),
                "{path}"
            );
        }
    }

    #[test]
    fn watcher_discovers_custom_latex_directories_and_missing_parents() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("research")).unwrap();
        for pattern in ["research/*.tex", "research/future/*.tex"] {
            let script = pipeline("_site").replace(
                "documents(load_markdown(\"content/*.md\"))",
                &format!("compile_papers(load_latex({pattern:?}), \"_site/papers\")"),
            );
            std::fs::write(root.path().join("berlin.pipeline.rhai"), script).unwrap();
            let project = Project::new(root.path().to_owned(), vec![]);
            assert!(
                watch_paths(&project)
                    .unwrap()
                    .contains(&root.path().join("research").canonicalize().unwrap())
            );
        }
    }

    fn pipeline(output: &str) -> String {
        format!(
            r#"fn notebook(c) {{throw "not executed while selecting preview";}} pipeline site {{
            let sources = catalog([documents(load_markdown("content/*.md"))]);
            let website = compose(sources,Fn("notebook"));
            let config = html_config(#{{url: "http://localhost:8081", title: "Example", author: "Author", description: "Test"}});
            output render_html(website, "{output}", config);
        }}"#
        )
    }

    #[test]
    fn preview_and_watch_use_explicit_files_including_external_files() {
        let root = tempfile::tempdir().unwrap();
        let scripts = tempfile::tempdir().unwrap();
        let shared = scripts.path().join("shared.rhai");
        let site = root.path().join("site.rhai");
        std::fs::write(&shared, "const DESTINATION = \"public\";").unwrap();
        std::fs::write(
            &site,
            pipeline("public").replace("\"public\", config", "DESTINATION, config"),
        )
        .unwrap();
        std::fs::write(root.path().join("berlin.pipeline.rhai"), "invalid default").unwrap();
        let project = Project::new(
            root.path().to_owned(),
            vec![shared.clone(), "site.rhai".into()],
        );
        assert_eq!(
            website_output(&project).unwrap(),
            root.path().join("public")
        );
        let watched = watch_paths(&project).unwrap();
        assert!(watched.contains(&shared));
        assert!(watched.contains(&site));
        assert!(!watched.contains(&root.path().join("berlin.pipeline.rhai")));
    }

    #[test]
    fn preview_uses_the_declared_website_output() {
        let root = tempfile::tempdir().unwrap();
        for output in ["_site", "dist", "build/public"] {
            std::fs::write(root.path().join("berlin.pipeline.rhai"), pipeline(output)).unwrap();
            assert_eq!(
                website_output(&Project::new(root.path().to_owned(), vec![])).unwrap(),
                root.path().join(output)
            );
        }
    }

    #[test]
    fn preview_rejects_unsafe_or_missing_website_outputs() {
        let root = tempfile::tempdir().unwrap();
        for source in [pipeline("../outside"), "pipeline site {}".into()] {
            std::fs::write(root.path().join("berlin.pipeline.rhai"), source).unwrap();
            assert!(website_output(&Project::new(root.path().to_owned(), vec![])).is_err());
        }
    }

    #[test]
    fn f07_composed_preview_rejects_text_and_ambiguous_targets() {
        let root = tempfile::tempdir().unwrap();
        let base = r#"fn compose(c){throw "not executed by preview selection";}
            pipeline portfolio {let p=compose(catalog([documents(load_markdown("missing/*.md"))]),Fn("compose")); OUTPUT }"#;
        for output in [
            "output render_text(p,\"_text\",text_config(#{}));",
            "output render_html(p,\"one\",html_config(#{}));output render_html(p,\"two\",html_config(#{}));",
        ] {
            std::fs::write(
                root.path().join("berlin.pipeline.rhai"),
                base.replace("OUTPUT", output),
            )
            .unwrap();
            assert!(
                website_output_for(&Project::new(root.path().to_owned(), vec![]), "portfolio")
                    .is_err()
            );
        }
    }

    #[test]
    fn preview_does_not_guess_between_multiple_websites() {
        let root = tempfile::tempdir().unwrap();
        let source = pipeline("one").replace(
            "output render_html(website, \"one\", config);",
            "output render_html(website, \"one\", config).named(\"first\"); output render_html(website, \"two\", config).named(\"second\");",
        );
        std::fs::write(root.path().join("berlin.pipeline.rhai"), source).unwrap();
        assert!(
            website_output(&Project::new(root.path().to_owned(), vec![]))
                .unwrap_err()
                .to_string()
                .contains("found 2")
        );
    }
}
