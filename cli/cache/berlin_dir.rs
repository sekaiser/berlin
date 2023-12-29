use std::path::PathBuf;

use libs::once_cell::sync::OnceCell;

// Lazily creates the berlin dir which might be useful in scenarios
/// where functionality wants to continue if the BERLIN_DIR can't be created.
pub struct BerlinDirProvider {
    maybe_custom_root: Option<PathBuf>,
    berlin_dir: OnceCell<std::io::Result<BerlinDir>>,
}

impl BerlinDirProvider {
    pub fn new(maybe_custom_root: Option<PathBuf>) -> Self {
        Self {
            maybe_custom_root,
            berlin_dir: Default::default(),
        }
    }

    pub fn get_or_create(&self) -> Result<&BerlinDir, std::io::Error> {
        self.berlin_dir
            .get_or_init(|| BerlinDir::new(self.maybe_custom_root.clone()))
            .as_ref()
            .map_err(|err| std::io::Error::new(err.kind(), err.to_string()))
    }
}

#[derive(Clone)]
pub struct BerlinDir {
    root: PathBuf,
}

impl BerlinDir {
    pub fn new(maybe_custom_root: Option<PathBuf>) -> std::io::Result<Self> {
        let root = if let Some(root) = maybe_custom_root {
            root
        } else {
            std::env::current_dir()?
        };
        let root = if root.is_absolute() {
            root
        } else {
            std::env::current_dir()?.join(root)
        };
        assert!(root.is_absolute());
        let deno_dir = Self { root };

        Ok(deno_dir)
    }

    pub fn root_file_path(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn static_file_path(&self) -> PathBuf {
        self.root.join("static")
    }

    // TODO rename to layouts_file_path
    pub fn templates_file_path(&self) -> PathBuf {
        self.root.join("pages")
    }

    pub fn content_file_path(&self) -> PathBuf {
        self.root.join("content")
    }

    pub fn css_file_path(&self) -> PathBuf {
        self.root.join("css")
    }

    pub fn assets_file_path(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn data_file_path(&self) -> PathBuf {
        self.root.join("data")
    }

    pub fn target_file_path(&self) -> PathBuf {
        self.root.join("target")
    }
}
