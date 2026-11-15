//! Channel exports, exact-content approvals, and explicit delivery boundaries.
pub(crate) mod delivery;
pub(crate) mod linkedin;
mod markdown;
pub(crate) mod packages;
mod review;
pub(crate) mod store;

use super::release::{self, documents::DocumentRevision};
use crate::{args::announcement::Source, project::Project};
use anyhow::{Context, Error, bail};
use berlin_core::DeploymentTarget;
use berlin_document_linkedin::{AnnouncementKind, LINKEDIN_POST_CHARACTER_LIMIT};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;

// A full Markdown article is nested inside a JSON request, then inside an approval.
// Keep the larger bound specific to approvals; other lifecycle records stay small.
pub(crate) const MAX_APPROVAL_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Channel {
    LinkedIn,
    X,
    DevTo,
}
impl Channel {
    pub(crate) fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "linkedin" => Ok(Self::LinkedIn),
            "x" => Ok(Self::X),
            "devto" => Ok(Self::DevTo),
            _ => bail!("unknown distribution channel"),
        }
    }
}

/// Versioned approval payload. Paths to editable files and approval time are not
/// identity; text bytes, account and the precise release/document context are.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Snapshot {
    schema_version: u8,
    channel: Channel,
    pub account: String,
    pub before_release: String,
    pub after_release: String,
    website_url: Url,
    website_destination: DeploymentTarget,
    fingerprint_version: u8,
    pub document: DocumentRevision,
    kind: AnnouncementKind,
    pub article_url: Url,
    pub text_sha256: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<packages::Image>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linkedin: Option<linkedin::Target>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl Snapshot {
    pub(crate) fn channel(&self) -> &'static str {
        match self.channel {
            Channel::LinkedIn => "linkedin",
            Channel::X => "x",
            Channel::DevTo => "devto",
        }
    }
    fn id(&self) -> Result<String, Error> {
        Ok(release::digest(&serde_json::to_vec(self)?))
    }

    fn validate(&self) -> Result<(), Error> {
        if let Some(target) = &self.linkedin {
            if self.schema_version != 2 || self.channel != Channel::LinkedIn {
                bail!("LinkedIn identity binding requires a channel-package snapshot");
            }
            target.validate()?;
        }
        if !matches!(self.schema_version, 1 | 2)
            || (self.schema_version == 1
                && (self.channel != Channel::LinkedIn || self.image.is_some()))
        {
            bail!("unsupported announcement snapshot version");
        }
        if self.fingerprint_version != 1 {
            bail!("unsupported announcement document fingerprint version");
        }
        validate_account(&self.account)?;
        for id in [
            &self.before_release,
            &self.after_release,
            &self.document.content_sha256,
            &self.document.metadata_sha256,
            &self.text_sha256,
        ] {
            release::validate_id(id)?;
        }
        if self.document.id.0.trim().is_empty() || self.document.id.0.chars().any(char::is_control)
        {
            bail!("invalid announcement document ID");
        }
        release::validate_url(&self.website_url)?;
        self.website_destination.validate().map_err(Error::msg)?;
        if self.text.len()
            > if self.channel == Channel::DevTo {
                2 * 1024 * 1024
            } else {
                64 * 1024
            }
            || self
                .text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
        {
            bail!("announcement text contains unsupported control characters or is too large");
        }
        if release::digest(self.text.as_bytes()) != self.text_sha256 {
            bail!("announcement text hash does not match its bytes");
        }
        if let Some(image) = &self.image {
            release::validate_url(&image.url)?;
            release::validate_id(&image.sha256)?;
            if image.alt.trim().is_empty() {
                bail!("teaser image requires alternative text");
            }
        }
        if self.channel == Channel::DevTo {
            let value: serde_json::Value = serde_json::from_str(&self.text)?;
            let article = value["article"]
                .as_object()
                .context("DEV payload requires an article object")?;
            if value.as_object().is_none_or(|v| v.len() != 1)
                || article.keys().any(|k| {
                    !matches!(
                        k.as_str(),
                        "title"
                            | "body_markdown"
                            | "published"
                            | "canonical_url"
                            | "description"
                            | "main_image"
                            | "tags"
                    )
                })
            {
                bail!("unsupported DEV payload fields");
            }
            if article
                .get("body_markdown")
                .and_then(|v| v.as_str())
                .is_some_and(|s| {
                    s.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
                        .starts_with("---")
                })
            {
                bail!(
                    "DEV body frontmatter can override draft/account settings; remove it before approval"
                );
            }
            if value["article"]["published"] != false
                || value["article"]["canonical_url"] != self.article_url.as_str()
                || !value["article"]["body_markdown"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty())
                || !value["article"]["title"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty())
            {
                bail!("DEV edition must be a nonempty draft with the sealed canonical URL");
            }
            if value["article"]["main_image"]
                != self
                    .image
                    .as_ref()
                    .map(|i| serde_json::Value::String(i.url.to_string()))
                    .unwrap_or(serde_json::Value::Null)
            {
                bail!("DEV teaser image differs from the captured image");
            }
            if article.get("description").is_some_and(|v| !v.is_string()) {
                bail!("DEV description must be text");
            }
            if let Some(tags) = article.get("tags") {
                let tags = tags
                    .as_str()
                    .context("DEV tags must be a comma-separated string")?;
                let tags = tags
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                if tags.len() > 4
                    || tags.iter().any(|t| {
                        t.len() > 30
                            || !t
                                .bytes()
                                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                    })
                {
                    bail!("DEV supports at most four tags of up to 30 lowercase letters/digits");
                }
            }
        }
        Ok(())
    }

    fn approval_allowed(&self) -> bool {
        !self.text.trim().is_empty() && self.character_count() <= self.character_limit()
    }
    fn character_limit(&self) -> usize {
        match self.channel {
            Channel::LinkedIn => LINKEDIN_POST_CHARACTER_LIMIT,
            Channel::X => 280,
            Channel::DevTo => 2 * 1024 * 1024,
        }
    }
    fn character_count(&self) -> usize {
        if self.channel != Channel::X {
            return self.text.chars().count();
        }
        // Conservative X weighting: every non-ASCII scalar counts twice and each
        // whitespace-delimited URL reserves t.co's 23 characters. Never truncate.
        self.text
            .split_inclusive(char::is_whitespace)
            .map(|part| {
                let word = part.trim_end();
                let whitespace = part.len() - word.len();
                if word.starts_with("https://") || word.starts_with("http://") {
                    let url = word
                        .trim_end_matches(['.', ',', '!', '?', ';', ':', ')', ']', '}', '"', '\'']);
                    23 + whitespace + word.len() - url.len()
                } else {
                    part.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum()
                }
            })
            .sum()
    }

    pub(crate) fn same_revision(&self, other: &Self) -> bool {
        self.channel == other.channel
            && self.account == other.account
            && self.website_url == other.website_url
            && self.website_destination == other.website_destination
            && self.document.id == other.document.id
            && self.fingerprint_version == other.fingerprint_version
            && self.document.content_sha256 == other.document.content_sha256
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Approval {
    pub review_id: String,
    pub snapshot: Snapshot,
    pub approved_at_unix_seconds: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Evidence {
    UserReported,
    ApiAcknowledged,
    ApiVerified,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Publication {
    schema_version: u8,
    pub review_id: String,
    pub post_url: Url,
    evidence: Evidence,
    pub recorded_at_unix_seconds: u64,
}
impl Publication {
    pub(crate) fn api_receipt_matches(&self, event: &delivery::Event) -> bool {
        self.review_id == event.review
            && event.url.as_ref() == Some(&self.post_url)
            && event.linkedin.is_some()
            && match self.evidence {
                Evidence::ApiAcknowledged => event.state == "submitted",
                Evidence::ApiVerified => event.state == "reconciled",
                Evidence::UserReported => true,
            }
    }
    pub(crate) fn requires_api_receipt(&self) -> bool {
        !matches!(self.evidence, Evidence::UserReported)
    }

    pub(crate) fn evidence_label(&self) -> &'static str {
        match self.evidence {
            Evidence::UserReported => "Reported published by user; not verified",
            Evidence::ApiAcknowledged => {
                "LinkedIn accepted submission; live visibility not checked"
            }
            Evidence::ApiVerified => "LinkedIn content verified by API",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReviewState {
    ReviewRequired,
    Approved,
    ReportedPublished,
    SubmissionAccepted,
    ApiVerified,
}

#[derive(Debug, Serialize)]
pub(crate) struct Review {
    pub review_id: String,
    pub snapshot: Snapshot,
    pub character_count: usize,
    pub character_limit: usize,
    pub approval_allowed: bool,
    pub state: ReviewState,
    /// Reports of the same article content on this site/account, even when draft
    /// wording or presentation changed. References point into the saved history.
    pub same_revision_reports: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct HistoryEntry {
    pub approval: Approval,
    pub publication: Option<Publication>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Status {
    schema_version: u8,
    remote_verified: bool,
    website_live_checked: bool,
    pub reviews: Vec<Review>,
    pub history: Vec<HistoryEntry>,
}

fn validate_account(account: &str) -> Result<(), Error> {
    if account.is_empty()
        || account.len() > 64
        || !account.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        bail!(
            "account must be a stable lowercase local name (letters, digits, '-' or '_'; up to 64 characters)"
        );
    }
    Ok(())
}

fn load_source(project: &Project, source: &Source) -> Result<Snapshot, Error> {
    let manifest = review::read_text(
        &project.root().join(&source.directory).join("manifest.json"),
        8 * 1024 * 1024,
    )?;
    if serde_json::from_str::<serde_json::Value>(&manifest)?["schema_version"] == 2 {
        return packages::snapshot(
            project,
            &source.directory,
            &source.document,
            &source.account,
            Channel::parse(&source.channel)?,
        );
    }
    if source.channel != "linkedin" {
        bail!("legacy release drafts only support LinkedIn");
    }
    review::Export::open(project, &source.directory)?
        .snapshots(&source.account, Some(&source.document))?
        .pop()
        .context("no selected announcement")
}

pub(crate) fn review(
    project: &Project,
    source: &Source,
    approve: Option<&str>,
) -> Result<Review, Error> {
    let snapshot = load_source(project, source)?;
    if let Some(confirm) = approve {
        if confirm != snapshot.id()? {
            bail!(
                "approval must match the current review ID; preview and review the exact text first"
            );
        }
        if !snapshot.approval_allowed() {
            bail!("cannot approve empty text or text beyond Berlin's default character limit");
        }
        let _lock = store::lock(project)?;
        let history = store::load(project)?;
        if !history
            .iter()
            .any(|entry| entry.approval.review_id == confirm)
        {
            store::approve(
                project,
                &Approval {
                    review_id: confirm.into(),
                    snapshot: snapshot.clone(),
                    approved_at_unix_seconds: now()?,
                },
            )?;
        }
    }
    review_with_history(snapshot, &store::load(project)?)
}

fn review_with_history(snapshot: Snapshot, history: &[HistoryEntry]) -> Result<Review, Error> {
    let id = snapshot.id()?;
    let state = match history.iter().find(|entry| entry.approval.review_id == id) {
        Some(entry) if entry.publication.is_some() => {
            match entry.publication.as_ref().unwrap().evidence {
                Evidence::UserReported => ReviewState::ReportedPublished,
                Evidence::ApiAcknowledged => ReviewState::SubmissionAccepted,
                Evidence::ApiVerified => ReviewState::ApiVerified,
            }
        }
        Some(_) => ReviewState::Approved,
        None => ReviewState::ReviewRequired,
    };
    Ok(Review {
        review_id: id,
        character_count: snapshot.character_count(),
        character_limit: snapshot.character_limit(),
        approval_allowed: snapshot.approval_allowed(),
        state,
        same_revision_reports: history
            .iter()
            .filter(|entry| {
                entry.publication.is_some() && entry.approval.snapshot.same_revision(&snapshot)
            })
            .map(|entry| entry.approval.review_id.clone())
            .collect(),
        snapshot,
    })
}

pub(crate) fn record(
    project: &Project,
    source: &Source,
    url: &str,
    confirm: &str,
) -> Result<Publication, Error> {
    let snapshot = load_source(project, source)?;
    if snapshot.channel == Channel::DevTo {
        bail!(
            "DEV draft delivery has its own verified delivery record; do not report a draft as a public announcement"
        );
    }
    if snapshot.id()? != confirm {
        bail!(
            "confirmation does not match the current text/account/revision; review and approve it again"
        );
    }
    let post_url = channel_post_url(&snapshot.channel, url)?;
    // Inspect before creating a lock/state directory so unapproved requests are read-only.
    require_approval(&store::load(project)?, confirm)?;
    let _lock = store::lock(project)?;
    let history = store::load(project)?;
    let approved = require_approval(&history, confirm)?;
    if let Some(existing) = &approved.publication {
        if existing.post_url == post_url {
            return Ok(existing.clone());
        }
        bail!("this review already has a reported publication; its URL will not be overwritten");
    }
    for entry in &history {
        if let Some(publication) = &entry.publication {
            if entry.approval.snapshot.same_revision(&snapshot) {
                bail!(
                    "this document revision/account already has a reported announcement: {}",
                    publication.post_url
                );
            }
            if publication.post_url == post_url {
                bail!("this post URL is already recorded against another review");
            }
        }
    }
    let publication = Publication {
        schema_version: 1,
        review_id: confirm.into(),
        post_url,
        evidence: Evidence::UserReported,
        recorded_at_unix_seconds: now()?,
    };
    store::record(project, &publication)?;
    Ok(publication)
}

fn require_approval<'a>(history: &'a [HistoryEntry], id: &str) -> Result<&'a HistoryEntry, Error> {
    history.iter().find(|entry| entry.approval.review_id == id).context("announcement has not been approved; preview it and explicitly approve the current review ID")
}

pub(crate) fn status(
    project: &Project,
    directory: Option<&Path>,
    account: Option<&str>,
    document: Option<&str>,
) -> Result<Status, Error> {
    if let Some(account) = account {
        validate_account(account)?;
    }
    let history = store::load(project)?
        .into_iter()
        .filter(|entry| {
            account.is_none_or(|account| account == entry.approval.snapshot.account)
                && document.is_none_or(|id| id == entry.approval.snapshot.document.id.0)
        })
        .collect::<Vec<_>>();
    let reviews = match directory {
        Some(directory)
            if serde_json::from_str::<serde_json::Value>(&review::read_text(
                &project.root().join(directory).join("manifest.json"),
                8 * 1024 * 1024,
            )?)?["schema_version"]
                == 2 =>
        {
            let value: serde_json::Value = serde_json::from_str(&review::read_text(
                &project.root().join(directory).join("manifest.json"),
                64 * 1024,
            )?)?;
            let id = value["document"]
                .as_str()
                .context("missing package document")?;
            if document.is_some_and(|d| d != id) {
                bail!("document is not in this package");
            }
            let account = account.context("package status requires an account")?;
            let mut result = vec![];
            for channel in [Channel::LinkedIn, Channel::X, Channel::DevTo] {
                if channel == Channel::DevTo
                    && !project
                        .root()
                        .join(directory)
                        .join("devto.json")
                        .try_exists()?
                {
                    continue;
                }
                result.push(review_with_history(
                    packages::snapshot(project, directory, id, account, channel)?,
                    &history,
                )?);
            }
            result
        }
        Some(directory) => review::Export::open(project, directory)?
            .snapshots(
                account.context("review-directory status requires an account")?,
                document,
            )?
            .into_iter()
            .map(|snapshot| review_with_history(snapshot, &history))
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
    Ok(Status {
        schema_version: 1,
        remote_verified: false,
        website_live_checked: false,
        reviews,
        history,
    })
}

/// Syntax and host check only. No claim that this URL is a post, belongs to the
/// named account, exists, or contains the approved text. Strip share tracking.
fn channel_post_url(channel: &Channel, value: &str) -> Result<Url, Error> {
    let mut url = Url::parse(value)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !match channel {
            Channel::LinkedIn => {
                matches!(url.host_str(), Some("linkedin.com" | "www.linkedin.com"))
            }
            Channel::X => matches!(
                url.host_str(),
                Some("x.com" | "www.x.com" | "twitter.com" | "www.twitter.com")
            ),
            Channel::DevTo => false,
        }
        || url.path().trim_matches('/').is_empty()
    {
        bail!(
            "use an HTTPS {} publication URL without credentials or a port",
            if *channel == Channel::X {
                "x.com"
            } else {
                "linkedin.com"
            }
        );
    }
    url.set_host(Some(if *channel == Channel::X {
        "x.com"
    } else {
        "www.linkedin.com"
    }))?;
    url.set_query(None);
    url.set_fragment(None);
    if url.path().ends_with('/') {
        let path = url.path().trim_end_matches('/').to_owned();
        url.set_path(&path);
    }
    Ok(url)
}

fn now() -> Result<u64, Error> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
