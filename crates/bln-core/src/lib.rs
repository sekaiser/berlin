//! Typed, effect-free pipeline plans and their validation rules.

mod pipeline;
mod website;

pub use pipeline::ArtifactKind;
pub use pipeline::FunctionRef;
pub use pipeline::NodeId;
pub use pipeline::Operation;
pub use pipeline::OperationSignature;
pub use pipeline::PipelineLoader;
pub use pipeline::PipelineNode;
pub use pipeline::PipelinePlan;
pub use pipeline::PipelineValidationError;
pub use website::{WebsiteConfig, WebsiteProfiles};
