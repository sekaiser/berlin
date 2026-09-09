//! Reusable LinkedIn renderer and draft diagnostic policy.

use crate::text;
use crate::{
    Announcement, AnnouncementKind, LINKEDIN_POST_CHARACTER_LIMIT, LinkedInDraftDiagnostic,
    LinkedInDrafts, LinkedInPostDraft,
};
use berlin_content::{Collection, DocumentCollection, DocumentRef};
use berlin_document::Document;

/// Renders semantic documents into reviewable LinkedIn drafts.
pub struct Renderer {
    character_limit: usize,
}

impl Renderer {
    /// Renders non-draft documents in collection order.
    pub fn render_collection(&self, documents: &DocumentCollection) -> LinkedInDrafts {
        Collection::new(
            documents
                .non_drafts()
                .map(|document| self.render(document))
                .collect::<Vec<_>>(),
        )
    }

    /// Renders one document, retaining over-limit text and reporting diagnostics.
    ///
    /// Explicit single-document rendering also accepts documents marked as drafts.
    pub fn render(&self, document: &Document) -> LinkedInPostDraft {
        let text = text::render_document(document);
        self.draft(
            DocumentRef::from(document),
            artifact_name(&document.provenance.source),
            text,
        )
    }

    /// Renders a title-and-link starting point, not a summary of unseen changes.
    pub fn render_announcement(&self, announcement: &Announcement<'_>) -> LinkedInPostDraft {
        let prefix = match announcement.kind {
            AnnouncementKind::New => "New",
            AnnouncementKind::Updated => "Updated",
        };
        let title = announcement
            .title
            .chars()
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .collect::<String>();
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        let text = format!("{prefix}: {title}\n\n{}", announcement.url);
        self.draft(
            announcement.source.clone(),
            artifact_name(announcement.url.as_str()),
            text,
        )
    }

    fn draft(&self, source: DocumentRef, artifact_name: String, text: String) -> LinkedInPostDraft {
        let character_count = text.chars().count();
        let diagnostics = (character_count > self.character_limit)
            .then_some(LinkedInDraftDiagnostic::CharacterLimitExceeded {
                limit: self.character_limit,
                actual: character_count,
            })
            .into_iter()
            .collect();

        LinkedInPostDraft {
            source,
            artifact_name,
            character_count,
            text,
            diagnostics,
        }
    }
}

impl Default for Renderer {
    /// Creates a renderer with Berlin's default feed-post character limit.
    fn default() -> Self {
        Self {
            character_limit: LINKEDIN_POST_CHARACTER_LIMIT,
        }
    }
}

fn artifact_name(source: &str) -> String {
    let source = source
        .split(['?', '#'])
        .next()
        .unwrap_or(source)
        .trim_end_matches('/');
    let segment = source.rsplit('/').next().unwrap_or(source);
    if segment.ends_with(':') {
        return "document".into();
    }
    let stem = segment.rsplit_once('.').map_or(segment, |(stem, _)| stem);
    let stem = stem
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let stem = stem.trim_matches('-');
    if stem.is_empty() {
        "document".into()
    } else {
        stem.into()
    }
}
