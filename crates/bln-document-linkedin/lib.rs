//! Local, reviewable LinkedIn draft projection for semantic documents.

#![deny(clippy::print_stderr)]
#![deny(clippy::print_stdout)]

mod draft;
mod renderer;
mod text;

pub use draft::{
    LINKEDIN_POST_CHARACTER_LIMIT, LinkedInDraftDiagnostic, LinkedInDrafts, LinkedInPostDraft,
};
pub use renderer::Renderer;
