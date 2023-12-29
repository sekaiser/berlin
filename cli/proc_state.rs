use crate::args::CliOptions;
use crate::args::Flags;
use crate::cache::BerlinDir;
use crate::util::fs::load_files;
use berlin_core::Resolutions;
use berlin_core::ResolutionsBuilder;
use content::library_cache::LibraryCache;
use files::normalize_path;
use files::ModuleSpecifier;
use libs::anyhow::Error;
use libs::parking_lot::Mutex;
use parser::ParsedSource;
use parser::ParsedSourceCache;
use templates::Hera;

use core::fmt;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use libs::lightningcss::css_modules::{Config, Pattern};
use libs::lightningcss::rules::import::ImportRule;
use libs::lightningcss::rules::CssRule;
use libs::lightningcss::stylesheet::{ParserOptions, StyleSheet};
use libs::tera;
use libs::tokio;

#[derive(Clone)]
pub struct ProcState(Arc<Inner>);

pub struct Inner {
    pub dir: BerlinDir,
    pub options: Arc<CliOptions>,
    pub parsed_source_cache: ParsedSourceCache,
    pub library_cache: LibraryCache,
    maybe_file_watcher_reporter: Option<FileWatcherReporter>,
    pub maybe_css_resolutions: Option<Resolutions>,
    pub hera: Arc<Mutex<Hera>>,
}

impl Deref for ProcState {
    type Target = Arc<Inner>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ProcState {
    pub async fn build(flags: Flags) -> Result<Self, Error> {
        Self::from_options(Arc::new(CliOptions::from_flags(flags)?)).await
    }

    pub async fn from_options(options: Arc<CliOptions>) -> Result<Self, Error> {
        Self::build_with_sender(options, None).await
    }

    pub async fn build_for_file_watcher(
        flags: Flags,
        files_to_watch_sender: tokio::sync::mpsc::UnboundedSender<Vec<PathBuf>>,
    ) -> Result<Self, Error> {
        let cli_options = Arc::new(CliOptions::from_flags(flags)?);
        let ps = Self::build_with_sender(cli_options, Some(files_to_watch_sender.clone())).await?;
        ps.init_watcher();
        Ok(ps)
    }

    async fn build_with_sender(
        cli_options: Arc<CliOptions>,
        maybe_sender: Option<tokio::sync::mpsc::UnboundedSender<Vec<PathBuf>>>,
    ) -> Result<Self, Error> {
        let dir = cli_options.resolve_berlin_dir()?;
        let parsed_source_cache = ParsedSourceCache::new(None);
        let library_cache = LibraryCache::new(None);

        let maybe_file_watcher_reporter = maybe_sender.map(|sender| FileWatcherReporter {
            sender,
            file_paths: Arc::new(Mutex::new(vec![])),
        });

        let mut css_resolutions_builder = ResolutionsBuilder::new();
        for f in load_files(&dir.css_file_path(), "**/*.css") {
            let imports = read_css_imports(&f)?;
            css_resolutions_builder = css_resolutions_builder.add_rule(f, &imports);
        }
        let css_resolutions = css_resolutions_builder.build()?;
        let maybe_css_resolutions = if css_resolutions.is_empty() {
            None
        } else {
            Some(css_resolutions)
        };

        let hera = Hera::new(&dir.templates_file_path())?;

        Ok(ProcState(Arc::new(Inner {
            dir,
            options: cli_options,
            parsed_source_cache,
            library_cache,
            maybe_file_watcher_reporter,
            maybe_css_resolutions,
            hera: Arc::new(Mutex::new(hera)),
        })))
    }

    pub fn render_parsed_source_with_context(
        &self,
        file_path: &str,
        source: &ParsedSource,
        context: &tera::Context,
    ) -> String {
        let mut hera = self.hera.lock();
        hera.render_parsed_source_with_context(file_path, source, context)
    }

    pub fn render_with_context(&self, file_path: &str, context: &tera::Context) -> String {
        let mut hera = self.hera.lock();
        hera.render_with_context(file_path, context)
    }

    fn init_watcher(&self) {
        let files_to_watch_sender = match &self.0.maybe_file_watcher_reporter {
            Some(reporter) => &reporter.sender,
            None => return,
        };

        if let Some(watch_paths) = self.options.watch_paths() {
            files_to_watch_sender.send(watch_paths.clone()).unwrap();
        }
    }
}

fn read_css_imports(path_buf: &PathBuf) -> Result<Vec<PathBuf>, Error> {
    let parent = path_buf.parent().unwrap();
    let mut imports = Vec::new();
    let path_str = path_buf.to_str().expect("Invalid path");
    let data = std::fs::read_to_string(path_str).expect("Unable to read file");
    let stylesheet = StyleSheet::parse(
        &data,
        ParserOptions {
            css_modules: Some(Config {
                pattern: Pattern::parse("[local]")?,
                dashed_idents: true,
            }),
            ..ParserOptions::default()
        },
    )
    .unwrap();

    for rule in stylesheet.rules.0 {
        match rule {
            CssRule::Import(ImportRule { url, .. }) => {
                let p: &str = &url;
                imports.push(normalize_path(parent.join(p)))
            }
            _ => {}
        }
    }

    Ok(imports)
}

/// A trait which can be used to report status events to the user.
pub trait Reporter: fmt::Debug {
    // A handler which is called after each load of a file.
    // The returned future is ready.
    fn on_load(&self, specifier: &ModuleSpecifier, files_done: usize, files_total: usize);
}

#[derive(Clone, Debug)]
struct FileWatcherReporter {
    sender: tokio::sync::mpsc::UnboundedSender<Vec<PathBuf>>,
    file_paths: Arc<Mutex<Vec<PathBuf>>>,
}

impl Reporter for FileWatcherReporter {
    fn on_load(&self, specifier: &ModuleSpecifier, files_done: usize, files_total: usize) {
        let mut file_paths = self.file_paths.lock();
        if specifier.scheme() == "file" {
            file_paths.push(specifier.to_file_path().unwrap());
        }

        if files_done == files_total {
            self.sender.send(file_paths.drain(..).collect()).unwrap();
        }
    }
}
