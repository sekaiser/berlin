use std::collections::HashSet;
use std::future::Future;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context as _;
use anyhow::Error;
use log::info;
use notify::RecommendedWatcher;
use notify::RecursiveMode;
use notify::Watcher;
use notify::event::EventKind;
use tokio::select;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::sleep;

use crate::colors;

const CLEAR_SCREEN: &str = "\x1B[H\x1B[2J\x1B[3J";
const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(1000);

struct DebouncedReceiver {
    received_paths: HashSet<PathBuf>,
    receiver: UnboundedReceiver<notify::Result<Vec<PathBuf>>>,
}

impl DebouncedReceiver {
    fn new_with_sender() -> (mpsc::UnboundedSender<notify::Result<Vec<PathBuf>>>, Self) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            sender,
            Self {
                receiver,
                received_paths: HashSet::new(),
            },
        )
    }

    async fn recv(&mut self) -> notify::Result<Option<Vec<PathBuf>>> {
        if self.received_paths.is_empty() {
            let Some(paths) = self.receiver.recv().await else {
                return Ok(None);
            };
            self.received_paths.extend(paths?);
        }
        loop {
            select! {
                paths = self.receiver.recv() => match paths {
                    Some(paths) => self.received_paths.extend(paths?),
                    None => return Ok(None),
                },
                _ = sleep(DEBOUNCE_INTERVAL) => {
                    return Ok(Some(self.received_paths.drain().collect()));
                }
            }
        }
    }
}

pub struct PrintConfig {
    banner: &'static str,
    job_name: &'static str,
    clear_screen: bool,
}

impl PrintConfig {
    pub fn new(banner: &'static str, job_name: &'static str, clear_screen: bool) -> Self {
        Self {
            banner,
            job_name,
            clear_screen,
        }
    }
}

pub async fn watch_recv<O, F>(
    paths_to_watch: Vec<PathBuf>,
    print_config: PrintConfig,
    mut operation: O,
) -> Result<(), Error>
where
    O: FnMut(Option<Vec<PathBuf>>) -> Result<F, Error>,
    F: Future<Output = Result<(), Error>>,
{
    let (event_tx, mut events) = DebouncedReceiver::new_with_sender();
    let mut watcher = new_watcher(event_tx)?;
    let mut watched_paths = HashSet::new();
    add_paths_to_watcher(&mut watcher, &paths_to_watch, &mut watched_paths)?;
    let mut changed_paths: Option<Vec<PathBuf>> = None;

    info!(
        "{} {} started.",
        colors::intense_blue(print_config.banner),
        print_config.job_name
    );

    loop {
        if let Some(paths) = &changed_paths {
            let message = paths
                .first()
                .map(|path| format!("Rebuilding after change to {path:?}"))
                .unwrap_or_else(|| "Rebuilding after file change".into());
            log::info!(
                "{} {}",
                colors::intense_blue(print_config.banner),
                colors::gray(message)
            );
        }

        let success = match operation(changed_paths.take()) {
            Ok(future) => match future.await {
                Ok(()) => true,
                Err(error) => {
                    eprintln!(
                        "{}: {}",
                        colors::red_bold("error"),
                        error.to_string().trim_start_matches("error: ")
                    );
                    false
                }
            },
            Err(error) => return Err(error),
        };

        info!(
            "{} {} {}. Watching for changes...",
            colors::intense_blue(print_config.banner),
            print_config.job_name,
            if success { "finished" } else { "failed" }
        );

        changed_paths = select! {
            paths = events.recv() => paths?,
            _ = tokio::signal::ctrl_c() => return Ok(()),
        };

        if print_config.clear_screen && std::io::stderr().is_terminal() {
            eprint!("{}", CLEAR_SCREEN);
        }
    }
}

fn new_watcher(
    sender: mpsc::UnboundedSender<notify::Result<Vec<PathBuf>>>,
) -> Result<RecommendedWatcher, Error> {
    Ok(Watcher::new(
        move |result: notify::Result<notify::Event>| {
            let paths = match result {
                Ok(event)
                    if matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    ) =>
                {
                    Some(Ok(event.paths))
                }
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            };
            if let Some(paths) = paths {
                let _ = sender.send(paths);
            }
        },
        Default::default(),
    )?)
}

fn add_paths_to_watcher(
    watcher: &mut RecommendedWatcher,
    paths: &[PathBuf],
    watched_paths: &mut HashSet<PathBuf>,
) -> Result<(), Error> {
    for path in paths {
        if watched_paths.insert(path.clone()) {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .map_err(Error::from)
                .with_context(|| format!("Unable to watch {}", path.display()))?;
        }
    }
    log::debug!("Watching paths: {:?}", watched_paths);
    Ok(())
}
