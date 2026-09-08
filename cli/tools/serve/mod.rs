use std::net::IpAddr;
use std::net::Ipv4Addr;

use anyhow::Error;
use berlin_core::{Operation, PipelineLoader as _};

use crate::args::ServeFlags;
use crate::project::Project;
use crate::tasks::run_pipeline;
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
    if serve_flags.watch {
        return serve_with_watch(project, serve_flags.port).await;
    }

    // Regular serve mode - build once and serve
    let target = website_output(&project)?;
    run_pipeline(&project, "site")?;
    let ip_addr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let port = serve_flags.port;

    println!("Server running on http://{}:{}", ip_addr, port);
    start_server(target, ip_addr, port, None).await;

    Ok(())
}

async fn serve_with_watch(project: Project, port: u16) -> Result<(), Error> {
    let target = website_output(&project)?;
    let watched_output = target.clone();
    let ip_addr = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let revision = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as u64,
    ));
    let build_revision = revision.clone();
    let build_project = project.clone();
    let watcher = crate::util::file_watcher::watch_recv(
        move || watch_paths(&project),
        PrintConfig::new("Watcher", "Build", true),
        move |_changed_paths| {
            let project = build_project.clone();
            let revision = build_revision.clone();
            let expected_output = watched_output.clone();
            Ok(async move {
                tokio::task::spawn_blocking(move || {
                    if website_output(&project)? != expected_output {
                        anyhow::bail!(
                            "Website output changed; restart serve to use the new directory"
                        );
                    }
                    run_pipeline(&project, "site")?;
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
fn website_output(project: &Project) -> Result<std::path::PathBuf, Error> {
    let root = project.root();
    let plan = crate::pipeline::load_pipeline_program(project)?
        .load("site")
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    plan.validate()?;
    let websites = plan
        .nodes()
        .iter()
        .filter(|node| matches!(node.operation, Operation::RenderWebsite { .. }))
        .collect::<Vec<_>>();
    let [website] = websites.as_slice() else {
        anyhow::bail!(
            "serve requires exactly one website renderer in the site pipeline; found {}",
            websites.len()
        );
    };
    let output = website
        .output_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Website renderer has no output directory"))?;
    Ok(root.join(output))
}

fn watch_paths(project: &Project) -> Result<Vec<std::path::PathBuf>, Error> {
    let root = project.root();
    let mut paths = WATCH_PATHS
        .iter()
        .map(|path| root.join(path))
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
    paths.extend(project.pipeline_files().iter().cloned());
    let program = crate::pipeline::load_pipeline_program(project)?;
    for directory in program.source_roots().values() {
        paths.push(root.join(directory).canonicalize()?);
    }
    let plan = program
        .load("site")
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    for node in plan.nodes() {
        if let Operation::RenderWebsite { config } = &node.operation
            && let Some(selected) = &config.theme
        {
            paths.push(crate::tasks::theme::theme_root(root, selected)?);
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipeline(output: &str) -> String {
        format!(
            r#"pipeline site {{
            let documents = parse_markdown(load_markdown("content/*.md"));
            let feed = parse_feed(load_data("data/*.csv"));
            let website = assemble_website(documents, feed);
            let config = website_config(#{{url: "http://localhost:8081", title: "Example", author: "Author", description: "Test"}});
            output render_website(website, "{output}", config);
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
    fn preview_does_not_guess_between_multiple_websites() {
        let root = tempfile::tempdir().unwrap();
        let source = pipeline("one").replace(
            "output render_website(website, \"one\", config);",
            "output render_website(website, \"one\", config).named(\"first\"); output render_website(website, \"two\", config).named(\"second\");",
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
