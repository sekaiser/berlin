use std::path::PathBuf;

use crate::Page;

pub trait LibraryStore {
    /// Sets the parsed source, potentially returning the previous value.
    fn set_page(&self, path: PathBuf, page: Page) -> Option<Page>;

    fn get_page(&self, path: &PathBuf) -> Option<Page>;
}
