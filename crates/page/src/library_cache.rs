use libs::parking_lot::Mutex;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::library::LibraryStore;
use crate::page::Page;

#[derive(Clone, Default, Debug)]
struct LibraryCacheSources(Arc<Mutex<HashMap<PathBuf, Page>>>);

impl LibraryStore for LibraryCacheSources {
    fn set_page(&self, path: PathBuf, page: Page) -> Option<Page> {
        self.0.lock().insert(path, page)
    }

    fn get_page(&self, path: &PathBuf) -> Option<Page> {
        self.0.lock().get(path).cloned()
    }
}

#[derive(Clone, Debug)]
pub struct LibraryCache {
    _db_cache_path: Option<PathBuf>,
    sources: LibraryCacheSources,
}

impl LibraryCache {
    pub fn new(sql_cache_path: Option<PathBuf>) -> Self {
        Self {
            _db_cache_path: sql_cache_path,
            sources: Default::default(),
        }
    }

    /// Frees the parsed source from memory.
    pub fn free(&self, path: &PathBuf) {
        self.sources.0.lock().remove(path);
    }
}
