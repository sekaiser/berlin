//! HTML projection for Berlin's channel-neutral semantic document model.

#![deny(clippy::print_stderr)]
#![deny(clippy::print_stdout)]

mod code;
mod fragment;
mod highlight;
mod html;
mod math;
mod output;
mod renderer;

pub use berlin_math::Error as MathError;
pub use fragment::{FragmentContext, FragmentError, SemanticComponentPresenter};
pub use html::Html;
pub use renderer::Renderer;
