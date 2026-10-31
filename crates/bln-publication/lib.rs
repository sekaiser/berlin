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
mod registry;

pub use assembly::*;
pub use canonical::{canonical_bytes, fingerprint, input_digest};
pub use catalog::*;
pub use components::*;
pub use diagnostic::{Code, Diagnostic, Result, Stage};
pub use excerpt::*;
pub use limits::{BoundedWriter, PublicationLimits};
pub use model::*;
pub use registry::*;
