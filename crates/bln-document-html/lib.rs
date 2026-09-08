//! HTML projection for Berlin's channel-neutral semantic document model.

#![deny(clippy::print_stderr)]
#![deny(clippy::print_stdout)]

mod code;
mod highlight;
mod html;
mod renderer;

pub use html::Html;
pub use renderer::Renderer;
