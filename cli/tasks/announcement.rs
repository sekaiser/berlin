//! Exact-text approvals and user-reported manual publication, without remote I/O.
mod review;
mod store;

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Channel {
    LinkedIn,
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
}

impl Snapshot {
    fn id(&self) -> Result<String, Error> {
        Ok(release::digest(&serde_json::to_vec(self)?))
    }

    fn validate(&self) -> Result<(), Error> {
        if self.schema_version != 1 {
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
        if self.text.len() > 64 * 1024
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
        Ok(())
    }

    fn approval_allowed(&self) -> bool {
        !self.text.trim().is_empty() && self.text.chars().count() <= LINKEDIN_POST_CHARACTER_LIMIT
    }

    fn same_revision(&self, other: &Self) -> bool {
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReviewState {
    ReviewRequired,
    Approved,
    ReportedPublished,
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
        Some(entry) if entry.publication.is_some() => ReviewState::ReportedPublished,
        Some(_) => ReviewState::Approved,
        None => ReviewState::ReviewRequired,
    };
    Ok(Review {
        review_id: id,
        character_count: snapshot.text.chars().count(),
        character_limit: LINKEDIN_POST_CHARACTER_LIMIT,
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
    if snapshot.id()? != confirm {
        bail!(
            "confirmation does not match the current text/account/revision; review and approve it again"
        );
    }
    let post_url = post_url(url)?;
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
fn post_url(value: &str) -> Result<Url, Error> {
    let mut url = Url::parse(value)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !matches!(url.host_str(), Some("linkedin.com" | "www.linkedin.com"))
        || url.path().trim_matches('/').is_empty()
    {
        bail!("use an HTTPS linkedin.com publication URL without credentials or a port");
    }
    url.set_host(Some("www.linkedin.com"))?;
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
