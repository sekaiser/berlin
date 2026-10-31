//! Channel-neutral publication composition. No Tera, Rhai, filesystem or clock.
//!
//! Source documents retain their authored identity. Each placement has a separate
//! deterministic occurrence identity. Renderers consume validated assemblies.

mod assembly;
mod canonical;
mod catalog;
mod components;
mod diagnostic;
mod excerpt;
mod limits;
mod model;
mod query;
mod registry;
mod resources;
mod site;

pub use assembly::*;
pub use canonical::{canonical_bytes, fingerprint, input_digest};
pub use catalog::*;
pub use components::*;
pub use diagnostic::{Code, Diagnostic, Result, Stage};
pub use excerpt::*;
pub use limits::{BoundedWriter, PublicationLimits};
pub use model::*;
pub use query::*;
pub use registry::*;
pub use resources::*;
pub use site::*;

pub use berlin_publication_macros::{__view, Component};

/// Compose typed components. The facade forwards its hygienic runtime path,
/// including when this crate is renamed by a downstream Cargo dependency.
#[macro_export]
macro_rules! view {
    ($($view:tt)*) => { $crate::__view!($crate; $($view)*) };
}

#[doc(hidden)]
pub mod __private {
    pub struct Missing;
    pub struct Set;
}
