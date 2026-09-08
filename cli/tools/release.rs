//! CLI presentation for sealed releases and explicit publication.
use crate::{
    args::{PublicationFlags, PublishFlags, ReleaseFlags},
    project::Project,
    tasks,
};
use anyhow::{Error, bail};
use std::path::PathBuf;

pub fn prepare(flags: ReleaseFlags, files: Vec<PathBuf>) -> Result<(), Error> {
    let project = Project::load(files)?;
    let release = match flags.from_directory {
        Some(directory) => tasks::release::from_directory(&project, &flags.pipeline, &directory)?,
        None => tasks::prepare_release(&project, &flags.pipeline)?,
    };
    eprintln!(
        "Sealed {} files for {}. Nothing published.",
        release.manifest.files.len(),
        release.manifest.url
    );
    println!("{}", release.id);
    Ok(())
}

fn sealed_project(files: Vec<PathBuf>) -> Result<Project, Error> {
    if !files.is_empty() {
        bail!(
            "sealed release operations do not accept pipeline files; the release already binds its destination"
        );
    }
    Project::load(vec![])
}

pub fn plan(id: &str, files: Vec<PathBuf>) -> Result<(), Error> {
    let project = sealed_project(files)?;
    let release = tasks::release::open(&project, id)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "release": release.id,
            "manifest": release.manifest,
            "site_directory": release.site(),
            "effect": "Replace the complete gh-pages branch tree, preserving history; never change the default branch or Pages settings",
        }))?
    );
    Ok(())
}

pub fn publish(flags: PublishFlags, files: Vec<PathBuf>) -> Result<(), Error> {
    let project = sealed_project(files)?;
    let record = tasks::publication::publish(&project, &flags.release, &flags.confirm)?;
    report(record)
}

pub fn inspect(flags: PublicationFlags, files: Vec<PathBuf>) -> Result<(), Error> {
    let project = sealed_project(files)?;
    let record = tasks::publication::inspect(&project, &flags.release, flags.refresh)?;
    report(record)
}

fn report(record: tasks::publication::Publication) -> Result<(), Error> {
    println!("{}", serde_json::to_string_pretty(&record)?);
    if matches!(
        record.status,
        tasks::publication::Status::Failed
            | tasks::publication::Status::Conflict
            | tasks::publication::Status::Unknown
    ) {
        bail!("publication needs attention; inspect its recorded state before proceeding");
    }
    Ok(())
}
