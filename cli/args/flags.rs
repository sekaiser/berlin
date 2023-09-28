use libs::clap;
use libs::clap::ColorChoice;
use libs::clap::ValueHint;
use libs::clap::{Arg, Command};
use libs::log::Level;
use libs::once_cell::sync::Lazy;
use std::path::PathBuf;

static LONG_VERSION: Lazy<String> = Lazy::new(|| format!("{}", crate::version::berlin(),));

static SHORT_VERSION: Lazy<String> = Lazy::new(|| {
    crate::version::berlin()
        .split('+')
        .next()
        .unwrap()
        .to_string()
});

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildFlags {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitFlags {
    pub dir: Option<String>,
    // pub theme: Option<String>,
    // pub theme_location: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServeFlags {
    pub dir: Option<String>,
    pub port: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InfoFlags {
    pub json: bool,
    pub file: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BerlinSubcommand {
    Build(BuildFlags),
    Init(InitFlags),
    Info(InfoFlags),
    Serve(ServeFlags),
}

impl Default for BerlinSubcommand {
    fn default() -> BerlinSubcommand {
        BerlinSubcommand::Info(InfoFlags {
            json: false,
            file: None,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigFlag {
    Discover,
    Path(String),
}

impl Default for ConfigFlag {
    fn default() -> Self {
        Self::Discover
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Flags {
    pub subcommand: BerlinSubcommand,
    pub cache_path: Option<PathBuf>,
    pub config_flag: ConfigFlag,
    pub log_level: Option<Level>,
    pub watch: Option<Vec<PathBuf>>,
}

impl Flags {
    // Extract path arguments for config search paths.
    /// If it returns Some(vec), the config should be discovered
    /// from the current dir after trying to discover from each entry in vec.
    /// If it returns None, the config file shouldn't be discovered at all.
    pub fn config_path_args(&self) -> Option<Vec<PathBuf>> {
        Some(vec![])
        // if let Ok(module_specifier) = berlin_core::resolve_url_or_path(script) {
        //     if module_specifier.scheme() == "file" {
        //         if let Ok(p) = module_specifier.to_file_path() {
        //             Some(vec![p])
        //         } else {
        //             Some(vec![])
        //         }
        //     } else {
        //         // When the entrypoint doesn't have file: scheme (it's the remote
        //         // script), then we don't auto discover config file.
        //         None
        //     }
        // } else {
        //     Some(vec![])
        // }
    }
}

fn clap_root() -> Command {
    Command::new("bln")
        .bin_name("bln")
        .color(ColorChoice::Never)
        .max_term_width(80)
        .version(SHORT_VERSION.as_str())
        .long_version(LONG_VERSION.as_str())
        .arg(
            Arg::new("log-level")
                .short('L')
                .long("log-level")
                .help("Set log level")
                .hide(true)
                .num_args(1)
                .value_parser(["debug", "info"])
                .global(true),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .help("Suppress diagnostic output")
                .global(true),
        )
        .subcommand(init_subcommand())
        .subcommand(build_subcommand())
        .subcommand(serve_subcommand())
}

fn init_subcommand() -> Command {
    Command::new("init").about("Initialize a new project").arg(
        Arg::new("dir")
            .num_args(1)
            .required(false)
            .value_hint(ValueHint::DirPath),
    )
}

fn build_subcommand() -> Command {
    Command::new("build")
        .about("Compile the data")
        .arg(config_arg())
}

fn serve_subcommand() -> Command {
    Command::new("serve")
        .about("Start the webserver.")
        .arg(config_arg())
        .arg(serve_arg())
        .arg(watch_arg())
}

fn config_arg<'a>() -> clap::Arg {
    Arg::new("config")
        .short('c')
        .long("config")
        .value_name("FILE")
        .help("Specify the configuration file")
        //.long_help(CONFIG_HELP.as_str())
        .num_args(1)
        .value_hint(ValueHint::FilePath)
}

fn serve_arg<'a>() -> clap::Arg {
    Arg::new("port")
        .short('p')
        .long("port")
        .value_name("Port")
        .help("Specify the port the webserver should use.")
        //.long_help(CONFIG_HELP.as_str())
        .num_args(1)
        .value_hint(ValueHint::Unknown)
}

fn watch_arg<'a>() -> clap::Arg {
    let arg = Arg::new("watch")
        .short('w')
        .long("watch")
        .help("Watch for file changes and restart automatically");

    arg.value_name("FILES")
        .num_args(1)
        .required(false)
        .use_value_delimiter(true)
        .require_equals(true)
        .long_help(
            "Watch for file changes and restart process automatically.
Local files from entry point module graph are watched by default.
Additional paths might be watched by passing them as arguments to this flag.",
        )
        .value_hint(ValueHint::AnyPath)
}

/// Main entry point for parsing berlin's command line flags.
pub fn flags_from_vec(args: Vec<String>) -> clap::error::Result<Flags> {
    let mut app = clap_root();
    let matches = app.try_get_matches_from_mut(&args)?;

    let mut flags = Flags::default();

    if matches.contains_id("quiet") {
        flags.log_level = Some(Level::Error);
    } else {
        if let Some(log_level) = matches.get_one::<String>("log-level") {
            flags.log_level = match log_level.as_str() {
                "debug" => Some(Level::Debug),
                "info" => Some(Level::Info),
                _ => unreachable!(),
            }
        }
    }

    match matches.subcommand() {
        Some(("init", m)) => init(&mut flags, m),
        Some(("build", m)) => build(&mut flags, m),
        Some(("serve", m)) => serve(&mut flags, m),
        _ => {}
    }

    Ok(flags)
}

fn init(flags: &mut Flags, matches: &clap::ArgMatches) {
    flags.subcommand = BerlinSubcommand::Init(InitFlags {
        dir: matches.get_one::<String>("dir").map(|f| f.to_string()),
    });
}

fn build(flags: &mut Flags, matches: &clap::ArgMatches) {
    config_args_parse(flags, matches);
    flags.subcommand = BerlinSubcommand::Build(BuildFlags {});
}

fn serve(flags: &mut Flags, matches: &clap::ArgMatches) {
    serve_args_parse(flags, matches);
    watch_arg_parse(flags, matches, false);
    flags.subcommand = BerlinSubcommand::Serve(ServeFlags {
        dir: None,
        port: 8081,
    });
}

fn config_args_parse(flags: &mut Flags, matches: &clap::ArgMatches) {
    flags.config_flag = if let Some(config) = matches.get_one::<String>("config") {
        ConfigFlag::Path(config.to_string())
    } else {
        ConfigFlag::Discover
    };
}

fn serve_args_parse(flags: &mut Flags, matches: &clap::ArgMatches) {
    flags.config_flag = if let Some(config) = matches.get_one::<String>("config") {
        ConfigFlag::Path(config.to_string())
    } else {
        ConfigFlag::Discover
    };
}

fn watch_arg_parse(flags: &mut Flags, matches: &clap::ArgMatches, allow_extra: bool) {
    if allow_extra {
        if let Some(f) = matches.get_many::<String>("watch") {
            flags.watch = Some(f.map(PathBuf::from).collect());
        }
    } else if matches.contains_id("watch") {
        flags.watch = Some(vec![
            PathBuf::from("./pages"),
            PathBuf::from("./content"),
            PathBuf::from("./css"),
            PathBuf::from("./layouts"),
        ]);
    }
}
