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
const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(200);

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
    banner: &'static str,
    job_name: &'static str,
    clear_screen: bool,
}

impl PrintConfig {
    /// By default `PrintConfig` uses "Watcher" as a banner name that will
    /// be printed in color. If you need to customize it, use
    /// `PrintConfig::new_with_banner` instead.
    pub fn new(job_name: &'static str, clear_screen: bool) -> Self {
        Self {
            banner: "Watcher",
            job_name,
            clear_screen,
        }
    }

    pub fn new_with_banner(
        banner: &'static str,
        job_name: &'static str,
        clear_screen: bool,
    ) -> Self {
        Self {
            banner,
            job_name,
            clear_screen,
        }
    }
}

fn create_print_after_restart_fn(banner: &'static str, clear_screen: bool) -> impl Fn() {
    move || {
        if clear_screen && std::io::stderr().is_terminal() {
            eprint!("{CLEAR_SCREEN}");
        }
        info!(
            "{} File change detected! Restarting!",
            colors::intense_blue(banner),
        );
    }
}

/// An interface to interact with Deno's CLI file watcher.
#[derive(Debug)]
pub struct WatcherCommunicator {
    /// Send a list of paths that should be watched for changes.
    paths_to_watch_tx: tokio::sync::mpsc::UnboundedSender<Vec<PathBuf>>,

    /// Listen for a list of paths that were changed.
    changed_paths_rx: tokio::sync::broadcast::Receiver<Option<Vec<PathBuf>>>,

    /// Send a message to force a restart.
    restart_tx: tokio::sync::mpsc::UnboundedSender<()>,

    restart_mode: Mutex<WatcherRestartMode>,

    banner: String,
}

impl WatcherCommunicator {
    pub fn watch_paths(&self, paths: Vec<PathBuf>) -> Result<(), Error> {
        self.paths_to_watch_tx.send(paths).map_err(Error::from)
    }

    pub fn force_restart(&self) -> Result<Self, Error> {
        *self.restart_mode.lock() = WatcherRestartMode::Automatic;
        self.restart_tx.send(()).map_err(Error::from)
    }

    pub async fn watch_for_changed_paths(&self) -> Result<Option<Vec<PathBuf>>, AnyError> {
        let mut rx = self.changed_paths_rx.resubscribe();
        rx.recv().await.map_err(AnyError::from)
    }

    pub fn change_restart_mode(&self, restart_mode: WatcherRestartMode) {
        *self.restart_mode.lock() = restart_mode;
    }

    pub fn print(&self, msg: String) {
        log::info!("{} {}", self.banner, msg);
    }
}

#[derive(Clone, Copy, Debug)]
pub enum WatcherRestartMode {
    /// When a file path changes the process is restarted.
    Automatic,

    /// When a file path changes the caller will trigger a restart, using
    /// `WatcherInterface.restart_tx`.
    Manual,
}

pub async fn watch_func<O, F>(
    flags: Flags,
    print_config: PrintConfig,
    operation: O,
) -> Result<(), Error>
where
    O: FnMut(Flags, Arc<WatcherCommunicator>, Option<Vec<PathBuf>>) -> Result<F, Error>,
    F: Future<Output = Result<(), Error>>,
{
    let fut = watch_recv(
        flags,
        print_config,
        WatcherRestartMode::Automatic,
        operation,
    )
    .boxed_local();

    fut.await

    // let (watcher_sender, mut watcher_receiver) = DebouncedReceiver::new_with_sender();

    // let PrintConfig {
    //     job_name,
    //     clear_screen,
    // } = print_config;

    // let print_after_restart = create_print_after_restart_fn(clear_screen);

    // info!("{} {} started.", colors::intense_blue("Watcher"), job_name);

    // let mut watcher = new_watcher(watcher_sender.clone())?;
    // loop {
    //     match paths_to_watch_receiver.try_recv() {
    //         Ok(paths) => {
    //             add_paths_to_watcher(&mut watcher, &paths);
    //         }
    //         Err(e) => match e {
    //             mpsc::error::TryRecvError::Empty => {
    //                 break;
    //             }

    //             _ => unreachable!(),
    //         },
    //     }
    // }

    // while let Some(res) = watcher_receiver.recv().await {
    //     //let operation_future = error_handler(operation(operation_args.clone())?);
    //     let args = resolve_url_or_path(res[0].to_str().unwrap())?;
    //     let operation_future = error_handler(operation(args)?);
    //     print_after_restart();

    //     select! {
    //         _ = operation_future => {
    //             log::info!("{} {} done!",colors::intense_blue("Watcher"), job_name);
    //            }
    //     }
    // }

    // Ok(())
}

