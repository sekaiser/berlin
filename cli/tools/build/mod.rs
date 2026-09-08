use anyhow::Error;

use crate::args::BuildFlags;
use crate::project::Project;
use crate::tasks::ExecutionOptions;
use crate::tasks::run_pipeline_with_options;

pub fn build(
    build_flags: BuildFlags,
    pipeline_files: Vec<std::path::PathBuf>,
) -> Result<(), Error> {
    let project = Project::load(pipeline_files)?;
    run_pipeline_with_options(
        &project,
        &build_flags.pipeline,
        ExecutionOptions {
            dry_run: build_flags.dry_run,
        },
    )
}
