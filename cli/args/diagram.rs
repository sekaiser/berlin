//! Explicit authoring entry point for the shared graphics compiler.
use clap::{Arg, ArgAction, ArgMatches, Command};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Options {
    pub source: PathBuf,
    pub output: PathBuf,
    pub input_root: Option<PathBuf>,
    pub cache_root: Option<PathBuf>,
    pub engine: String,
    pub allow_tex: bool,
    pub no_cache: bool,
}
pub fn command() -> Command {
    Command::new("render-diagram")
        .about("Render trusted standalone LaTeX to SVG; does not publish or execute Org")
        .arg(
            Arg::new("source")
                .required(true)
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .required(true)
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("input-root")
                .long("input-root")
                .help("Snapshot this declared resource directory (preambles, fonts, data)")
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("cache-root")
                .long("cache-root")
                .help("Parent of local .berlin cache; defaults to the output directory")
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("engine")
                .long("engine")
                .value_parser(["pdflatex", "lualatex"])
                .default_value("pdflatex"),
        )
        .arg(
            Arg::new("allow-tex")
                .long("allow-tex")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-cache")
                .long("no-cache")
                .action(ArgAction::SetTrue),
        )
}
pub fn parse(args: &ArgMatches) -> Options {
    Options {
        source: args.get_one::<PathBuf>("source").unwrap().clone(),
        output: args.get_one::<PathBuf>("output").unwrap().clone(),
        input_root: args.get_one::<PathBuf>("input-root").cloned(),
        cache_root: args.get_one::<PathBuf>("cache-root").cloned(),
        engine: args.get_one::<String>("engine").unwrap().clone(),
        allow_tex: args.get_flag("allow-tex"),
        no_cache: args.get_flag("no-cache"),
    }
}
