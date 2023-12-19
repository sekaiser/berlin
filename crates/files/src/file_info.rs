use std::path::{Path, PathBuf};

/// Takes a full path to a file and returns only the components after the first `content` directory
/// Will not return the filename as last component
pub fn find_content_components<P: AsRef<Path>>(path: P) -> Vec<String> {
    let path = path.as_ref();
    let mut is_in_content = false;
    let mut components = vec![];

    for section in path.parent().unwrap().components() {
        let component = section.as_os_str().to_string_lossy();

        if is_in_content {
            components.push(component.to_string());
            continue;
        }

        if component == "content" {
            is_in_content = true;
        }
    }

    components
}

/// Struct that contains all the information about the actual file
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FileInfo {
    /// The full path to the .md file
    pub path: PathBuf,
    /// The on-disk filename, will differ from the `name` when there is a language code in it
    pub filename: String,
    /// The name of the .md file without the extension, always `_index` for sections
    /// Doesn't contain the language if there was one in the filename
    pub name: String,
    /// The .md path, starting from the content directory, with `/` slashes
    pub relative: String,
    /// The path from the content directory to the colocated directory. Ends with a `/` when set.
    /// Only filled if it is a colocated directory, None otherwise.
    pub colocated_path: Option<String>,
    /// Path of the directory containing the .md file
    pub parent: PathBuf,
    /// Path of the grand parent directory for that file. Only used in sections to find subsections.
    pub grand_parent: Option<PathBuf>,
    /// The folder names to this section file, starting from the `content` directory
    /// For example a file at content/kb/solutions/blabla.md will have 2 components:
    /// `kb` and `solutions`
    pub components: Vec<String>,
    /// This is `parent` + `name`, used to find content referring to the same content but in
    /// various languages.
    pub canonical: PathBuf,
}

impl FileInfo {
    pub fn new_page(path: &Path, base_path: &Path) -> FileInfo {
        let file_path = path.to_path_buf();
        let mut parent = file_path
            .parent()
            .expect("Get parent of page")
            .to_path_buf();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let canonical = parent.join(&name);
        let mut components =
            find_content_components(file_path.strip_prefix(base_path).unwrap_or(&file_path));
        let relative = if !components.is_empty() {
            format!("{}/{}.md", components.join("/"), name)
        } else {
            format!("{}.md", name)
        };
        let mut colocated_path = None;

        // If we have a folder with an asset, don't consider it as a component
        // Splitting on `.` as we might have a language so it isn't *only* index but also index.fr
        // etc
        if !components.is_empty() && name.split('.').collect::<Vec<_>>()[0] == "index" {
            colocated_path = Some({
                let mut val = components.join("/");
                val.push('/');
                val
            });

            components.pop();
            // also set parent_path to grandparent instead
            parent = parent.parent().unwrap().to_path_buf();
        }

        FileInfo {
            filename: file_path.file_name().unwrap().to_string_lossy().to_string(),
            path: file_path,
            // We don't care about grand parent for pages
            grand_parent: None,
            canonical,
            parent,
            name,
            components,
            relative,
            colocated_path,
        }
    }
}
