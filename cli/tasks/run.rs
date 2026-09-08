//! Coordinates one pipeline run, its transaction, and best-effort receipts.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Instant, SystemTime};

use anyhow::Error;
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
    // Fields drop in declaration order: release the lock after transaction cleanup.
    lock: Option<output::ProjectLock>,
}

struct PreparedPipeline {
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
        let prepared = self.prepare()?;
        self.execute(&prepared)?;
        // Seal the complete staged site; never install over the preview output.
        super::release::seal(
            self.project,
            self.pipeline,
            &prepared.plan,
            &self.output_root,
        )
    }

    fn prepare(&mut self) -> Result<PreparedPipeline, Error> {
        let program = crate::pipeline::load_pipeline_program(self.project)?;
        let plan = program
            .load(self.pipeline)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        plan.validate()?;
        self.prepare_outputs(&plan)?;
        Ok(PreparedPipeline { program, plan })
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
        for node in prepared.plan.topological_order()? {
            let artifact = NodeExecutor {
                project: self.project,
                artifacts: &self.artifacts,
                program: &prepared.program,
                options: self.options,
                output_root: &self.output_root,
                diagnostics: &mut self.diagnostics,
            }
            .execute(node)?;
            self.accept_output(node, artifact)?;
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
            project: self.project,
            pipeline: self.pipeline,
            started_at: self.started_at,
            duration: self.timer.elapsed(),
            plan: &prepared.plan,
            artifacts: &self.artifacts,
            owned_roots: &self.owned_roots,
            diagnostics: &self.diagnostics,
            error: error_message.as_deref(),
        }) {
            log::warn!("Unable to write build receipt: {receipt_error}");
        }
    }
}
