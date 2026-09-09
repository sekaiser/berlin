//! CLI presentation for sealed releases and explicit publication.
use crate::{
    args::{PublicationFlags, PublishFlags, ReleaseDiffFlags, ReleaseFlags},
    project::Project,
    tasks,
};
use anyhow::{Error, bail};
use std::path::PathBuf;
use tasks::release::diff::ReleaseComparison;
use tasks::release::documents::DocumentComparison;

pub fn prepare(flags: ReleaseFlags, files: Vec<PathBuf>) -> Result<(), Error> {
    let project = Project::load(files)?;
    let release = match flags.from_directory {
        Some(directory) => tasks::release::from_directory(&project, &flags.pipeline, &directory)?,
        None => tasks::prepare_release(
            &project,
            &flags.pipeline,
            tasks::ExecutionOptions {
                production: flags.production,
                allow_tex: flags.allow_tex,
                no_cache: flags.no_cache,
                ..Default::default()
            },
        )?,
    };
    eprintln!(
        "Sealed {} files for {}. Nothing published.",
        release.manifest.files.len(),
        release.manifest.url
    );
    println!("{}", release.id);
    Ok(())
}

pub(super) fn sealed_project(files: Vec<PathBuf>) -> Result<Project, Error> {
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

pub fn diff(flags: ReleaseDiffFlags, files: Vec<PathBuf>) -> Result<(), Error> {
    let project = sealed_project(files)?;
    let comparison = tasks::release::diff::compare(&project, &flags.before, &flags.after)?;
    if flags.json {
        println!("{}", serde_json::to_string_pretty(&comparison)?);
        Ok(())
    } else {
        print_comparison(&comparison)
    }
}

fn print_comparison(comparison: &ReleaseComparison) -> Result<(), Error> {
    println!("Release comparison (verified local snapshots; no live status checked)");
    println!("Before: {}", comparison.before.id);
    println!("After:  {}", comparison.after.id);
    let before = &comparison.before.metadata;
    let after = &comparison.after.metadata;
    if comparison.metadata_changed {
        print_metadata_change("Destination", &before.destination, &after.destination)?;
        print_metadata_change("Public URL", &before.url, &after.url)?;
        print_metadata_change("Pipeline", &before.pipeline, &after.pipeline)?;
        print_metadata_change("Website node", &before.website_node, &after.website_node)?;
        print_metadata_change(
            "Berlin version",
            &before.berlin_version,
            &after.berlin_version,
        )?;
    } else {
        println!("Release metadata unchanged.");
    }
    let files = &comparison.files;
    println!(
        "Files: {} added, {} modified, {} removed, {} unchanged",
        files.added.len(),
        files.modified.len(),
        files.removed.len(),
        files.unchanged
    );
    if files.added.is_empty() && files.modified.is_empty() && files.removed.is_empty() {
        println!("No file changes.");
    }
    for file in &files.added {
        println!("+ {} ({} bytes)", file.path, file.bytes);
    }
    for file in &files.modified {
        println!(
            "~ {} ({} -> {} bytes)",
            file.before.path, file.before.bytes, file.after.bytes
        );
    }
    for file in &files.removed {
        println!("- {} ({} bytes)", file.path, file.bytes);
    }
    print_document_comparison(&comparison.documents);
    Ok(())
}

fn print_document_comparison(comparison: &DocumentComparison) {
    let changes = match comparison {
        DocumentComparison::Compared { changes } => changes,
        DocumentComparison::Unavailable {
            before_available,
            after_available,
        } => {
            println!(
                "Documents: comparison unavailable (BEFORE inventory: {}; AFTER inventory: {}).",
                if *before_available {
                    "present"
                } else {
                    "absent"
                },
                if *after_available {
                    "present"
                } else {
                    "absent"
                },
            );
            println!(
                "Legacy and imported releases have no document inventory; file comparison still applies."
            );
            return;
        }
    };
    println!(
        "Documents: {} added, {} modified, {} removed, {} unchanged",
        changes.added.len(),
        changes.modified.len(),
        changes.removed.len(),
        changes.unchanged,
    );
    for document in &changes.added {
        println!(
            "+ {:?}: {:?} ({})",
            document.id.0, document.title, document.path
        );
    }
    for document in &changes.modified {
        let categories = [
            (document.content_changed, "content"),
            (document.metadata_changed, "metadata"),
            (document.route_changed, "route"),
        ]
        .into_iter()
        .filter_map(|(changed, name)| changed.then_some(name))
        .collect::<Vec<_>>();
        println!(
            "~ {:?}: {:?} [{}]",
            document.after.id.0,
            document.after.title,
            categories.join(", ")
        );
        if document.route_changed {
            println!("  {} -> {}", document.before.path, document.after.path);
        }
    }
    for document in &changes.removed {
        println!(
            "- {:?}: {:?} ({})",
            document.id.0, document.title, document.path
        );
    }
}

fn print_metadata_change<T: serde::Serialize + PartialEq>(
    label: &str,
    before: &T,
    after: &T,
) -> Result<(), Error> {
    if before != after {
        // JSON quoting keeps user-authored values escaped in terminal output too.
        println!(
            "{label}: {} -> {}",
            serde_json::to_string(before)?,
            serde_json::to_string(after)?
        );
    }
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
