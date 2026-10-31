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

/// Shared, non-truncating character-limit policy for local drafts.
pub fn draft_diagnostics(text: &str, limit: usize) -> Vec<LinkedInDraftDiagnostic> {
    let actual = text.chars().count();
    (actual > limit)
        .then_some(LinkedInDraftDiagnostic::CharacterLimitExceeded { limit, actual })
        .into_iter()
        .collect()
}
