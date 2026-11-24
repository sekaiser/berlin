//! CLI presentation of channel preparation, reviews and explicit draft delivery.
use crate::{
    args::announcement::{Action, LinkedInAction},
    tasks::announcement::{self, Review, ReviewState},
};
use anyhow::Error;
use std::path::PathBuf;

pub fn run(action: Action, files: Vec<PathBuf>) -> Result<(), Error> {
    let project = super::release::sealed_project(files)?;
    match action {
        Action::Preview { source } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&announcement::substack::preview(&project, &source)?)?
            );
        }
        Action::LinkedIn(action) => {
            use announcement::linkedin::{self, connection};
            let result = match action {
                LinkedInAction::Status { account } => {
                    serde_json::json!({"account":account,"member":connection::target(&project,&account)?,"remote_checked":false})
                }
                LinkedInAction::Connect {
                    account,
                    token_env,
                    confirm,
                    version,
                    organization,
                } => connection::connect(
                    &project,
                    &account,
                    &token_env,
                    confirm.as_deref(),
                    &version,
                    organization.as_deref(),
                )?,
                LinkedInAction::AuthStart {
                    account,
                    client_id,
                    redirect,
                    version,
                    organization,
                } => connection::auth_start(
                    &project,
                    &account,
                    &client_id,
                    &redirect,
                    &version,
                    organization.as_deref(),
                )?,
                LinkedInAction::AuthFinish {
                    account,
                    callback,
                    secret_env,
                } => connection::auth_finish(&project, &account, &callback, &secret_env)?,
                LinkedInAction::Reconcile {
                    review,
                    post,
                    reported,
                    token_env,
                } => serde_json::to_value(linkedin::reconcile(
                    &project,
                    &review,
                    &post,
                    reported,
                    token_env.as_deref(),
                )?)?,
                LinkedInAction::CallbackPage { url, output } => {
                    connection::callback_page(&project, &url, &output)?
                }
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Action::Deliveries { review } => {
            if let Some(id) = &review {
                crate::tasks::release::validate_id(id)?;
            }
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &crate::tasks::state::Store::open(project.root())?
                        .delivery_events(review.as_deref())?
                )?
            );
        }
        Action::Prepare {
            release,
            document,
            output,
            updated,
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&announcement::packages::prepare(
                    &project, &release, &document, &output, updated
                )?)?
            );
        }
        Action::Deliver {
            source,
            confirm,
            token_env,
        } => {
            if source.channel == "substack" {
                anyhow::bail!(
                    "Substack delivery is manual: use 'announcement preview', publish in Substack, then 'announcement record'"
                );
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&if source.channel == "linkedin" {
                    announcement::linkedin::deliver(
                        &project,
                        &source,
                        &confirm,
                        token_env.as_deref(),
                    )?
                } else {
                    announcement::delivery::deliver(
                        &project,
                        &source,
                        &confirm,
                        token_env.as_deref().unwrap_or("DEVTO_API_KEY"),
                    )?
                })?
            );
        }
        Action::Reconcile {
            review,
            draft,
            token_env,
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&announcement::delivery::reconcile(
                    &project, &review, draft, &token_env
                )?)?
            );
        }
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
                        "{} — {:?} / {}:{}",
                        approval.review_id,
                        approval.snapshot.document.id.0,
                        approval.snapshot.channel(),
                        approval.snapshot.account
                    );
                    match &entry.publication {
                        Some(publication) => println!(
                            "  {}: {}",
                            publication.evidence_label(),
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
        "Account: {}:{}",
        review.snapshot.channel(),
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
    if let Some(image) = &review.snapshot.image {
        println!(
            "Teaser: {}\nAlternative text: {}\nImage SHA-256: {}",
            image.url, image.alt, image.sha256
        );
    }
    if let Some(target) = &review.snapshot.linkedin {
        if let Some(organization) = &target.organization {
            println!(
                "LinkedIn Page author: {} ({})",
                organization.name, organization.urn
            );
        }
        println!(
            "LinkedIn member: {} ({})\nVisibility: {}\nAPI version: {}",
            target.name, target.member_urn, target.visibility, target.api_version
        );
    }
    if review.snapshot.channel() == "substack" {
        let draft: announcement::substack::Draft =
            serde_json::from_str(&review.snapshot.text).expect("validated Substack edition");
        println!(
            "Substack publication: {}\nAudience: {:?}\nSend email: {} (manual setting)",
            draft
                .publication_url
                .as_ref()
                .map_or("not set", |url| url.as_str()),
            draft.audience,
            draft.send_email
        );
    }
    for warning in &review.snapshot.warnings {
        println!("Review note: {warning}");
    }
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
            ReviewState::SubmissionAccepted =>
                "LinkedIn accepted submission; live visibility not checked",
            ReviewState::ApiVerified => "LinkedIn content verified by API",
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
