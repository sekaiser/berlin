use crate::{
    args::review::Action,
    project::Project,
    tasks::{
        review::{self, Brief, Disposition, Event, Response, Session, Snapshot},
        state::Store,
    },
};
use anyhow::{Error, bail};
use std::path::PathBuf;

pub fn run(action: Action, files: Vec<PathBuf>) -> Result<(), Error> {
    if !files.is_empty() {
        bail!(
            "article review does not evaluate pipelines; pass the authored article path instead of --pipeline-file"
        );
    }
    let project = Project::load(vec![])?;
    // Review must not run output recovery or touch preview/release output.
    if matches!(
        action,
        Action::Start { .. }
            | Action::Record { .. }
            | Action::Respond { .. }
            | Action::Advance { .. }
    ) {
        review::with_lock(project.root(), || execute(action, &project))
    } else {
        execute(action, &project)
    }
}
fn execute(action: Action, project: &Project) -> Result<(), Error> {
    let store = Store::open(project.root())?;
    let view = match action {
        Action::Start {
            article,
            audience,
            goal,
            thesis,
            prerequisites,
        } => {
            let snapshot = Snapshot::capture(&project.root().join(article))?;
            let id = store.start_review(Session {
                schema_version: 1,
                snapshot,
                brief: Brief {
                    audience,
                    goal,
                    thesis,
                    prerequisites,
                },
            })?;
            store.review(id)?
        }
        Action::List { document, limit } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&store.reviews(document.as_deref(), limit)?)?
            );
            return Ok(());
        }
        Action::Show { id, json: false } => {
            println!("{}", review::packet::display(&store.review(id)?));
            return Ok(());
        }
        Action::Show { id, json: true } => store.review(id)?,
        Action::Packet(id) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&review::packet::packet(&store.review(id)?)?)?
            );
            return Ok(());
        }
        Action::Record { id, file } => {
            let text = review::read_text(&project.root().join(file), review::RECORD_LIMIT as u64)?;
            let report = serde_json::from_str(&text)?;
            store.append_review(id, Event::Report { report })?
        }
        Action::Respond {
            id,
            finding,
            disposition,
            response,
            evidence,
        } => {
            let view = store.review(id)?;
            let step = view
                .current_step()
                .ok_or_else(|| anyhow::anyhow!("review already completed"))?
                .clone();
            let disposition = match disposition.as_str() {
                "addressed" => Disposition::Addressed,
                "disputed" => Disposition::Disputed,
                "deferred" => Disposition::Deferred,
                _ => bail!("unknown response disposition"),
            };
            store.append_review(
                id,
                Event::Respond {
                    step,
                    response: Response {
                        finding,
                        disposition,
                        response,
                        evidence,
                    },
                },
            )?
        }
        Action::Advance { id, reflection } => {
            let view = store.review(id)?;
            let step = view
                .current_step()
                .ok_or_else(|| anyhow::anyhow!("review already completed"))?
                .clone();
            store.append_review(id, Event::Advance { step, reflection })?
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&review::packet::status(&view))?
    );
    Ok(())
}
