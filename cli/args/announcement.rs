//! Channel preparation, approval and explicit DEV draft delivery arguments.
use clap::{Arg, ArgAction, ArgMatches, Command};
use std::path::PathBuf;
mod linkedin;
pub use linkedin::LinkedInAction;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Source {
    pub directory: PathBuf,
    pub document: String,
    pub account: String,
    pub channel: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    LinkedIn(LinkedInAction),
    Deliveries {
        review: Option<String>,
    },
    Prepare {
        release: String,
        document: String,
        output: PathBuf,
        updated: bool,
    },
    Deliver {
        source: Source,
        confirm: String,
        token_env: Option<String>,
    },
    Reconcile {
        review: String,
        draft: u64,
        token_env: String,
    },
    Review {
        source: Source,
        approve: Option<String>,
        json: bool,
    },
    Record {
        source: Source,
        url: String,
        confirm: String,
    },
    Status {
        directory: Option<PathBuf>,
        account: Option<String>,
        document: Option<String>,
        json: bool,
    },
}

pub(super) fn command() -> Command {
    Command::new("announcement")
        .about("Prepare and review channel exports; explicitly deliver approved publications")
        .subcommand_required(true)
        .subcommand(linkedin::command())
        .subcommand(Command::new("deliveries").about("Inspect local delivery attempts; no network").arg(Arg::new("review").long("review")))
        .subcommand(Command::new("prepare").about("Export summaries, teaser media, LinkedIn/X text and a DEV draft from a sealed release; no network")
            .arg(Arg::new("release").required(true)).arg(Arg::new("document").long("document").required(true))
            .arg(Arg::new("output").long("output").required(true).value_parser(clap::value_parser!(PathBuf)))
            .arg(Arg::new("updated").long("updated").action(ArgAction::SetTrue)))
        .subcommand(source_args(Command::new("deliver").about("Explicitly publish an approved LinkedIn announcement or create a DEV draft"))
            .arg(Arg::new("confirm").long("confirm").required(true))
            .arg(Arg::new("token-env").long("token-env")))
        .subcommand(Command::new("reconcile").about("Verify a DEV draft after an uncertain delivery; never creates a post")
            .arg(Arg::new("review").required(true)).arg(Arg::new("draft").long("draft").required(true).value_parser(clap::value_parser!(u64).range(1..)))
            .arg(Arg::new("token-env").long("token-env").default_value("DEVTO_API_KEY")))
        .subcommand(source_args(Command::new("review")
            .about("Show the current text and review ID; optionally approve those exact bytes"))
            .arg(Arg::new("approve").long("approve").value_name("REVIEW_ID"))
            .arg(json_arg()))
        .subcommand(source_args(Command::new("record")
            .about("Record your assertion that the approved text was posted; does not publish or verify it"))
            .arg(Arg::new("url").long("url").value_name("POST_URL").required(true))
            .arg(Arg::new("confirm").long("confirm").value_name("REVIEW_ID").required(true)))
        .subcommand(Command::new("status")
            .about("Show saved approvals and reported posts, optionally alongside an exported review")
            .arg(directory_arg().required(false).requires("account"))
            .arg(Arg::new("account").long("account").value_name("ACCOUNT"))
            .arg(Arg::new("document").long("document").value_name("CONTENT_ID"))
            .arg(json_arg()))
}

fn source_args(command: Command) -> Command {
    command
        .arg(
            Arg::new("channel")
                .long("channel")
                .default_value("linkedin")
                .value_parser(["linkedin", "x", "devto"]),
        )
        .arg(directory_arg())
        .arg(
            Arg::new("document")
                .long("document")
                .value_name("CONTENT_ID")
                .required(true),
        )
        .arg(
            Arg::new("account")
                .long("account")
                .value_name("ACCOUNT")
                .required(true)
                .help("Local label for manual channels; DEV requires your authenticated username"),
        )
}

fn directory_arg() -> Arg {
    Arg::new("directory")
        .value_name("REVIEW_DIRECTORY")
        .required(true)
        .value_parser(clap::value_parser!(PathBuf))
}

fn json_arg() -> Arg {
    Arg::new("json").long("json").action(ArgAction::SetTrue)
}

fn source(args: &ArgMatches) -> Source {
    Source {
        directory: args.get_one::<PathBuf>("directory").unwrap().clone(),
        document: args.get_one::<String>("document").unwrap().clone(),
        account: args.get_one::<String>("account").unwrap().clone(),
        channel: args.get_one::<String>("channel").unwrap().clone(),
    }
}

