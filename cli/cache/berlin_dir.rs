use std::path::PathBuf;

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
