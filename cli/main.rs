mod args;
mod colors;
mod pipeline;
mod project;

mod tasks;
mod templates;
mod tools;
mod util;
mod version;

use std::env;

use anyhow::Error;

use crate::args::BerlinSubcommand;
use crate::args::Flags;
use crate::args::flags_from_vec;

fn run_local<F: std::future::Future>(future: F) -> F::Output {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(16)
        .build()
        .expect("Tokio runtime initialization failed");
    tokio::task::LocalSet::new().block_on(&runtime, future)
}

fn run_subcommand(flags: Flags) -> Result<(), Error> {
    match flags.subcommand.clone() {
        BerlinSubcommand::Release(release_flags) => {
            tools::release::prepare(release_flags, flags.pipeline_files)
        }
        BerlinSubcommand::ReleasePlan(id) => tools::release::plan(&id, flags.pipeline_files),
        BerlinSubcommand::Publish(publish_flags) => {
            tools::release::publish(publish_flags, flags.pipeline_files)
        }
        BerlinSubcommand::Publication(publication_flags) => {
            tools::release::inspect(publication_flags, flags.pipeline_files)
        }
        BerlinSubcommand::Plan(plan_flags) => {
            pipeline::print_current_plan(plan_flags, flags.pipeline_files)
        }
        BerlinSubcommand::Check(check_flags) => {
            tools::check::check(check_flags, flags.pipeline_files)
        }
        BerlinSubcommand::Build(build_flags) => {
            tools::build::build(build_flags, flags.pipeline_files)
        }
        BerlinSubcommand::Serve(serve_flags) => {
            run_local(tools::serve::serve(serve_flags, flags.pipeline_files))
        }
    }
}

fn setup_panic_hook() {
    // This function does two things inside of the panic hook:
    // - Tokio does not exit the process when a task panics, so we define a custom
    //   panic hook to implement this behaviour.
    // - We print a message to stderr to indicate that this is a bug in Berlin, and
    //   should be reported to us.
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        eprintln!("\n============================================================");
        eprintln!("Berlin has panicked. This is a bug in Berlin. Please report this");
        eprintln!("at https://github.com/sekaiser/berlin/issues/new.");
        eprintln!("If you can reliably reproduce this panic, include the");
        eprintln!("reproduction steps and re-run with the RUST_BACKTRACE=1 env");
        eprintln!("var set and include the backtrace in your report.");
        eprintln!();
        eprintln!("Platform: {} {}", env::consts::OS, env::consts::ARCH);
        eprintln!("Version: {}", version::berlin());
        eprintln!("Args: {:?}", env::args().collect::<Vec<_>>());
        eprintln!();
        orig_hook(panic_info);
        std::process::exit(1);
    }));
}

fn unwrap_or_exit<T>(result: Result<T, Error>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => {
            let error_string = format!("{error:?}");
            let error_code = 1;

            eprintln!(
                "{}: {}",
                colors::red_bold("error"),
                error_string.trim_start_matches("error: ")
            );
            std::process::exit(error_code);
        }
    }
}

pub fn main() {
    setup_panic_hook();
    let args: Vec<String> = env::args().collect();

    let flags = match flags_from_vec(args) {
        Ok(flags) => flags,
        Err(err @ clap::Error { .. })
            if err.kind() == clap::error::ErrorKind::DisplayHelp
                || err.kind() == clap::error::ErrorKind::DisplayVersion =>
        {
            err.print().unwrap();
            std::process::exit(0);
        }
        Err(err) => unwrap_or_exit(Err(Error::from(err))),
    };
    util::logger::init(flags.log_level);

    unwrap_or_exit(run_subcommand(flags));
}
