//! CLI presentation and explicit export of review-only announcement drafts.

use crate::{
    args::ReleaseDraftsFlags,
    tasks::release::announcements::{self, AnnouncementPlan},
};
use anyhow::Error;
use std::path::PathBuf;

pub fn prepare(flags: ReleaseDraftsFlags, files: Vec<PathBuf>) -> Result<(), Error> {
    let project = super::release::sealed_project(files)?;
    let plan = announcements::prepare(&project, &flags.before, &flags.after)?;
    if let Some(output) = flags.output {
        let directory = announcements::export(&project, &plan, &output)?;
        eprintln!(
            "Exported {} draft starting points to {}. Nothing published.",
            plan.drafts.len(),
            directory.display()
        );
    }
    if flags.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        print_plan(&plan);
    }
    Ok(())
}

fn print_plan(plan: &AnnouncementPlan) {
    println!("LinkedIn announcement drafts — review required; live status not checked");
    println!("Before: {}", plan.before.id);
    println!("After:  {}", plan.after.id);
    println!(
        "{} draft starting points. Skipped: {} metadata/route-only, {} removed, {} unchanged documents.",
        plan.drafts.len(),
        plan.skipped.metadata_or_route_only,
        plan.skipped.removed,
        plan.skipped.unchanged
    );
    for item in &plan.drafts {
        println!(
            "\nDocument {:?} ({} characters):\n{}",
            item.document.id.0, item.draft.character_count, item.draft.text
        );
        for diagnostic in &item.draft.diagnostics {
            match diagnostic {
                berlin_document_linkedin::LinkedInDraftDiagnostic::CharacterLimitExceeded {
                    limit,
                    actual,
                } => {
                    println!(
                        "Review warning: {actual} characters exceeds Berlin's default limit of {limit}."
                    );
                }
            }
        }
    }
    println!("\n{}", plan.review_notes);
}