/// Creates a file watcher.
///
/// - `operation` is the actual operation we want to run every time the watcher detects file
/// changes.
pub async fn watch_recv<O, F>(
    mut flags: Flags,
    print_config: PrintConfig,
    restart_mode: WatcherRestartMode,
    mut operation: O,
) -> Result<(), AnyError>
where
    O: FnMut(Flags, Arc<WatcherCommunicator>, Option<Vec<PathBuf>>) -> Result<F, AnyError>,
    F: Future<Output = Result<(), AnyError>>,
{
    let (paths_to_watch_tx, mut paths_to_watch_rx) = tokio::sync::mpsc::unbounded_channel();
    let (restart_tx, mut restart_rx) = tokio::sync::mpsc::unbounded_channel();
    let (changed_paths_tx, changed_paths_rx) = tokio::sync::broadcast::channel(4);
    let (watcher_sender, mut watcher_receiver) = DebouncedReceiver::new_with_sender();

    let PrintConfig {
        banner,
        job_name,
        clear_screen,
    } = print_config;

    let print_after_restart = create_print_after_restart_fn(banner, clear_screen);
    let watcher_communicator = Arc::new(WatcherCommunicator {
        paths_to_watch_tx: paths_to_watch_tx.clone(),
        changed_paths_rx: changed_paths_rx.resubscribe(),
        restart_tx: restart_tx.clone(),
        restart_mode: Mutex::new(restart_mode),
        banner: colors::intense_blue(banner).to_string(),
    });
    info!("{} {} started.", colors::intense_blue(banner), job_name);

    let changed_paths = Rc::new(RefCell::new(None));
    let changed_paths_ = changed_paths.clone();
    let watcher_ = watcher_communicator.clone();

    berlin_core::unsync::spawn(async move {
        loop {
            let received_changed_paths = watcher_receiver.recv().await;
            *changed_paths_.borrow_mut() = received_changed_paths.clone();

            match *watcher_.restart_mode.lock() {
                WatcherRestartMode::Automatic => {
                    let _ = restart_tx.send(());
                }
                WatcherRestartMode::Manual => {
                    let _ = changed_paths_tx.send(received_changed_paths);
                }
            }
        }
    });

    loop {
        // We may need to give the runtime a tick to settle, as cancellations may need to propagate
        // to tasks. We choose yielding 10 times to the runtime as a decent heuristic. If watch tests
        // start to fail, this may need to be increased.
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }

        let mut watcher = new_watcher(watcher_sender.clone())?;
        consume_paths_to_watch(&mut watcher, &mut paths_to_watch_rx);

        let receiver_future = async {
            loop {
                let maybe_paths = paths_to_watch_rx.recv().await;
                add_paths_to_watcher(&mut watcher, &maybe_paths.unwrap());
            }
        };
        let operation_future = error_handler(operation(
            flags.clone(),
            watcher_communicator.clone(),
            changed_paths.borrow_mut().take(),
        )?);

        // don't reload dependencies after the first run
        flags.reload = false;

        select! {
          _ = receiver_future => {},
          _ = restart_rx.recv() => {
            print_after_restart();
            continue;
          },
          success = operation_future => {
            consume_paths_to_watch(&mut watcher, &mut paths_to_watch_rx);
            // TODO(bartlomieju): print exit code here?
            info!(
              "{} {} {}. Restarting on file change...",
              colors::intense_blue(banner),
              job_name,
              if success {
                "finished"
              } else {
                "failed"
              }
            );
          },
        };
        let receiver_future = async {
            loop {
                let maybe_paths = paths_to_watch_rx.recv().await;
                add_paths_to_watcher(&mut watcher, &maybe_paths.unwrap());
            }
        };

        // If we got this far, it means that the `operation` has finished; let's wait
        // and see if there are any new paths to watch received or any of the already
        // watched paths has changed.
        select! {
          _ = receiver_future => {},
          _ = restart_rx.recv() => {
            print_after_restart();
            continue;
          },
        };
    }
}

fn new_watcher(
    sender: Arc<mpsc::UnboundedSender<Vec<PathBuf>>>,
) -> Result<RecommendedWatcher, Error> {
    Ok(Watcher::new(
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
    )?)
}

fn add_paths_to_watcher(watcher: &mut RecommendedWatcher, paths: &[PathBuf]) {
    for path in paths {
        let _ = watcher.watch(path, RecursiveMode::Recursive);
    }
    log::debug!("Watching paths: {:?}", paths);
}

fn consume_paths_to_watch(
    watcher: &mut RecommendedWatcher,
    receiver: &mut UnboundedReceiver<Vec<PathBuf>>,
) {
    loop {
        match receiver.try_recv() {
            Ok(paths) => {
                add_paths_to_watcher(watcher, &paths);
            }
            Err(e) => match e {
                mpsc::error::TryRecvError::Empty => {
                    break;
                }
                // there must be at least one receiver alive
                _ => unreachable!(),
            },
        }
    }
}
