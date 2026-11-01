//! Coordinates one pipeline run, its transaction, and best-effort receipts.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Instant, SystemTime};

use anyhow::{Context, Error};
use berlin_core::{NodeId, PipelineLoader as _, PipelineNode, PipelinePlan};
use berlin_pipeline_dsl::RhaiPipelineLoader;

use super::{ExecutionOptions, RuntimeArtifact, executor::NodeExecutor, output, receipt};
use crate::project::Project;

pub(super) struct PipelineRun<'a> {
    project: &'a Project,
    pipeline: &'a str,
    options: ExecutionOptions,
    started_at: SystemTime,
    timer: Instant,
    artifacts: HashMap<NodeId, RuntimeArtifact>,
    diagnostics: Vec<receipt::Diagnostic>,
    transaction: Option<output::OutputTransaction>,
    output_root: PathBuf,
    owned_roots: Vec<PathBuf>,
    consumed_inputs: Vec<receipt::ConsumedInput>,
    // Fields drop in declaration order: release the lock after transaction cleanup.
    lock: Option<output::ProjectLock>,
}

struct PreparedPipeline {
    project: Project,
    program: RhaiPipelineLoader,
    plan: PipelinePlan,
}

impl<'a> PipelineRun<'a> {
    pub(super) fn new(project: &'a Project, pipeline: &'a str, options: ExecutionOptions) -> Self {
        Self {
            project,
            pipeline,
            options,
            started_at: SystemTime::now(),
            timer: Instant::now(),
            artifacts: HashMap::new(),
            diagnostics: Vec::new(),
            transaction: None,
            output_root: project.root().to_path_buf(),
            owned_roots: Vec::new(),
            consumed_inputs: Vec::new(),
            lock: None,
        }
    }

    pub(super) fn run(mut self) -> Result<(), Error> {
        // Hold the project lock through preparation, commit, and receipt writing.
        self.lock = if self.options.dry_run {
            None
        } else {
            Some(output::ProjectLock::acquire(self.project.root())?)
        };
        let prepared = self.prepare().inspect_err(|error| {
            self.record_setup_failure(error);
        })?;
        let outcome = self.execute_and_commit(&prepared);
        self.record_receipt(&prepared, &outcome);
        outcome
    }

    pub(super) fn release(mut self) -> Result<super::release::WebsiteRelease, Error> {
        self.lock = Some(output::ProjectLock::acquire(self.project.root())?);
        let prepared = match self.prepare() {
            Ok(prepared) => prepared,
            Err(error) => {
                let outcome = Err(error);
                self.record_release_attempt(None, super::state::attempts::Phase::Setup, &outcome);
                return outcome;
            }
        };
        let mut phase = super::state::attempts::Phase::Execution;
        let outcome = self.execute(&prepared).and_then(|()| {
            phase = super::state::attempts::Phase::Sealing;
            // Seal the complete staged site; never install over the preview output.
            super::release::seal(
                &prepared.project,
                self.pipeline,
                &prepared.plan,
                &self.output_root,
                &self.artifacts,
            )
        });
        self.record_release_attempt(Some(&prepared), phase, &outcome);
        outcome
    }

    fn record_release_attempt(
        &self,
        prepared: Option<&PreparedPipeline>,
        phase: super::state::attempts::Phase,
        outcome: &Result<super::release::WebsiteRelease, Error>,
    ) {
        let persisted = (|| -> Result<(), Error> {
            if !super::state::active(self.project.root())? {
                return Ok(());
            }
            let error = outcome.as_ref().err().map(ToString::to_string);
            let receipt = match prepared {
                Some(prepared) => receipt::capture(receipt::ReceiptContext {
                    consumed_inputs: &self.consumed_inputs,
                    project: &prepared.project,
                    pipeline: self.pipeline,
                    started_at: self.started_at,
                    duration: self.timer.elapsed(),
                    plan: &prepared.plan,
                    artifacts: &self.artifacts,
                    outputs: match outcome {
                        Ok(release) => receipt::Outputs::Sealed(release),
                        Err(_) => receipt::Outputs::Installed(&[]),
                    },
                    diagnostics: &self.diagnostics,
                    error: error.as_deref(),
                })?,
                None => receipt::capture_setup_failure(
                    self.project,
                    self.pipeline,
                    self.started_at,
                    self.timer.elapsed(),
                    error
                        .as_deref()
                        .context("release setup failure has no error")?,
                )?,
            };
            super::state::Store::open(self.project.root())?.record_release_attempt(
                &super::state::attempts::Attempt {
                    schema_version: 1,
                    phase,
                    receipt,
                    release: outcome.as_ref().ok().map(|r| r.id.clone()),
                },
            )
        })();
        if let Err(error) = persisted {
            log::warn!("Unable to record release attempt: {error}");
        }
    }

