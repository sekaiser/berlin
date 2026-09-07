//! Serializable review artifacts and their diagnostics.

use berlin_content::{Collection, DocumentRef};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LinkedInPostDraft {
    pub source: DocumentRef,
    pub artifact_name: String,
    pub text: String,
    pub character_count: usize,
    pub diagnostics: Vec<LinkedInDraftDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LinkedInDraftDiagnostic {
    CharacterLimitExceeded { limit: usize, actual: usize },
}

/// Default feed-post character limit used by Berlin's draft diagnostics.
/// <https://www.linkedin.com/help/linkedin/answer/a528176/>
pub const LINKEDIN_POST_CHARACTER_LIMIT: usize = 3_000;

pub type LinkedInDrafts = Collection<LinkedInPostDraft>;
