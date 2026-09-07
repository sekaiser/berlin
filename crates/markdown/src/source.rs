//! Validated Markdown input and source identity.

use url::Url;

use crate::Error;

/// Markdown text together with the identity used for provenance and link resolution.
#[derive(Clone, Debug)]
pub struct Source<'a> {
    text: &'a str,
    uri: Url,
}

impl<'a> Source<'a> {
    /// Creates a source after validating its absolute URI.
    pub fn new(text: &'a str, uri: impl Into<String>) -> Result<Self, Error> {
        let source_uri = uri.into();
        match Url::parse(&source_uri) {
            Ok(uri) => Ok(Self { text, uri }),
            Err(source) => Err(Error::InvalidSourceUri { source_uri, source }),
        }
    }

    /// Creates a source whose identity is local to the current process.
    pub fn in_memory(text: &'a str) -> Self {
        Self {
            text,
            uri: Url::parse("memory:").expect("the in-memory source URI is valid"),
        }
    }

    /// Returns the original Markdown text.
    pub fn text(&self) -> &'a str {
        self.text
    }

    /// Returns the validated source URI.
    pub fn uri(&self) -> &Url {
        &self.uri
    }
}