    fn prepare(&mut self) -> Result<PreparedPipeline, Error> {
        let (program, inputs) = crate::pipeline::load_pipeline_program_snapshot(self.project)?;
        let plan = program
            .load(self.pipeline)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        plan.validate()?;
        self.consumed_inputs = inputs;
        if !self.options.dry_run
            && !self.options.allow_tex
            && plan.nodes().iter().any(|node| {
                matches!(
                    node.operation,
                    berlin_core::Operation::CompilePapers { .. }
                        | berlin_core::Operation::CompileDiagrams { .. }
                )
            })
        {
            anyhow::bail!(
                "this pipeline compiles LaTeX; use --allow-tex only for trusted sources and toolchains (TeX is not sandboxed)"
            );
        }
        if self.options.production {
            super::production::validate_plan(&plan)?;
        }
        self.prepare_outputs(&plan)?;
        let project = self
            .project
            .with_captured_source_roots(program.source_roots());
        Ok(PreparedPipeline {
            project,
            program,
            plan,
        })
    }

    fn prepare_outputs(&mut self, plan: &PipelinePlan) -> Result<(), Error> {
        if self.options.dry_run {
            return Ok(());
        }
        let transaction = output::OutputTransaction::new(
            self.project.root(),
            plan.nodes()
                .iter()
                .filter_map(|node| node.output_path.as_deref()),
        )?;
        self.output_root = transaction.staging_root().to_path_buf();
        self.owned_roots = transaction.owned_roots().to_vec();
        self.transaction = Some(transaction);
        Ok(())
    }

    fn execute_and_commit(&mut self, prepared: &PreparedPipeline) -> Result<(), Error> {
        self.execute(prepared)?;
        if let Some(transaction) = self.transaction.take() {
            transaction.commit()?;
        }
        Ok(())
    }

    fn execute(&mut self, prepared: &PreparedPipeline) -> Result<(), Error> {
        let composed = prepared
            .plan
            .nodes()
            .iter()
            .any(|node| matches!(node.operation, berlin_core::Operation::Compose { .. }));
        for node in prepared.plan.topological_order()? {
            let artifact = NodeExecutor {
                consumed_inputs: &mut self.consumed_inputs,
                project: &prepared.project,
                artifacts: &self.artifacts,
                program: &prepared.program,
                options: self.options,
                output_root: &self.output_root,
                diagnostics: &mut self.diagnostics,
            }
            .execute(node)?;
            if composed
                && matches!(&artifact,Some(RuntimeArtifact::MarkdownSources(sources)|RuntimeArtifact::DataSources(sources)) if sources.is_empty())
            {
                anyhow::bail!(
                    "publication source node '{}' matched no files; prepare generated Org/paper inputs before checking or rendering the publication",
                    node.id
                );
            }
            self.accept_output(node, artifact)?;
        }
        if self.options.production && !self.options.dry_run {
            super::production::prepare(&prepared.plan, &self.output_root, &self.artifacts)?;
        }
        if !self.options.dry_run {
            for node in prepared.plan.nodes() {
                let text = match node.operation {
                    berlin_core::Operation::RenderHtml { .. } => false,
                    berlin_core::Operation::RenderText { .. } => true,
                    _ => continue,
                };
                let composed =
                    super::executor::runtime_input(node, &self.artifacts, 0, |artifact| {
                        match artifact {
                            RuntimeArtifact::Composed(composed) => Some(composed),
                            _ => None,
                        }
                    })?;
                let output = self.output_root.join(
                    node.output_path
                        .as_ref()
                        .context("publication output has no path")?,
                );
                super::composed::finalize_manifest(
                    &output,
                    text,
                    self.options.production,
                    composed.catalog().limits(),
                )?;
            }
        }
        Ok(())
    }

