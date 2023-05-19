mod args;
mod cache;
mod hera;
mod proc_state;
mod site_generator;
mod tasks;
mod tools;
mod util;
mod version;

use crate::args::flags_from_vec;
use crate::args::BerlinSubcommand;
use crate::args::Flags;

use berlin_runtime::colors;
use berlin_runtime::tokio_util::run_local;
use errors::anyhow::Error;
use std::env;

async fn run_subcommand(flags: Flags) -> Result<i32, Error> {
    match flags.subcommand.clone() {
        BerlinSubcommand::Info(_info_flags) => Ok(0),
        BerlinSubcommand::Build(build_flags) => {
            tools::build::build(flags, build_flags).await?;
            Ok(0)
        }
        BerlinSubcommand::Init(init_flags) => {
            tools::init::init_project(init_flags).await?;
            Ok(0)
        }
        BerlinSubcommand::Serve(serve_flags) => {
            tools::serve::serve(flags, serve_flags).await?;
            Ok(0)
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
        eprintln!("at https://github.com/mttrbit/berlin/issues/new.");
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

    let future = async move {
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

        run_subcommand(flags).await
    };

    let exit_code = unwrap_or_exit(run_local(future));

    std::process::exit(exit_code);
}
