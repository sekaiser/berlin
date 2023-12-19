use libs::url::Url;
use serde::Serialize;
use serde::Serializer;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;

pub type ModuleSpecifier = Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MediaType {
    Css,
    Csv,
    JsonFeedEntry,
    Html,
    Markdown,
    Org,
    Scss,
    Tera,
}

impl MediaType {
    pub fn as_extension(&self) -> &str {
        match self {
            Self::Css => "css",
            Self::Csv => "csv",
            Self::JsonFeedEntry => "feedentry",
            Self::Html => "html",
            Self::Markdown => "md",
            Self::Org => "org",
            Self::Scss => "scss",
            Self::Tera => "tera",
        }
    }

    fn from_path(path: &Path) -> Self {
        if let Some(os_str) = path.extension() {
            let lowercase_str = os_str.to_str().map(|s| s.to_lowercase());
            match lowercase_str.as_deref() {
                Some("css") => Self::Css,
                Some("csv") => Self::Csv,
                Some("feedentry") => Self::JsonFeedEntry,
                Some("html") => Self::Html,
                Some("md") => Self::Markdown,
                Some("org") => Self::Org,
                Some("scss") => Self::Scss,
                Some("tera") => Self::Tera,
                _ => unreachable!(),
            }
        } else {
            unreachable!()
        }
    }
}

impl Serialize for MediaType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Serialize::serialize(&self.to_string(), serializer)
    }
}

impl fmt::Display for MediaType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let value = match self {
            Self::Css => "Css",
            Self::Csv => "Csv",
            Self::JsonFeedEntry => "Feed Entry",
            Self::Html => "Html",
            Self::Markdown => "Markdown",
            Self::Org => "Org",
            Self::Scss => "Scss",
            Self::Tera => "Tera Template",
        };
        write!(f, "{}", value)
    }
}

impl<'a> From<&'a Path> for MediaType {
    fn from(path: &'a Path) -> Self {
        Self::from_path(path)
    }
}

impl<'a> From<&'a PathBuf> for MediaType {
    fn from(path: &'a PathBuf) -> Self {
        Self::from_path(path)
    }
}

impl<'a> From<&'a String> for MediaType {
    fn from(specifier: &'a String) -> Self {
        Self::from_path(&PathBuf::from(specifier))
    }
}

fn specifier_to_path(specifier: &ModuleSpecifier) -> PathBuf {
    if let Ok(path) = specifier.to_file_path() {
        path
    } else {
        specifier_path_to_path(specifier)
    }
}

fn specifier_path_to_path(specifier: &ModuleSpecifier) -> PathBuf {
    let path = specifier.path();
    if path.is_empty() {
        if let Some(domain) = specifier.domain() {
            PathBuf::from(domain)
        } else {
            PathBuf::from("")
        }
    } else {
        PathBuf::from(path)
    }
}

impl<'a> From<&'a ModuleSpecifier> for MediaType {
    fn from(specifier: &'a ModuleSpecifier) -> Self {
        let path = specifier_to_path(specifier);
        Self::from_path(&path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Normalize all intermediate components of the path (ie. remove "./" and "../" components).
    /// Similar to `fs::canonicalize()` but doesn't resolve symlinks.
    ///
    /// Taken from Cargo
    /// https://github.com/rust-lang/cargo/blob/af307a38c20a753ec60f0ad18be5abed3db3c9ac/src/cargo/util/paths.rs#L60-L85
    fn normalize_path<P: AsRef<Path>>(path: P) -> PathBuf {
        use std::path::Component;

        let mut components = path.as_ref().components().peekable();
        let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek().cloned() {
            components.next();
            PathBuf::from(c.as_os_str())
        } else {
            PathBuf::new()
        };

        for component in components {
            match component {
                Component::Prefix(..) => unreachable!(),
                Component::RootDir => {
                    ret.push(component.as_os_str());
                }
                Component::CurDir => {}
                Component::ParentDir => {
                    ret.pop();
                }
                Component::Normal(c) => {
                    ret.push(c);
                }
            }
        }
        ret
    }

    /// Returns true if the input string starts with a sequence of characters
    /// that could be a valid URI scheme, like 'https:', 'git+ssh:' or 'data:'.
    ///
    /// According to RFC 3986 (https://tools.ietf.org/html/rfc3986#section-3.1),
    /// a valid scheme has the following format:
    ///   scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )
    ///
    /// We additionally require the scheme to be at least 2 characters long,
    /// because otherwise a windows path like c:/foo would be treated as a URL,
    /// while no schemes with a one-letter name actually exist.
    fn specifier_has_uri_scheme(specifier: &str) -> bool {
        let mut chars = specifier.chars();
        let mut len = 0usize;
        // The first character must be a letter.
        match chars.next() {
            Some(c) if c.is_ascii_alphabetic() => len += 1,
            _ => return false,
        }
        // Second and following characters must be either a letter, number,
        // plus sign, minus sign, or dot.
        loop {
            match chars.next() {
                Some(c) if c.is_ascii_alphanumeric() || "+-.".contains(c) => len += 1,
                Some(':') if len >= 2 => return true,
                _ => return false,
            }
        }
    }

    fn resolve_url(url_str: &str) -> ModuleSpecifier {
        ModuleSpecifier::parse(url_str).expect("Invalid url.")
    }

    fn resolve_path(path_str: &str) -> ModuleSpecifier {
        let path = std::env::current_dir().unwrap().join(path_str);
        let path = normalize_path(path);
        ModuleSpecifier::from_file_path(path).expect("Invalid path.")
    }

    fn resolve_url_or_path(specifier: &str) -> ModuleSpecifier {
        if specifier_has_uri_scheme(specifier) {
            resolve_url(specifier)
        } else {
            resolve_path(specifier)
        }
    }

    #[test]
    fn test_map_file_extension() {
        assert_eq!(MediaType::from(Path::new("foo/bar.html")), MediaType::Html);
        assert_eq!(
            MediaType::from(Path::new("foo/bar.md")),
            MediaType::Markdown
        );
        assert_eq!(MediaType::from(Path::new("foo/bar.org")), MediaType::Org);
    }

    #[test]
    fn test_from_specifier() {
        let fixtures = vec![
            ("file:///a/b/c.html", MediaType::Html),
            ("file:///a/b/c.md", MediaType::Markdown),
            ("file:///a/b/c.org", MediaType::Org),
        ];

        for (specifier, expected) in fixtures {
            let actual = resolve_url_or_path(specifier);
            assert_eq!(MediaType::from(&actual), expected);
        }
    }
}
