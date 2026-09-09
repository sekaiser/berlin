//! Minimal announcement input, separate from a full article's document model.

use berlin_content::DocumentRef;
use serde::Serialize;
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnouncementKind {
    New,
    Updated,
}

/// A draft starting point. The caller supplies a public URL and determines the
/// change kind; the renderer makes no assertion that publication has occurred.
pub struct Announcement<'a> {
    pub source: &'a DocumentRef,
    pub title: &'a str,
    pub url: &'a Url,
    pub kind: AnnouncementKind,
}
