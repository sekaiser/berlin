//! Berlin's typed Markdown input adapter.
//!
//! [`Parser`] converts a validated [`Source`] into Berlin's channel-neutral
//! semantic document model. [`Renderer`] provides the separate direct-HTML
//! projection needed for Markdown fragments and syntax-highlighted code.
//! Berlin's Ox-Hugo-compatible Markdown dialect is configured internally.

mod code;
mod document;
mod error;
mod front_matter;
mod options;
mod parser;
mod renderer;
mod shortcode;
mod source;

pub use error::Error;
pub use parser::Parser;
pub use renderer::Html;
pub use renderer::Renderer;
pub use source::Source;
