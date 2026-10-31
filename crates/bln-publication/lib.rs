//! Channel-neutral publication composition. No Tera, Rhai, filesystem or clock.
//!
//! Source documents retain their authored identity. Each placement has a separate
//! deterministic occurrence identity. Renderers consume validated assemblies.

mod canonical;
mod diagnostic;
mod limits;
mod model;

pub use canonical::{canonical_bytes, fingerprint};
pub use diagnostic::{Code, Diagnostic, Result, Stage};
pub use limits::{BoundedWriter, PublicationLimits};
pub use model::*;
