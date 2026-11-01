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
    pub no_cache: bool,
    pub allow_tex: bool,
    pub dry_run: bool,
    pub production: bool,
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
    pub pipeline: String,
    pub no_cache: bool,
    pub allow_tex: bool,
    pub port: u16,
    pub watch: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseFlags {
    pub no_cache: bool,
    pub allow_tex: bool,
    pub pipeline: String,
    pub production: bool,
    pub from_directory: Option<std::path::PathBuf>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseDiffFlags {
    pub before: String,
    pub after: String,
    pub json: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseDraftsFlags {
    pub before: String,
    pub after: String,
    pub json: bool,
    pub output: Option<std::path::PathBuf>,
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
    State(super::state::Action),
    RenderDiagram(super::diagram::Options),
    Announcement(super::announcement::Action),
    Release(ReleaseFlags),
    ReleasePlan(String),
    ReleaseDiff(ReleaseDiffFlags),
    ReleaseDrafts(ReleaseDraftsFlags),
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
        .subcommand(super::state::command())
        .subcommand(super::announcement::command())
        .subcommand(super::diagram::command())
        .subcommand(
            Command::new("release")
                .arg(no_cache_arg().conflicts_with("from-directory"))
                .arg(allow_tex_arg().conflicts_with("from-directory"))
                .about("Build and seal a website release without replacing preview output")
                .arg(pipeline_file_arg())
                .arg(pipeline_arg("Select a website pipeline"))
                .arg(production_arg().conflicts_with("from-directory"))
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
            Command::new("release-diff")
                .about(
                    "Verify and compare two sealed releases locally; no builds, writes, or network",
                )
                .arg(Arg::new("before").value_name("BEFORE_ID").required(true))
                .arg(Arg::new("after").value_name("AFTER_ID").required(true))
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Print a structured release comparison")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("release-drafts")
                .about("Preview local LinkedIn announcement drafts from two sealed releases")
                .long_about("Draft title-and-link starting points for added or content-modified documents. Requires two native releases for the same website. Does not summarize changes, check live status, or publish. Review and edit before posting manually.")
                .arg(Arg::new("before").value_name("BEFORE_ID").required(true))
                .arg(Arg::new("after").value_name("AFTER_ID").required(true))
                .arg(Arg::new("json").long("json").help("Print the structured draft plan").action(ArgAction::SetTrue))
                .arg(Arg::new("output").long("output").value_name("NEW_DIRECTORY")
                    .value_parser(clap::value_parser!(std::path::PathBuf))
                    .help("Export editable text and a manifest; parent must exist, destination must not")),
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
        .arg(no_cache_arg())
        .arg(allow_tex_arg())
        .arg(pipeline_file_arg())
        .about("Execute a content pipeline")
        .arg(pipeline_arg("Select the pipeline to execute"))
        .arg(production_arg())
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .help("Show effectful work without writing outputs")
                .action(ArgAction::SetTrue),
        )
}

fn production_arg() -> Arg {
    Arg::new("production")
        .long("production")
        .help("Prepare and check website URLs, asset versions, and sitemap before completing the build")
        .action(ArgAction::SetTrue)
}

fn allow_tex_arg() -> Arg {
    Arg::new("allow-tex").long("allow-tex").action(ArgAction::SetTrue)
        .help("Allow pipeline LaTeX compilation for this invocation; trust sources and toolchains, TeX is not sandboxed")
}

fn no_cache_arg() -> Arg {
    Arg::new("no-cache")
        .long("no-cache")
        .action(ArgAction::SetTrue)
        .help("Recompute operation results without reading or writing the build cache")
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
        .arg(pipeline_arg("Select a website pipeline to preview"))
        .arg(no_cache_arg())
        .arg(allow_tex_arg())
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
        ("render-diagram", args) => BerlinSubcommand::RenderDiagram(super::diagram::parse(args)),
        ("state", args) => BerlinSubcommand::State(super::state::parse(args)),
        ("announcement", args) => BerlinSubcommand::Announcement(super::announcement::parse(args)),
        ("release", args) => BerlinSubcommand::Release(ReleaseFlags {
            no_cache: args.get_flag("no-cache"),
            allow_tex: args.get_flag("allow-tex"),
            pipeline: selected_pipeline(args),
            production: args.get_flag("production"),
            from_directory: args
                .get_one::<std::path::PathBuf>("from-directory")
                .cloned(),
        }),
        ("release-plan", args) => {
            BerlinSubcommand::ReleasePlan(args.get_one::<String>("release").unwrap().clone())
        }
        ("release-diff", args) => BerlinSubcommand::ReleaseDiff(ReleaseDiffFlags {
            before: args.get_one::<String>("before").unwrap().clone(),
            after: args.get_one::<String>("after").unwrap().clone(),
            json: args.get_flag("json"),
        }),
        ("release-drafts", args) => BerlinSubcommand::ReleaseDrafts(ReleaseDraftsFlags {
            before: args.get_one::<String>("before").unwrap().clone(),
            after: args.get_one::<String>("after").unwrap().clone(),
            json: args.get_flag("json"),
            output: args.get_one::<std::path::PathBuf>("output").cloned(),
        }),
        ("publish", args) => BerlinSubcommand::Publish(PublishFlags {
            release: args.get_one::<String>("release").unwrap().clone(),
            confirm: args.get_one::<String>("confirm").unwrap().clone(),
        }),
        ("publication", args) => BerlinSubcommand::Publication(PublicationFlags {
            release: args.get_one::<String>("release").unwrap().clone(),
            refresh: args.get_flag("refresh"),
        }),
        ("build", args) => BerlinSubcommand::Build(BuildFlags {
            no_cache: args.get_flag("no-cache"),
            allow_tex: args.get_flag("allow-tex"),
            dry_run: args.get_flag("dry-run"),
            production: args.get_flag("production"),
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
            pipeline: selected_pipeline(args),
            no_cache: args.get_flag("no-cache"),
            allow_tex: args.get_flag("allow-tex"),
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
    fn release_drafts_default_to_preview_and_accept_explicit_export() {
        let parse = |args: Vec<&str>| flags_from_vec(args.into_iter().map(String::from).collect());
        assert_eq!(
            parse(vec!["bln", "release-drafts", "old", "new"])
                .unwrap()
                .subcommand,
            BerlinSubcommand::ReleaseDrafts(ReleaseDraftsFlags {
                before: "old".into(),
                after: "new".into(),
                json: false,
                output: None
            })
        );
        assert_eq!(
            parse(vec![
                "bln",
                "release-drafts",
                "old",
                "new",
                "--json",
                "--output",
                "review"
            ])
            .unwrap()
            .subcommand,
            BerlinSubcommand::ReleaseDrafts(ReleaseDraftsFlags {
                before: "old".into(),
                after: "new".into(),
                json: true,
                output: Some("review".into())
            })
        );
        for args in [
            vec!["bln", "release-drafts"],
            vec!["bln", "release-drafts", "old"],
            vec!["bln", "release-drafts", "old", "new", "extra"],
            vec!["bln", "release-drafts", "old", "new", "--json=true"],
            vec!["bln", "release-drafts", "old", "new", "--output"],
            vec!["bln", "release-drafts", "old", "new", "--confirm", "new"],
        ] {
            assert!(parse(args).is_err());
        }
    }

    #[test]
    fn release_diff_requires_two_ordered_ids_and_supports_json() {
        for json in [false, true] {
            let mut args = vec!["bln", "release-diff", "before", "after"];
            if json {
                args.push("--json");
            }
            let flags = flags_from_vec(args.into_iter().map(String::from).collect()).unwrap();
            assert_eq!(
                flags.subcommand,
                BerlinSubcommand::ReleaseDiff(ReleaseDiffFlags {
                    before: "before".into(),
                    after: "after".into(),
                    json,
                })
            );
        }
        for args in [
            vec!["bln", "release-diff"],
            vec!["bln", "release-diff", "before"],
            vec!["bln", "release-diff", "before", "after", "extra"],
            vec!["bln", "release-diff", "before", "after", "--json=true"],
        ] {
            assert!(flags_from_vec(args.into_iter().map(String::from).collect()).is_err());
        }
    }

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
                ..
            }) if pipeline == "org"
        ));
    }
}
