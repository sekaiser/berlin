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
pub struct CheckFlags {
    pub json: bool,
    pub pipeline: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServeFlags {
    pub port: u16,
    pub watch: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseFlags {
    pub pipeline: String,
    pub from_directory: Option<std::path::PathBuf>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishFlags {
    pub release: String,
    pub confirm: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationFlags {
    pub release: String,
    pub refresh: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BerlinSubcommand {
    Release(ReleaseFlags),
    ReleasePlan(String),
    Publish(PublishFlags),
    Publication(PublicationFlags),
    Build(BuildFlags),
    Plan(PlanFlags),
    Check(CheckFlags),
    Serve(ServeFlags),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Flags {
    pub pipeline_files: Vec<std::path::PathBuf>,
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
        .arg(pipeline_file_arg())
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
        .subcommand(check_subcommand())
        .subcommand(serve_subcommand())
        .subcommand(
            Command::new("release")
                .about("Build and seal a website release without replacing preview output")
                .arg(pipeline_file_arg())
                .arg(pipeline_arg("Select a website pipeline"))
                .arg(
                    Arg::new("from-directory")
                        .long("from-directory")
                        .value_name("DIRECTORY")
                        .value_parser(clap::value_parser!(std::path::PathBuf))
                        .help(
                            "Seal prepared website files without building; relative to the project",
                        ),
                ),
        )
        .subcommand(
            Command::new("release-plan")
                .about("Verify a sealed release and show its files and destination; no network")
                .arg(release_arg()),
        )
        .subcommand(
            Command::new("publish")
                .about("Publish a reviewed release to its declared GitHub Pages branch")
                .arg(release_arg())
                .arg(
                    Arg::new("confirm")
                        .long("confirm")
                        .value_name("RELEASE_ID")
                        .required(true),
                ),
        )
        .subcommand(
            Command::new("publication")
                .about("Show a release's publication record; optionally refresh from GitHub")
                .arg(release_arg())
                .arg(
                    Arg::new("refresh")
                        .long("refresh")
                        .action(ArgAction::SetTrue),
                ),
        )
}

fn release_arg() -> Arg {
    Arg::new("release").value_name("RELEASE_ID").required(true)
}

fn build_subcommand() -> Command {
    Command::new("build")
        .arg(pipeline_file_arg())
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
        .arg(pipeline_file_arg())
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
        .arg(pipeline_file_arg())
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

fn check_subcommand() -> Command {
    Command::new("check")
        .arg(pipeline_file_arg())
        .about("Inspect authored document connections without publishing")
        .long_about("Check the documents of one website output after parsing and mappings. Reads existing Markdown; never exports Org or writes build outputs. Editorial observations do not fail the check. This is not a full site or rendered-link validation.")
        .arg(pipeline_arg("Select a pipeline with one website output"))
        .arg(Arg::new("json")
            .long("json")
            .help("Print a structured authoring report")
            .action(ArgAction::SetTrue))
}

// Register separately at both levels: Clap's global propagation replaces rather
// than appends values when the option occurs before AND after the subcommand.
fn pipeline_file_arg() -> Arg {
    Arg::new("pipeline-file")
        .long("pipeline-file")
        .value_name("FILE")
        .value_parser(clap::value_parser!(std::path::PathBuf))
        .action(ArgAction::Append)
        .help("Rhai program file; repeat to combine files in order (default: berlin.pipeline.rhai)")
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
        ("release", args) => BerlinSubcommand::Release(ReleaseFlags {
            pipeline: selected_pipeline(args),
            from_directory: args
                .get_one::<std::path::PathBuf>("from-directory")
                .cloned(),
        }),
        ("release-plan", args) => {
            BerlinSubcommand::ReleasePlan(args.get_one::<String>("release").unwrap().clone())
        }
        ("publish", args) => BerlinSubcommand::Publish(PublishFlags {
            release: args.get_one::<String>("release").unwrap().clone(),
            confirm: args.get_one::<String>("confirm").unwrap().clone(),
        }),
        ("publication", args) => BerlinSubcommand::Publication(PublicationFlags {
            release: args.get_one::<String>("release").unwrap().clone(),
            refresh: args.get_flag("refresh"),
        }),
        ("build", args) => BerlinSubcommand::Build(BuildFlags {
            dry_run: args.get_flag("dry-run"),
            pipeline: selected_pipeline(args),
        }),
        ("plan", args) => BerlinSubcommand::Plan(PlanFlags {
            json: args.get_flag("json"),
            pipeline: selected_pipeline(args),
        }),
        ("check", args) => BerlinSubcommand::Check(CheckFlags {
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
        pipeline_files: [
            &matches,
            matches.subcommand().expect("subcommand is required").1,
        ]
        .into_iter()
        .flat_map(|args| {
            args.try_get_many::<std::path::PathBuf>("pipeline-file")
                .ok()
                .flatten()
                .into_iter()
                .flatten()
        })
        .cloned()
        .collect(),
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
    fn pipeline_files_are_optional_repeatable_and_global() {
        assert!(
            flags_from_vec(vec!["bln".into(), "plan".into()])
                .unwrap()
                .pipeline_files
                .is_empty()
        );
        for args in [
            vec![
                "bln",
                "--pipeline-file",
                "shared.rhai",
                "plan",
                "--pipeline-file",
                "site.rhai",
            ],
            vec![
                "bln",
                "--pipeline-file",
                "shared.rhai",
                "--pipeline-file",
                "site.rhai",
                "plan",
            ],
            vec![
                "bln",
                "plan",
                "--pipeline-file",
                "shared.rhai",
                "--pipeline-file",
                "site.rhai",
            ],
        ] {
            let flags = flags_from_vec(args.into_iter().map(String::from).collect()).unwrap();
            assert_eq!(
                flags.pipeline_files,
                vec![std::path::PathBuf::from("shared.rhai"), "site.rhai".into()]
            );
        }
        assert!(
            flags_from_vec(vec!["bln".into(), "plan".into(), "--pipeline-file".into()]).is_err()
        );
    }

    #[test]
    fn check_defaults_to_site_and_accepts_json_and_pipeline() {
        let defaults = flags_from_vec(vec!["bln".into(), "check".into()]).unwrap();
        assert_eq!(
            defaults.subcommand,
            BerlinSubcommand::Check(CheckFlags {
                json: false,
                pipeline: "site".into()
            })
        );
        let flags = flags_from_vec(
            ["bln", "check", "--pipeline", "notebook", "--json"]
                .map(String::from)
                .to_vec(),
        )
        .unwrap();
        assert_eq!(
            flags.subcommand,
            BerlinSubcommand::Check(CheckFlags {
                json: true,
                pipeline: "notebook".into()
            })
        );
        assert!(flags_from_vec(["bln", "check", "--dry-run"].map(String::from).to_vec()).is_err());
    }

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
