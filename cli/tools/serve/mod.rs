use crate::args::Flags;
use crate::args::ServeFlags;
use crate::proc_state::ProcState;
use crate::site_generator::create_main_site_generator;
use crate::util;
use berlin_runtime::tokio_util::start_server;
use files::ModuleSpecifier;
use libs::anyhow::Error;
use libs::tokio;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::sync::Arc;

pub async fn serve(flags: Flags, serve_flags: ServeFlags) -> Result<(), Error> {
    let ps = ProcState::build(flags.clone()).await?;
    let bln_dir = &ps.dir;
    let ip_addr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let port = 8081;

    //if flags.watch.is_some() {
    tokio::spawn(async move { run_with_watch(flags).await });
    //}

    start_server(
        bln_dir.target_file_path().display().to_string(),
        ip_addr,
        port,
    )
    .await;

    Ok(())
}

async fn run_with_watch(flags: Flags) -> Result<i32, Error> {
    let flags = Arc::new(flags);
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    let ps = ProcState::build_for_file_watcher((*flags).clone(), sender.clone()).await?;

    let operation = |specifier: ModuleSpecifier| {
        let ps = ps.clone();
        Ok(async move {
            let site_generator = create_main_site_generator(&ps)?;

            let _ = site_generator.watch(specifier)?;

            // let orig_media_type = MediaType::from(Path::new(specifier.path()));
            // match orig_media_type {
            //     MediaType::Css => {
            //         if let Some(css_resolutions) = &ps.maybe_css_resolutions {
            //             let root_css_files =
            //                 css_resolutions.get_root(PathBuf::from(specifier.path()));

            //             let specifiers: Vec<ModuleSpecifier> = root_css_files
            //                 .into_iter()
            //                 .map(|p| ModuleSpecifier::from_file_path(p).unwrap())
            //                 .collect();

            //             for specifier in specifiers {
            //                 ps.parsed_source_cache.free(&specifier);
            //                 let _ = site_generator.run(specifier)?;
            //             }
            //         }
            //     }
            //     MediaType::Tera => {
            //         let _ = ps.hera.lock().full_reload();
            //         let mut site_generator = create_main_site_generator(&ps)?;
            //         for f in load_files(&&ps.dir.content_file_path(), "**/*.*") {
            //             let specifier = ModuleSpecifier::from_file_path(f).expect("Invalid path.");
            //             let _ = site_generator.run(specifier).unwrap();
            //         }
            //     }
            //     _ => {
            //         ps.parsed_source_cache.free(&specifier);
            //         let _ = site_generator.run(specifier)?;
            //     }
            // };

            Ok(())
        })
    };

    util::file_watcher::watch_func2(
        receiver,
        operation,
        util::file_watcher::PrintConfig {
            job_name: "Compile".to_string(),
            clear_screen: true,
        },
    )
    .await?;

    Ok(0)
}
