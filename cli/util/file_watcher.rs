use files::resolve_url_or_path;
use files::ModuleSpecifier;
use libs::anyhow::Error;
use libs::log;
use libs::log::info;
use libs::notify::event::Event as NotifyEvent;
use libs::notify::event::EventKind;
use libs::notify::Error as NotifyError;
use libs::notify::RecommendedWatcher;
use libs::notify::RecursiveMode;
use libs::notify::Watcher;
use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::colors;
use crate::util::fs::canonicalize_path;
use libs::tokio::select;
use libs::tokio::sync::mpsc;
use libs::tokio::sync::mpsc::UnboundedReceiver;
use libs::tokio::time::sleep;

const CLEAR_SCREEN: &str = "\x1B[2J\x1b[1;1H";
const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(1000);

struct DebouncedReceiver {
    received_items: HashSet<PathBuf>,
    receiver: UnboundedReceiver<Vec<PathBuf>>,
}

impl DebouncedReceiver {
    fn new_with_sender() -> (Arc<mpsc::UnboundedSender<Vec<PathBuf>>>, Self) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Arc::new(sender),
            Self {
                receiver,
                received_items: HashSet::new(),
            },
        )
    }

    async fn recv(&mut self) -> Option<Vec<PathBuf>> {
        if self.received_items.is_empty() {
            self.received_items
                .extend(self.receiver.recv().await?.into_iter());
        }

        loop {
            select! {
                items = self.receiver.recv() => {
                    self.received_items.extend(items?);
                }
                _ = sleep(DEBOUNCE_INTERVAL) => {
                    return Some(self.received_items.drain().collect());
                }
            }
        }
    }
}

async fn error_handler<F>(watch_future: F)
where
    F: Future<Output = Result<(), Error>>,
{
    let result = watch_future.await;
    if let Err(err) = result {
        eprintln!(
            "{}:{}",
            colors::red_bold("error"),
            err.to_string().trim_start_matches("error: ")
        );
    };
}

pub struct PrintConfig {
    pub job_name: String,
    pub clear_screen: bool,
}

fn create_print_after_restart_fn(clear_screen: bool) -> impl Fn() {
    move || {
        if clear_screen && libs::atty::is(libs::atty::Stream::Stderr) {
            eprint!("{CLEAR_SCREEN}");
        }
        info!("{} File change detected!", colors::intense_blue("Watcher"),);
    }
}

pub async fn watch_func2<O, F>(
    mut paths_to_watch_receiver: UnboundedReceiver<Vec<PathBuf>>,
    mut operation: O,
    print_config: PrintConfig,
) -> Result<(), Error>
where
    O: FnMut(ModuleSpecifier) -> Result<F, Error>,
    F: Future<Output = Result<(), Error>>,
{
    let (watcher_sender, mut watcher_receiver) = DebouncedReceiver::new_with_sender();

    let PrintConfig {
        job_name,
        clear_screen,
    } = print_config;

    let print_after_restart = create_print_after_restart_fn(clear_screen);

    info!("{} {} started.", colors::intense_blue("Watcher"), job_name);

    let mut watcher = new_watcher(watcher_sender.clone())?;
    loop {
        match paths_to_watch_receiver.try_recv() {
            Ok(paths) => {
                add_paths_to_watcher(&mut watcher, &paths);
            }
            Err(e) => match e {
                mpsc::error::TryRecvError::Empty => {
                    break;
                }

                _ => unreachable!(),
            },
        }
    }

    while let Some(res) = watcher_receiver.recv().await {
        //let operation_future = error_handler(operation(operation_args.clone())?);
        let args = resolve_url_or_path(res[0].to_str().unwrap())?;
        let operation_future = error_handler(operation(args)?);
        print_after_restart();

        select! {
            _ = operation_future => {
                log::info!("{} {} done!",colors::intense_blue("Watcher"), job_name);
               }
        }
    }

    Ok(())
}

fn new_watcher(
    sender: Arc<mpsc::UnboundedSender<Vec<PathBuf>>>,
) -> Result<RecommendedWatcher, Error> {
    let watcher = Watcher::new(
        move |res: Result<NotifyEvent, NotifyError>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                ) {
                    let paths = event
                        .paths
                        .iter()
                        .filter_map(|path| canonicalize_path(path).ok())
                        .collect();

                    sender.send(paths).unwrap();
                }
            }
        },
        Default::default(),
    )?;

    Ok(watcher)
}

fn add_paths_to_watcher(watcher: &mut RecommendedWatcher, paths: &[PathBuf]) {
    for path in paths {
        let _ = watcher.watch(path, RecursiveMode::Recursive);
    }
    log::debug!("Watching paths: {:?}", paths);
}
