//! Errors produced while validating, parsing, or rendering Markdown sources.

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid Markdown source URI '{source_uri}': {source}")]
    InvalidSourceUri {
        source_uri: String,
        #[source]
        source: url::ParseError,
    },
    #[error("invalid front matter in {source_uri}: {message}")]
    InvalidFrontMatter { source_uri: String, message: String },
    #[error("invalid shortcode syntax in {source_uri}: {message}")]
    InvalidShortcode { source_uri: String, message: String },
    #[error("invalid semantic document from {source_uri}: {message}")]
    InvalidDocument { source_uri: String, message: String },
}
