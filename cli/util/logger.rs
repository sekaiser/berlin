use libs::env_logger;
use libs::log;
use std::io::Write;

struct CliLogger(env_logger::Logger);

impl CliLogger {
    pub fn new(logger: env_logger::Logger) -> Self {
        Self(logger)
    }

    pub fn filter(&self) -> log::LevelFilter {
        self.0.filter()
    }
}

impl log::Log for CliLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.0.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            self.0.log(record);
        }
    }

    fn flush(&self) {
        self.0.flush();
    }
}

pub fn init(maybe_level: Option<log::Level>) {
    let log_level = maybe_level.unwrap_or(log::Level::Info);
    let logger = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(log_level.to_level_filter().to_string()),
    )
    .format(|buf, record| {
        let mut target = record.target().to_string();
        if let Some(line_no) = record.line() {
            target.push(':');
            target.push_str(&line_no.to_string());
        }
        if record.level() <= log::Level::Info
            || record.level() == log::Level::Warn
            || record.level() == log::Level::Error
        {
            writeln!(buf, "{}", record.args())
        } else {
            // Add prefix to DEBUG or TRACE logs
            writeln!(
                buf,
                "{} RS - {} - {}",
                record.level(),
                target,
                record.args()
            )
        }
    })
    .build();

    let cli_logger = CliLogger::new(logger);
    let max_level = cli_logger.filter();
    let r = log::set_boxed_logger(Box::new(cli_logger));
    if r.is_ok() {
        log::set_max_level(max_level);
    }
    r.expect("Could not install logger.");
}
