// Portions adapted from Deno.
// Copyright 2018-2023 the Deno authors. All rights reserved. MIT license.
// See cli/THIRD_PARTY_NOTICES.txt in the repository (THIRD_PARTY_NOTICES.txt
// in the CLI package) for the applicable permission and copyright notice.

use std::sync::LazyLock;

use clap::Arg;
use clap::ArgAction;
use clap::ColorChoice;
use clap::Command;
use log::Level;

static LONG_VERSION: LazyLock<String> = LazyLock::new(|| crate::version::berlin().to_string());
static SHORT_VERSION: LazyLock<String> = LazyLock::new(|| {
    crate::version::berlin()
        .split('+')
        .next()
        .unwrap_or_default()
        .to_string()
});

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildFlags {
    pub dry_run: bool,
    pub pipeline: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanFlags {
    pub json: bool,
    pub pipeline: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServeFlags {
    pub port: u16,
    pub watch: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BerlinSubcommand {
    Build(BuildFlags),
    Plan(PlanFlags),
    Serve(ServeFlags),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Flags {
    pub subcommand: BerlinSubcommand,
    pub log_level: Option<Level>,
}

fn clap_root() -> Command {
    Command::new("bln")
        .bin_name("bln")
        .color(ColorChoice::Never)
        .max_term_width(80)
        .version(SHORT_VERSION.as_str())
        .long_version(LONG_VERSION.as_str())
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            Arg::new("log-level")
                .short('L')
                .long("log-level")
                .help("Set log level")
                .hide(true)
                .value_parser(["debug", "info"])
                .global(true),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .help("Suppress diagnostic output")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        .subcommand(build_subcommand())
        .subcommand(plan_subcommand())
        .subcommand(serve_subcommand())
}

fn build_subcommand() -> Command {
    Command::new("build")
        .about("Execute a content pipeline")
        .arg(pipeline_arg("Select the pipeline to execute"))
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .help("Show effectful work without writing outputs")
                .action(ArgAction::SetTrue),
        )
}

fn plan_subcommand() -> Command {
    Command::new("plan")
        .about("Show the content pipeline without executing it")
        .arg(pipeline_arg("Select a named pipeline"))
        .arg(
            Arg::new("json")
                .long("json")
                .help("Print the pipeline as JSON")
                .action(ArgAction::SetTrue),
        )
}

fn serve_subcommand() -> Command {
    Command::new("serve")
        .about("Build and serve the website")
        .arg(
            Arg::new("port")
                .short('p')
                .long("port")
                .value_name("PORT")
                .help("Set the HTTP server port")
                .value_parser(clap::value_parser!(u16))
                .default_value("8081"),
        )
        .arg(
            Arg::new("watch")
                .short('w')
                .long("watch")
                .help("Watch project inputs and rebuild automatically")
                .action(ArgAction::SetTrue),
        )
}

fn pipeline_arg(help: &'static str) -> Arg {
    Arg::new("pipeline")
        .long("pipeline")
        .value_name("PIPELINE")
        .help(help)
        .default_value("site")
}

pub fn flags_from_vec(args: Vec<String>) -> clap::error::Result<Flags> {
    let matches = clap_root().try_get_matches_from(args)?;
    let log_level = if matches.get_flag("quiet") {
        Some(Level::Error)
    } else {
        match matches.get_one::<String>("log-level").map(String::as_str) {
            Some("debug") => Some(Level::Debug),
            Some("info") => Some(Level::Info),
            _ => None,
        }
    };

    let subcommand = match matches.subcommand().expect("subcommand is required") {
        ("build", args) => BerlinSubcommand::Build(BuildFlags {
            dry_run: args.get_flag("dry-run"),
            pipeline: selected_pipeline(args),
        }),
        ("plan", args) => BerlinSubcommand::Plan(PlanFlags {
            json: args.get_flag("json"),
            pipeline: selected_pipeline(args),
        }),
        ("serve", args) => BerlinSubcommand::Serve(ServeFlags {
            port: *args.get_one::<u16>("port").expect("port has a default"),
            watch: args.get_flag("watch"),
        }),
        _ => unreachable!("Clap only returns registered subcommands"),
    };

    Ok(Flags {
        subcommand,
        log_level,
    })
}

fn selected_pipeline(args: &clap::ArgMatches) -> String {
    args.get_one::<String>("pipeline")
        .expect("pipeline has a default")
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separate_site_config_flag_is_no_longer_accepted() {
        for command in ["build", "serve"] {
            let result = flags_from_vec(vec![
                "bln".into(),
                command.into(),
                "--config".into(),
                "berlin.toml".into(),
            ]);
            assert!(result.is_err());
        }
    }

    #[test]
    fn serve_watch_is_an_explicit_boolean_flag() {
        let without_watch = flags_from_vec(vec!["bln".into(), "serve".into()]).unwrap();
        let with_watch =
            flags_from_vec(vec!["bln".into(), "serve".into(), "--watch".into()]).unwrap();

        assert!(matches!(
            without_watch.subcommand,
            BerlinSubcommand::Serve(ServeFlags { watch: false, .. })
        ));
        assert!(matches!(
            with_watch.subcommand,
            BerlinSubcommand::Serve(ServeFlags { watch: true, .. })
        ));
    }

    #[test]
    fn quiet_is_a_boolean_flag() {
        let flags = flags_from_vec(vec!["bln".into(), "--quiet".into(), "build".into()]).unwrap();
        assert_eq!(flags.log_level, Some(Level::Error));
    }

    #[test]
    fn build_dry_run_is_an_explicit_boolean_flag() {
        let flags = flags_from_vec(vec![
            "bln".into(),
            "build".into(),
            "--pipeline".into(),
            "org".into(),
            "--dry-run".into(),
        ])
        .unwrap();

        assert!(matches!(
            flags.subcommand,
            BerlinSubcommand::Build(BuildFlags {
                dry_run: true,
                pipeline,
            }) if pipeline == "org"
        ));
    }
}
