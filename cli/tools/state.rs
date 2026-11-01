use crate::{args::state::Action, tasks::state};
use anyhow::Error;
use std::path::PathBuf;
pub fn run(action: Action, files: Vec<PathBuf>) -> Result<(), Error> {
    let project = super::release::sealed_project(files)?;
    match action {
        Action::Migrate => state::migrate(&project)?,
        Action::Status => {}
        Action::ReleaseAttempts { pipeline, limit } => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &state::Store::open(project.root())?
                        .release_attempts(pipeline.as_deref(), limit)?
                )?
            );
            return Ok(());
        }
        Action::ReleaseAttempt(id) => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &state::Store::open(project.root())?.release_attempt(id)?
                )?
            );
            return Ok(());
        }
        Action::MetricDefinitions { limit } => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &state::Store::open(project.root())?.metric_definitions(limit)?
                )?
            );
            return Ok(());
        }
        Action::Metrics { document, limit } => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &state::Store::open(project.root())?.metrics(document.as_deref(), limit)?
                )?
            );
            return Ok(());
        }
        Action::IndexReleases => state::documents::index(&project)?,
        Action::Documents { limit } => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &state::Store::open(project.root())?.documents(limit)?
                )?
            );
            return Ok(());
        }
        Action::Document { id, limit } => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &state::Store::open(project.root())?.document(&id, limit)?
                )?
            );
            return Ok(());
        }
        Action::Backup(path) => state::backup(&project, &project.root().join(path))?,
        Action::Restore(path) => state::restore(&project, &project.root().join(path))?,
        Action::Builds { pipeline, limit } => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &state::Store::open(project.root())?.builds(pipeline.as_deref(), limit)?
                )?
            );
            return Ok(());
        }
        Action::Build(id) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&state::Store::open(project.root())?.build(id)?)?
            );
            return Ok(());
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&state::status(&project)?)?
    );
    Ok(())
}
