//! Typed HTML produced from a semantic document.

use std::fmt;

/// Rendered HTML projected from a Berlin semantic document.
///
/// Raw HTML blocks and figure captions are preserved; this type does not imply
/// sanitization.
#[derive(Clone, Debug, Eq, PartialEq)]
#[must_use]
pub struct Html(String);

impl Html {
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    /// Borrows the rendered HTML.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the value and returns its rendered HTML.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for Html {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for Html {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
