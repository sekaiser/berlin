//! Local, revision-bound editorial dialogue. Reports are supplied by a reviewer,
//! not inferred by Berlin or treated as publication approval.
pub(crate) mod packet;
mod source;

use anyhow::{Error, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) use source::Snapshot;
pub(crate) use source::read_text;
pub(crate) const RECORD_LIMIT: usize = 256 * 1024;
pub(crate) const EVENT_LIMIT: usize = 256;

pub(crate) fn with_lock<T>(
    root: &std::path::Path,
    operation: impl FnOnce() -> Result<T, Error>,
) -> Result<T, Error> {
    let _lock = super::output::ProjectLock::acquire_without_recovery(root)?;
    operation()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Step {
    Audience,
    Argument,
    Challenge,
    Verification,
}
impl Step {
    pub(crate) const ALL: [Self; 4] = [
        Self::Audience,
        Self::Argument,
        Self::Challenge,
        Self::Verification,
    ];
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Audience => "audience",
            Self::Argument => "argument",
            Self::Challenge => "challenge",
            Self::Verification => "verification",
        }
    }
    pub(crate) fn questions(&self) -> [&'static str; 3] {
        match self {
            Self::Audience => [
                "What will this reader understand or be able to decide after reading? Compare that with the author's goal.",
                "Where does the explanation assume knowledge the intended reader may not have? Identify a concrete missing bridge.",
                "Where would an experienced reader need a deeper explanation, and where does detail distract from the central idea? Reader simulation is a hypothesis, not audience research.",
            ],
            Self::Argument => [
                "Identify the central claim and trace the evidence and assumptions needed for the conclusion. Where is an inference unsupported?",
                "Separate observation, design choice, hypothesis and measured result. Where does the article generalize beyond its example?",
                "Which explanation is essential to following the argument, and which detail can be removed without weakening it?",
            ],
            Self::Challenge => [
                "What is the strongest technically defensible objection? Explain the conditions in which it applies.",
                "Construct a concrete failure case or counterexample. Which guarantee, assumption or trade-off does it expose?",
                "Which plausible alternative could be preferable under another workload or constraint? What evidence would change the recommendation?",
            ],
            Self::Verification => [
                "Check the consequential factual and technical claims against primary contracts, source code or experiments where available. State exactly what you inspected or ran.",
                "Do code, measurements and citations support the stated scope? Distinguish verified facts from engineering judgment and unresolved claims.",
                "Which claims still need evidence or qualification? Never invent a citation, test execution, benchmark, toolchain or result.",
            ],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Brief {
    pub audience: String,
    pub goal: String,
    pub thesis: String,
    pub prerequisites: String,
}
impl Brief {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        for value in [
            &self.audience,
            &self.goal,
            &self.thesis,
            &self.prerequisites,
        ] {
            text(value)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Session {
    pub schema_version: u8,
    pub snapshot: Snapshot,
    pub brief: Brief,
}
impl Session {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        ensure!(
            self.schema_version == 1,
            "unsupported article review version"
        );
        self.snapshot.validate()?;
        self.brief.validate()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Evidence {
    pub reference: String,
    pub method: Method,
    pub assessment: Assessment,
    pub details: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Method {
    Documentation,
    CodeExecution,
    Measurement,
    Reasoning,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Assessment {
    Supports,
    Contradicts,
    Unresolved,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Finding {
    pub start_line: usize,
    pub end_line: usize,
    pub quote: String,
    pub question: String,
    pub reason: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Report {
    pub schema_version: u8,
    pub review_id: i64,
    pub source_sha256: String,
    pub step: Step,
    pub reviewer: String,
    pub summary: String,
    pub limitations: String,
    pub findings: Vec<Finding>,
}
impl Report {
    fn validate(&self, view: &View) -> Result<(), Error> {
        ensure!(
            self.schema_version == 1 && self.review_id == view.id,
            "report belongs to another review/version"
        );
        ensure!(
            self.source_sha256 == view.session.snapshot.sha256,
            "report source revision differs from captured article"
        );
        ensure!(
            Some(&self.step) == view.current_step(),
            "report is not for the current step"
        );
        for v in [&self.reviewer, &self.summary, &self.limitations] {
            text(v)?;
        }
        ensure!(
            self.findings.len() <= 5,
            "a step accepts at most five consequential findings"
        );
        let lines: Vec<_> = view.session.snapshot.text.lines().collect();
        for f in &self.findings {
            ensure!(
                f.start_line > 0 && f.end_line >= f.start_line && f.end_line <= lines.len(),
                "finding line range is outside captured article"
            );
            ensure!(
                lines[f.start_line - 1..f.end_line].join("\n") == f.quote,
                "finding quote does not match its captured lines"
            );
            for v in [&f.quote, &f.question, &f.reason] {
                text(v)?;
            }
            ensure!(
                f.evidence.len() <= 3,
                "a finding accepts at most three evidence records"
            );
            for e in &f.evidence {
                text(&e.reference)?;
                text(&e.details)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Disposition {
    Addressed,
    Disputed,
    Deferred,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Response {
    pub finding: usize,
    pub disposition: Disposition,
    pub response: String,
    pub evidence: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Event {
    Report { report: Report },
    Respond { step: Step, response: Response },
    Advance { step: Step, reflection: String },
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct StepRecord {
    pub report: Report,
    pub responses: BTreeMap<usize, Response>,
    pub reflection: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct View {
    pub id: i64,
    pub session: Session,
    pub steps: Vec<StepRecord>,
    pub event_count: usize,
}
impl View {
    pub(crate) fn new(id: i64, session: Session) -> Result<Self, Error> {
        ensure!(id > 0, "invalid review ID");
        session.validate()?;
        Ok(Self {
            id,
            session,
            steps: vec![],
            event_count: 0,
        })
    }
    pub(crate) fn current_step(&self) -> Option<&Step> {
        let completed = self.steps.iter().filter(|s| s.reflection.is_some()).count();
        Step::ALL.get(completed)
    }
    pub(crate) fn current_record(&self) -> Option<&StepRecord> {
        self.steps
            .last()
            .filter(|s| Some(&s.report.step) == self.current_step())
    }
    pub(crate) fn apply(&mut self, event: Event) -> Result<(), Error> {
        ensure!(
            self.event_count < EVENT_LIMIT,
            "review event limit reached; start a new review"
        );
        let step = self
            .current_step()
            .ok_or_else(|| anyhow::anyhow!("review already completed"))?
            .clone();
        match event {
            Event::Report { report } => {
                ensure!(
                    self.current_record().is_none(),
                    "this step already has a report"
                );
                report.validate(self)?;
                self.steps.push(StepRecord {
                    report,
                    responses: BTreeMap::new(),
                    reflection: None,
                });
            }
            Event::Respond {
                step: event_step,
                response,
            } => {
                ensure!(event_step == step, "response step mismatch");
                text(&response.response)?;
                if let Some(evidence) = &response.evidence {
                    text(evidence)?;
                }
                let record = self
                    .steps
                    .last_mut()
                    .filter(|s| s.report.step == step)
                    .ok_or_else(|| anyhow::anyhow!("record a report before responding"))?;
                ensure!(
                    (1..=record.report.findings.len()).contains(&response.finding),
                    "unknown finding in current step"
                );
                record.responses.insert(response.finding, response);
            }
            Event::Advance {
                step: event_step,
                reflection,
            } => {
                ensure!(event_step == step, "advance step mismatch");
                text(&reflection)?;
                let record = self
                    .steps
                    .last_mut()
                    .filter(|s| s.report.step == step)
                    .ok_or_else(|| anyhow::anyhow!("record a report before advancing"))?;
                ensure!(
                    record.responses.len() == record.report.findings.len(),
                    "respond to every finding before advancing; deferral requires a reason"
                );
                record.reflection = Some(reflection);
            }
        }
        self.event_count += 1;
        Ok(())
    }
    pub(crate) fn outstanding(&self) -> usize {
        self.steps
            .iter()
            .map(|s| {
                s.report.findings.len() - s.responses.len()
                    + s.responses
                        .values()
                        .filter(|r| matches!(r.disposition, Disposition::Deferred))
                        .count()
            })
            .sum()
    }
}

pub(crate) fn text(value: &str) -> Result<(), Error> {
    if value.trim().is_empty() || value.len() > 16 * 1024 {
        bail!("review text must be nonempty and at most 16 KiB");
    }
    Ok(())
}
