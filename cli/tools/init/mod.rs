use crate::args::InitFlags;
use crate::colors;
use errors::anyhow::{Context, Error};
use log::info;
use std::io::Write;
use std::path::Path;

fn create_file(dir: &Path, filename: &str, content: &str) -> Result<(), Error> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(filename))
        .with_context(|| format!("Failed to create {} file", filename))?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

pub async fn init_project(init_flags: InitFlags) -> Result<(), Error> {
    let cwd = std::env::current_dir().context("Can't read current working directory.")?;
    let dir = if let Some(dir) = &init_flags.dir {
        let dir = cwd.join(dir);
        std::fs::create_dir_all(&dir)?;
        dir
    } else {
        cwd
    };

    create_file(&dir, "berlin.toml", include_str!("./templates/berlin.toml"))?;

    for d in vec![
        &dir.join("layouts").join("partials"),
        &dir.join("static").join("css"),
        &dir.join("static").join("js"),
        &dir.join("sass"),
    ] {
        std::fs::create_dir_all(&d)?;
    }

    create_file(
        &dir.join("layouts"),
        "default.html",
        include_str!("./templates/layouts/default.html"),
    )?;

    create_file(
        &dir.join("layouts").join("partials"),
        "head.html",
        include_str!("./templates/layouts/partials/head.html"),
    )?;

    create_file(
        &dir.join("sass"),
        "main.scss",
        include_str!("./templates/sass/main.scss"),
    )?;

    create_file(
        &dir.join("static").join("css"),
        "framework.min.css",
        include_str!("./templates/static/css/framework.min.css"),
    )?;

    create_file(
        &dir.join("static").join("js"),
        "main.js",
        include_str!("./templates/static/js/main.js"),
    )?;

    info!("✅ {}", colors::green("Project initialized"));
    info!("");
    info!("{}", colors::gray("Run these commands to get started"));
    info!("");
    if let Some(dir) = init_flags.dir {
        info!("  cd {}", dir);
        info!("");
    }
    info!("  {}", colors::gray("# Run the program"));
    info!("  bln build berlin.toml");
    info!("");
    info!(
        "  {}",
        colors::gray("# Build the website, start a server and watch for file changes")
    );
    info!("  bln dev");
    info!("");
    info!("  {}", colors::gray("# Start the server"));
    info!("  bln serve");
    Ok(())
}