pub(super) fn parse(args: &ArgMatches) -> Action {
    match args.subcommand().expect("required action") {
        ("linkedin", args) => Action::LinkedIn(linkedin::parse(args)),
        ("deliveries", args) => Action::Deliveries {
            review: args.get_one::<String>("review").cloned(),
        },
        ("prepare", args) => Action::Prepare {
            release: args.get_one::<String>("release").unwrap().clone(),
            document: args.get_one::<String>("document").unwrap().clone(),
            output: args.get_one::<PathBuf>("output").unwrap().clone(),
            updated: args.get_flag("updated"),
        },
        ("deliver", args) => Action::Deliver {
            source: source(args),
            confirm: args.get_one::<String>("confirm").unwrap().clone(),
            token_env: args.get_one::<String>("token-env").cloned(),
        },
        ("reconcile", args) => Action::Reconcile {
            review: args.get_one::<String>("review").unwrap().clone(),
            draft: *args.get_one::<u64>("draft").unwrap(),
            token_env: args.get_one::<String>("token-env").unwrap().clone(),
        },
        ("review", args) => Action::Review {
            source: source(args),
            approve: args.get_one::<String>("approve").cloned(),
            json: args.get_flag("json"),
        },
        ("record", args) => Action::Record {
            source: source(args),
            url: args.get_one::<String>("url").unwrap().clone(),
            confirm: args.get_one::<String>("confirm").unwrap().clone(),
        },
        ("status", args) => Action::Status {
            directory: args.get_one::<PathBuf>("directory").cloned(),
            account: args.get_one::<String>("account").cloned(),
            document: args.get_one::<String>("document").cloned(),
            json: args.get_flag("json"),
        },
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(args: &[&str]) -> Result<Action, clap::Error> {
        command()
            .try_get_matches_from(args)
            .map(|args| parse(&args))
    }

    #[test]
    fn review_defaults_to_read_only_and_requires_a_named_document_and_account() {
        let args = [
            "announcement",
            "review",
            "review-dir",
            "--document",
            "note",
            "--account",
            "personal",
        ];
        let source = Source {
            directory: "review-dir".into(),
            document: "note".into(),
            account: "personal".into(),
            channel: "linkedin".into(),
        };
        assert_eq!(
            action(&args).unwrap(),
            Action::Review {
                source: source.clone(),
                approve: None,
                json: false
            }
        );
        let mut approval = args.to_vec();
        approval.extend(["--approve", "review-id", "--json"]);
        assert_eq!(
            action(&approval).unwrap(),
            Action::Review {
                source,
                approve: Some("review-id".into()),
                json: true
            }
        );
        for args in [
            vec!["announcement"],
            vec!["announcement", "review"],
            vec!["announcement", "review", "review-dir", "--document", "note"],
            vec![
                "announcement",
                "review",
                "review-dir",
                "--account",
                "personal",
            ],
        ] {
            assert!(action(&args).is_err());
        }
    }

    #[test]
    fn record_requires_both_url_and_explicit_confirmation() {
        let mut args = vec![
            "announcement",
            "record",
            "review-dir",
            "--document",
            "note",
            "--account",
            "personal",
        ];
        assert!(action(&args).is_err());
        args.extend(["--url", "https://www.linkedin.com/posts/example"]);
        assert!(action(&args).is_err());
        args.extend(["--confirm", "review-id"]);
        assert_eq!(
            action(&args).unwrap(),
            Action::Record {
                source: Source {
                    directory: "review-dir".into(),
                    document: "note".into(),
                    account: "personal".into(),
                    channel: "linkedin".into()
                },
                url: "https://www.linkedin.com/posts/example".into(),
                confirm: "review-id".into(),
            }
        );
    }

    #[test]
    fn status_can_read_history_alone_but_current_drafts_require_an_account() {
        assert_eq!(
            action(&["announcement", "status"]).unwrap(),
            Action::Status {
                directory: None,
                account: None,
                document: None,
                json: false,
            }
        );
        assert!(action(&["announcement", "status", "review-dir"]).is_err());
        assert_eq!(
            action(&[
                "announcement",
                "status",
                "review-dir",
                "--account",
                "personal",
                "--document",
                "note",
                "--json"
            ])
            .unwrap(),
            Action::Status {
                directory: Some("review-dir".into()),
                account: Some("personal".into()),
                document: Some("note".into()),
                json: true,
            }
        );
    }
}
