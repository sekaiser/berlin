//! Presentation and exit behavior for the local authoring report.

use std::io::{self, Write};

use anyhow::{Error, bail};
use berlin_content::{AuthoringIssue, AuthoringReport, Severity};
use serde::Serialize;

use crate::{args::CheckFlags, project::Project};

#[derive(Serialize)]
struct CheckOutput<'a> {
    schema_version: u32,
    pipeline: &'a str,
    #[serde(flatten)]
    outcome: Outcome,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Outcome {
    Complete { report: AuthoringReport },
    Incomplete { message: String },
}

pub fn check(flags: CheckFlags) -> Result<(), Error> {
    let result =
        Project::load().and_then(|project| crate::tasks::check::inspect(&project, &flags.pipeline));
    let outcome = match result {
        Ok(report) => Outcome::Complete { report },
        Err(error) => Outcome::Incomplete {
            message: format!("{error:#}"),
        },
    };
    let failed = match &outcome {
        Outcome::Complete { report } => report.has_errors(),
        Outcome::Incomplete { .. } => true,
    };
    let output = CheckOutput {
        schema_version: 1,
        pipeline: &flags.pipeline,
        outcome,
    };
    let mut stdout = io::stdout().lock();
    if flags.json {
        serde_json::to_writer_pretty(&mut stdout, &output)?;
        writeln!(stdout)?;
    } else {
        write_text(&mut stdout, &output)?;
    }
    stdout.flush()?;
    if failed {
        bail!("Authoring check failed; see the report above");
    }
    Ok(())
}

fn write_text(output: &mut impl Write, check: &CheckOutput<'_>) -> io::Result<()> {
    writeln!(output, "Authoring check: {}", check.pipeline)?;
    let report = match &check.outcome {
        Outcome::Complete { report } => report,
        Outcome::Incomplete { message } => {
            writeln!(output, "Incomplete: {message}")?;
            return Ok(());
        }
    };
    let errors = report
        .findings
        .iter()
        .filter(|finding| finding.severity == Severity::Error)
        .count();
    writeln!(
        output,
        "Published documents: {}; guides: {}; excluded drafts: {}",
        report.published_documents, report.guides, report.excluded_drafts
    )?;
    writeln!(
        output,
        "{errors} errors, {} editorial observations",
        report.findings.len() - errors
    )?;
    for (severity, heading) in [
        (Severity::Error, "Publication errors"),
        (Severity::Observation, "Editorial observations (optional)"),
    ] {
        let findings: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .collect();
        if findings.is_empty() {
            continue;
        }
        writeln!(output, "\n{heading}")?;
        for finding in findings {
            let location = &finding.location;
            let message = match &finding.issue {
                AuthoringIssue::UnresolvedReference { target } => format!(
                    "unresolved_reference: target '{}' is not in the published collection",
                    target.0
                ),
                AuthoringIssue::DisconnectedNote => {
                    "disconnected_note: no connections to other published notes".into()
                }
                AuthoringIssue::NotInGuide => {
                    "not_in_guide: not directly linked from a published guide".into()
                }
            };
            writeln!(output, "  {} — {message}", location.document.0)?;
            if let Some(origin) = &location.origin {
                write!(output, "    Org: {}", origin.source)?;
                if let Some(heading) = &origin.heading {
                    write!(output, " — {}", heading.outline.join(" / "))?;
                }
                if origin.stale {
                    write!(output, " (source changed since export; file only)")?;
                }
                writeln!(output)?;
            }
            write!(output, "    {}", location.source)?;
            if let Some(anchor) = &location.anchor {
                write!(output, "#{}", anchor.0)?;
            }
            writeln!(output)?;
            if let Some(excerpt) = &location.excerpt {
                writeln!(output, "    {excerpt}")?;
            }
        }
    }
    writeln!(
        output,
        "\nScope: document connections in existing Markdown after mappings; not a full site validation."
    )
}
