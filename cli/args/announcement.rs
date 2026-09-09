//! Manual announcement workflow arguments, separate from website deployment.
use clap::{Arg, ArgAction, ArgMatches, Command};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Source {
    pub directory: PathBuf,
    pub document: String,
    pub account: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
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
        .about("Review and track manually published LinkedIn announcements; no network")
        .subcommand_required(true)
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
                .help("Stable local account name, e.g. personal; not authentication"),
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
    }
}

pub(super) fn parse(args: &ArgMatches) -> Action {
    match args.subcommand().expect("required action") {
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
                    account: "personal".into()
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
