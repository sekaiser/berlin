use clap::{Arg, ArgAction, ArgMatches, Command};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    Start {
        article: PathBuf,
        audience: String,
        goal: String,
        thesis: String,
        prerequisites: String,
        editorial_brief: Option<PathBuf>,
    },
    Show {
        id: i64,
        json: bool,
    },
    List {
        document: Option<String>,
        limit: usize,
    },
    Packet(i64),
    Record {
        id: i64,
        file: PathBuf,
    },
    Respond {
        id: i64,
        finding: usize,
        disposition: String,
        response: String,
        evidence: Option<String>,
    },
    Advance {
        id: i64,
        reflection: String,
    },
}

fn id() -> Arg {
    Arg::new("id")
        .value_name("REVIEW_ID")
        .required(true)
        .value_parser(clap::value_parser!(i64).range(1..))
}
fn required(name: &'static str) -> Arg {
    let help = match name {
        "audience" => "Who is the intended reader? Include their experience or role",
        "goal" => "What should that reader understand or be able to decide?",
        "thesis" => "What central claim does the article defend?",
        "disposition" => "Your decision about this finding; deferrals remain visible",
        "response" => "Explain your reasoning, agreement or objection",
        "reflection" => "What did this step change or leave unresolved?",
        _ => unreachable!(),
    };
    Arg::new(name).long(name).required(true).help(help)
}
pub(super) fn command() -> Command {
    Command::new("review")
        .about("Review an article's audience, argument, trade-offs and evidence locally")
        .subcommand_required(true)
        .subcommand(Command::new("start")
            .about("Capture an Org/Markdown draft and its intended audience; requires SQLite")
            .arg(Arg::new("article").required(true).help("Saved Org/Markdown source; relative to the project or an absolute path").value_parser(clap::value_parser!(PathBuf)))
            .arg(required("audience")).arg(required("goal")).arg(required("thesis"))
            .arg(Arg::new("prerequisites").long("prerequisites").help("Knowledge you expect the reader to bring").default_value("Not specified; examine the assumed knowledge."))
            .arg(Arg::new("editorial-brief").long("editorial-brief").value_name("FILE")
                .help("Capture optional private writing principles with this review; UTF-8 text, at most 64 KiB")
                .value_parser(clap::value_parser!(PathBuf))))
        .subcommand(Command::new("show").about("Show the current step, findings, responses and revision status")
            .arg(id()).arg(Arg::new("json").long("json").action(ArgAction::SetTrue)))
        .subcommand(Command::new("list").about("List local reviews, newest first")
            .arg(Arg::new("document").long("document"))
            .arg(Arg::new("limit").long("limit").default_value("20").value_parser(clap::value_parser!(u16).range(1..=1000))))
        .subcommand(Command::new("packet").about("Print the captured draft and current-step instructions for a human or AI reviewer; no network")
            .arg(id()))
        .subcommand(Command::new("record").about("Validate and import a reviewer report for the current step")
            .arg(id()).arg(Arg::new("file").required(true).value_parser(clap::value_parser!(PathBuf))))
        .subcommand(Command::new("respond").about("Record or revise your response to a numbered finding")
            .arg(id()).arg(Arg::new("finding").required(true).value_parser(clap::value_parser!(u16).range(1..=5)))
            .arg(required("disposition").value_parser(["addressed", "disputed", "deferred"]))
            .arg(required("response")).arg(Arg::new("evidence").long("evidence")))
        .subcommand(Command::new("advance").about("Reflect on this step and move to the next; retains deferred findings")
            .arg(id()).arg(required("reflection")))
}
pub(super) fn parse(a: &ArgMatches) -> Action {
    let (name, a) = a.subcommand().unwrap();
    let text = |key: &str| a.get_one::<String>(key).unwrap().clone();
    let review_id = || *a.get_one::<i64>("id").unwrap();
    match name {
        "start" => Action::Start {
            article: a.get_one::<PathBuf>("article").unwrap().clone(),
            audience: text("audience"),
            goal: text("goal"),
            thesis: text("thesis"),
            prerequisites: text("prerequisites"),
            editorial_brief: a.get_one::<PathBuf>("editorial-brief").cloned(),
        },
        "show" => Action::Show {
            id: review_id(),
            json: a.get_flag("json"),
        },
        "list" => Action::List {
            document: a.get_one::<String>("document").cloned(),
            limit: *a.get_one::<u16>("limit").unwrap() as usize,
        },
        "packet" => Action::Packet(review_id()),
        "record" => Action::Record {
            id: review_id(),
            file: a.get_one::<PathBuf>("file").unwrap().clone(),
        },
        "respond" => Action::Respond {
            id: review_id(),
            finding: *a.get_one::<u16>("finding").unwrap() as usize,
            disposition: text("disposition"),
            response: text("response"),
            evidence: a.get_one::<String>("evidence").cloned(),
        },
        "advance" => Action::Advance {
            id: review_id(),
            reflection: text("reflection"),
        },
        _ => unreachable!(),
    }
}