    fn accept_output(
        &mut self,
        node: &PipelineNode,
        artifact: Option<RuntimeArtifact>,
    ) -> Result<(), Error> {
        let signature = node.operation.signature();
        match artifact {
            Some(artifact) if signature.produces_value => {
                let actual = artifact.kind();
                if actual != signature.output {
                    anyhow::bail!(
                        "pipeline node '{}' produced {actual:?}, expected {:?}",
                        node.id,
                        signature.output
                    );
                }
                self.artifacts.insert(node.id.clone(), artifact);
            }
            None if !signature.produces_value => {}
            Some(_) => {
                anyhow::bail!(
                    "effect-only pipeline node '{}' unexpectedly produced a runtime value",
                    node.id
                );
            }
            None => {
                anyhow::bail!(
                    "pipeline node '{}' did not produce its declared {:?} value",
                    node.id,
                    signature.output
                );
            }
        }
        Ok(())
    }

    fn record_setup_failure(&self, error: &Error) {
        if self.options.dry_run {
            return;
        }
        if let Err(receipt_error) = receipt::write_setup_failure(
            self.project,
            self.pipeline,
            self.started_at,
            self.timer.elapsed(),
            &error.to_string(),
        ) {
            log::warn!("Unable to write failed build receipt: {receipt_error}");
        }
    }

    fn record_receipt(&self, prepared: &PreparedPipeline, outcome: &Result<(), Error>) {
        if self.options.dry_run {
            return;
        }
        let error_message = outcome.as_ref().err().map(ToString::to_string);
        if let Err(receipt_error) = receipt::write(receipt::ReceiptContext {
            consumed_inputs: &self.consumed_inputs,
            project: &prepared.project,
            pipeline: self.pipeline,
            started_at: self.started_at,
            duration: self.timer.elapsed(),
            plan: &prepared.plan,
            artifacts: &self.artifacts,
            outputs: receipt::Outputs::Installed(&self.owned_roots),
            diagnostics: &self.diagnostics,
            error: error_message.as_deref(),
        }) {
            log::warn!("Unable to write build receipt: {receipt_error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_uses_source_roots_from_the_captured_pipeline_program() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path().join("site");
        std::fs::create_dir(&root).unwrap();
        for (directory, title) in [
            ("original", "Captured reference"),
            ("changed", "Uncaptured reference"),
        ] {
            let directory = workspace.path().join(directory);
            std::fs::create_dir(&directory).unwrap();
            std::fs::write(directory.join("resources.yaml"), format!(
                "schema_version: 1\nresources:\n  - id: paper\n    kind: paper\n    url: https://example.test/paper\n    title: {title}\n"
            )).unwrap();
        }
        let program = r#"
const SOURCE_ROOTS = #{data:"../original"};
fn references(c) {
    publication("references",#{title:"References"},[
        reading_list("Reading",c.select(#{origins_any:["external"]}).map_cards())
    ])
}
pipeline text {
    output render_text(compose(catalog([resources(load_data("@data/resources.yaml"))]),Fn("references")),"_text",text_config(#{}));
}
"#;
        let script = root.join("berlin.pipeline.rhai");
        std::fs::write(&script, program).unwrap();
        let project = Project::new(root.clone(), vec![]);
        let mut run = PipelineRun::new(&project, "text", ExecutionOptions::default());
        let prepared = run.prepare().unwrap();
        std::fs::write(&script, program.replace("../original", "../changed")).unwrap();
        let result = run.execute_and_commit(&prepared);
        run.record_receipt(&prepared, &result);
        result.unwrap();
        let output = std::fs::read_to_string(root.join("_text/publication.txt")).unwrap();
        assert!(output.contains("Captured reference"), "{output}");
        assert!(!output.contains("Uncaptured reference"));
        let receipt: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join(".berlin/receipts/text.json")).unwrap(),
        )
        .unwrap();
        let input = receipt["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|input| input["path"] == "berlin.pipeline.rhai")
            .unwrap();
        assert_eq!(
            input["sha256"],
            berlin_publication::input_digest(program.as_bytes())
        );
        assert!(
            std::fs::read_to_string(&script)
                .unwrap()
                .contains("../changed")
        );
        drop(run);
        // A later build captures the new configuration instead of caching the
        // first build's roots for the lifetime of the original Project handle.
        PipelineRun::new(&project, "text", ExecutionOptions::default())
            .run()
            .unwrap();
        let next = std::fs::read_to_string(root.join("_text/publication.txt")).unwrap();
        assert!(next.contains("Uncaptured reference"));
        let receipt: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join(".berlin/receipts/text.json")).unwrap(),
        )
        .unwrap();
        let input = receipt["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|input| input["path"] == "berlin.pipeline.rhai")
            .unwrap();
        assert_eq!(
            input["sha256"],
            berlin_publication::input_digest(
                program.replace("../original", "../changed").as_bytes()
            )
        );
    }
}
