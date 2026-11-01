//! Typed, effect-free pipeline plans and their validation rules.

mod deployment;
mod pipeline;
mod tex;
pub use deployment::DeploymentTarget;
pub use tex::TexEngine;
mod website;

pub use pipeline::ArtifactKind;
pub use pipeline::Composer;
pub use pipeline::FunctionRef;
pub use pipeline::NodeId;
pub use pipeline::Operation;
pub use pipeline::OperationSignature;
pub use pipeline::PipelineLoader;
pub use pipeline::PipelineNode;
pub use pipeline::PipelinePlan;
pub use pipeline::PipelineValidationError;
pub use website::{GiscusConfig, WebsiteConfig, WebsiteProfiles};
