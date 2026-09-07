use std::net::IpAddr;
use std::net::Ipv4Addr;

use anyhow::Error;

use crate::args::ServeFlags;
use crate::project::Project;
use crate::tasks::run_pipeline;
use crate::util::file_watcher::PrintConfig;

const WATCH_PATHS: [&str; 6] = [
    "pages",
    "content",
    "css",
    "static",
    "data",
    "berlin.pipeline.rhai",
];

async fn start_server(dir: String, ip_addr: IpAddr, port: u16) {
    println!("Starting server at http://localhost:{port}");
    warp::serve(warp::fs::dir(dir))
        .run(std::net::SocketAddr::new(ip_addr, port))
        .await;
}

pub async fn serve(serve_flags: ServeFlags) -> Result<(), Error> {
    if serve_flags.watch {
        return serve_with_watch(serve_flags.port).await;
    }

    // Regular serve mode - build once and serve
    let project = Project::load()?;
    run_pipeline(&project, "site")?;

    let target = project.root().join("_site");
    let ip_addr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let port = serve_flags.port;

    println!("Server running on http://{}:{}", ip_addr, port);
    start_server(target.display().to_string(), ip_addr, port).await;

    Ok(())
}

async fn serve_with_watch(port: u16) -> Result<(), Error> {
    let project = Project::load()?;
    let target = project.root().join("_site");
    let watch_paths = WATCH_PATHS
        .iter()
        .map(|path| project.root().join(path))
        .collect();
    let ip_addr = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let watcher = crate::util::file_watcher::watch_recv(
        watch_paths,
        PrintConfig::new("Watcher", "Build", true),
        move |_changed_paths| {
            Ok(async move {
                tokio::task::spawn_blocking(move || {
                    let project = Project::load()?;
                    run_pipeline(&project, "site")
                })
                .await?
            })
        },
    );

    println!("Server running on http://{}:{}", ip_addr, port);
    tokio::select! {
        result = watcher => result?,
        _ = start_server(target.display().to_string(), ip_addr, port) => {}
    }

    Ok(())
}
