use clap::{Arg, ArgMatches, Command};
use std::path::PathBuf;
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    Migrate,
    Status,
    Backup(PathBuf),
    Restore(PathBuf),
    Builds {
        pipeline: Option<String>,
        limit: usize,
    },
    Build(i64),
    ReleaseAttempts {
        pipeline: Option<String>,
        limit: usize,
    },
    ReleaseAttempt(i64),
    IndexReleases,
    Documents {
        limit: usize,
    },
    Document {
        id: String,
        limit: usize,
    },
    MetricDefinitions {
        limit: usize,
    },
    Metrics {
        document: Option<String>,
        limit: usize,
    },
}
pub(super) fn command() -> Command {
    Command::new("state")
        .about("Manage the local SQLite lifecycle store; no remote I/O")
        .subcommand_required(true)
        .subcommand(
            Command::new("migrate").about(
                "Validate and import JSON history; activate SQLite without deleting originals",
            ),
        )
        .subcommand(Command::new("status").about("Inspect backend and verify database integrity"))
        .subcommand(
            Command::new("release-attempts")
                .about("List captured release preparations, newest first")
                .arg(Arg::new("pipeline").long("pipeline"))
                .arg(history_limit()),
        )
        .subcommand(
            Command::new("release-attempt")
                .about("Inspect captured inputs and a sealed-release association")
                .arg(
                    Arg::new("id")
                        .required(true)
                        .value_parser(clap::value_parser!(i64).range(1..)),
                ),
        )
        .subcommand(
            Command::new("metric-definitions")
                .about("Inspect registered metric definitions; no collection")
                .arg(history_limit()),
        )
        .subcommand(
            Command::new("metrics")
                .about("Inspect recorded metric observations; no collection or imports")
                .arg(Arg::new("document").long("document"))
                .arg(history_limit()),
        )
        .subcommand(
            Command::new("index-releases").about("Verify and index sealed bundles; no remote I/O"),
        )
        .subcommand(
            Command::new("documents")
                .about("List document identities observed in sealed releases")
                .arg(history_limit()),
        )
        .subcommand(
            Command::new("document")
                .about("Inspect a document's sealed revisions and recorded publications")
                .arg(Arg::new("id").required(true))
                .arg(history_limit()),
        )
        .subcommand(
            Command::new("builds")
                .about("List recent build attempts, newest first")
                .arg(Arg::new("pipeline").long("pipeline"))
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .default_value("20")
                        .value_parser(clap::value_parser!(u16).range(1..=1000)),
                ),
        )
        .subcommand(
            Command::new("build")
                .about("Inspect a build receipt and observed catalog")
                .arg(
                    Arg::new("id")
                        .required(true)
                        .value_parser(clap::value_parser!(i64).range(1..)),
                ),
        )
        .subcommand(
            Command::new("backup")
                .about("Create a consistent database snapshot; destination must not exist")
                .arg(Arg::new("path").required(true)),
        )
        .subcommand(
            Command::new("restore")
                .about("Restore a validated snapshot into a project without a database")
                .arg(Arg::new("path").required(true)),
        )
}
fn history_limit() -> Arg {
    Arg::new("limit")
        .long("limit")
        .default_value("20")
        .value_parser(clap::value_parser!(u16).range(1..=1000))
}
pub(super) fn parse(args: &ArgMatches) -> Action {
    match args.subcommand().unwrap() {
        ("migrate", _) => Action::Migrate,
        ("status", _) => Action::Status,
        ("release-attempts", a) => Action::ReleaseAttempts {
            pipeline: a.get_one::<String>("pipeline").cloned(),
            limit: *a.get_one::<u16>("limit").unwrap() as usize,
        },
        ("release-attempt", a) => Action::ReleaseAttempt(*a.get_one::<i64>("id").unwrap()),
        ("metric-definitions", a) => Action::MetricDefinitions {
            limit: *a.get_one::<u16>("limit").unwrap() as usize,
        },
        ("metrics", a) => Action::Metrics {
            document: a.get_one::<String>("document").cloned(),
            limit: *a.get_one::<u16>("limit").unwrap() as usize,
        },
        ("index-releases", _) => Action::IndexReleases,
        ("documents", a) => Action::Documents {
            limit: *a.get_one::<u16>("limit").unwrap() as usize,
        },
        ("document", a) => Action::Document {
            id: a.get_one::<String>("id").unwrap().clone(),
            limit: *a.get_one::<u16>("limit").unwrap() as usize,
        },
        ("builds", a) => Action::Builds {
            pipeline: a.get_one::<String>("pipeline").cloned(),
            limit: *a.get_one::<u16>("limit").unwrap() as usize,
        },
        ("build", a) => Action::Build(*a.get_one::<i64>("id").unwrap()),
        ("backup", a) => Action::Backup(a.get_one::<String>("path").unwrap().into()),
        ("restore", a) => Action::Restore(a.get_one::<String>("path").unwrap().into()),
        _ => unreachable!(),
    }
}
