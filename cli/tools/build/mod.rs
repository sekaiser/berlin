use errors::anyhow::Error;

use crate::args::BuildFlags;
use crate::args::Flags;
use crate::proc_state::ProcState;
use crate::site_generator::create_main_site_generator;

pub async fn build(flags: Flags, _build_flags: BuildFlags) -> Result<(), Error> {
    let ps = ProcState::build(flags.clone()).await?;
    let site_generator = create_main_site_generator(&ps)?;
    site_generator.run_tasks()?;

    Ok(())
}
