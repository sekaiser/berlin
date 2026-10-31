//! Opt-in presentation context for already resolved semantic excerpts.
use std::collections::HashMap;
use std::fmt;

use crate::Html;
use berlin_document::{ComponentId, PropertyValue};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct FragmentContext {
    pub namespace: String,
    /// Explicit checked destinations, including outside-excerpt original links.
    pub destinations: HashMap<String, String>,
    pub heading_offset: i8,
}
impl FragmentContext {
    pub(crate) fn qualify(&self, id: &str) -> String {
        if self.namespace.is_empty() {
            id.into()
        } else {
            format!("{}--{id}", self.namespace)
        }
    }
    pub(crate) fn destination(&self, source: &str) -> String {
        self.destinations
            .get(source)
            .cloned()
            .unwrap_or_else(|| source.into())
    }
}

#[derive(Debug)]
pub enum FragmentError {
    Math(crate::MathError),
    OutputLimit,
    Presentation(String),
    HeadingDepth,
}
impl fmt::Display for FragmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Math(error) => write!(f, "{error}"),
            Self::OutputLimit => f.write_str("ResourceLimit: semantic HTML byte budget exceeded"),
            Self::Presentation(detail) => f.write_str(detail),
            Self::HeadingDepth => {
                f.write_str("HeadingDepthExceeded: semantic heading must remain between 1 and 6")
            }
        }
    }
}
impl std::error::Error for FragmentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Math(e) => Some(e),
            _ => None,
        }
    }
}

/// Trusted, opt-in presenter; children have already passed through this semantic
/// renderer. The callback performs no content discovery or source mutation.
pub trait SemanticComponentPresenter {
    fn present(
        &self,
        id: &ComponentId,
        name: &str,
        properties: &BTreeMap<String, PropertyValue>,
        children: Html,
    ) -> Result<String, String>;
}
