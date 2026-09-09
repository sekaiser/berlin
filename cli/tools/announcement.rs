//! CLI presentation of reviews and user-reported posts, never platform delivery.
use crate::{
    args::announcement::Action,
    tasks::announcement::{self, Review, ReviewState},
};
use anyhow::Error;
use std::path::PathBuf;

pub fn run(action: Action, files: Vec<PathBuf>) -> Result<(), Error> {
    let project = super::release::sealed_project(files)?;
    match action {
        Action::Review {
            source,
            approve,
            json,
        } => {
            let review = announcement::review(&project, &source, approve.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&review)?);
            } else {
                print_review(&review);
            }
        }
        Action::Record {
            source,
            url,
            confirm,
        } => {
            let publication = announcement::record(&project, &source, &url, &confirm)?;
            println!("{}", serde_json::to_string_pretty(&publication)?);
            eprintln!(
                "Recorded your report only. Berlin did not publish, verify the remote text/account, or check that the website is live."
            );
        }
        Action::Status {
            directory,
            account,
            document,
            json,
        } => {
            let status = announcement::status(
                &project,
                directory.as_deref(),
                account.as_deref(),
                document.as_deref(),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                println!(
                    "Announcement status — local records; remote delivery and website live status not verified"
                );
                for review in &status.reviews {
                    print_review(review);
                }
                println!(
                    "\n{} saved approvals (including any reported publications):",
                    status.history.len()
                );
                for entry in &status.history {
                    let approval = &entry.approval;
                    println!(
                        "{} — {:?} / linkedin:{}",
                        approval.review_id,
                        approval.snapshot.document.id.0,
                        approval.snapshot.account
                    );
                    match &entry.publication {
                        Some(publication) => println!(
                            "  Reported published (not verified): {}",
                            publication.post_url
                        ),
                        None => println!("  Approved; no publication reported"),
                    }
                }
                if directory.is_none() {
                    println!(
                        "Pass a review directory and --account to compare current edited drafts with saved approvals."
                    );
                }
            }
        }
    }
    Ok(())
}

fn print_review(review: &Review) {
    println!("\nReview: {}", review.review_id);
    println!(
        "Document: {:?} — {:?}",
        review.snapshot.document.id.0, review.snapshot.document.title
    );
    println!(
        "Account: linkedin:{} (local name, not authentication)",
        review.snapshot.account
    );
    println!("Article: {}", review.snapshot.article_url);
    println!(
        "Releases: {} -> {}",
        review.snapshot.before_release, review.snapshot.after_release
    );
    println!(
        "Content SHA-256: {}",
        review.snapshot.document.content_sha256
    );
    println!("Text SHA-256: {}", review.snapshot.text_sha256);
    println!(
        "Characters: {} / {} (Berlin default)",
        review.character_count, review.character_limit
    );
    println!(
        "State: {}",
        match review.state {
            ReviewState::ReviewRequired => "review required",
            ReviewState::Approved => "approved; no publication reported",
            ReviewState::ReportedPublished => "reported published; not remotely verified",
        }
    );
    if !review.approval_allowed {
        println!(
            "Approval unavailable: text is empty or exceeds Berlin's default character limit."
        );
    }
    if !review.same_revision_reports.is_empty() {
        println!(
            "Already reported for this document content/account: {}",
            review.same_revision_reports.join(", ")
        );
    }
    println!(
        "--- exact text begins ---\n{}\n--- exact text ends ---",
        review.snapshot.text
    );
    println!(
        "Approval covers these exact bytes, including whitespace. No live-state check or posting is performed."
    );
}
