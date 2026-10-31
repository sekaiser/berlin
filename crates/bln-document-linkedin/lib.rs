//! Local, reviewable LinkedIn draft projection for semantic documents.

#![deny(clippy::print_stderr)]
#![deny(clippy::print_stdout)]

mod announcement;
mod draft;
pub mod fragment;
mod renderer;
mod text;

pub use announcement::{Announcement, AnnouncementKind};
pub use draft::{
    LINKEDIN_POST_CHARACTER_LIMIT, LinkedInDraftDiagnostic, LinkedInDrafts, LinkedInPostDraft,
};
pub use renderer::Renderer;
