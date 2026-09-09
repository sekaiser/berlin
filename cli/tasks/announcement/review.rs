//! Reconnect exported, editable text to verified immutable release identities.
use super::{Channel, Snapshot};
use crate::{
    project::Project,
    tasks::release::{
        self,
        announcements::{self, AnnouncementPlan},
    },
};
use anyhow::{Context, Error, bail};
use serde_json::Value;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub(super) struct Export {
    directory: PathBuf,
    plan: AnnouncementPlan,
}

impl Export {
    pub fn open(project: &Project, directory: &Path) -> Result<Self, Error> {
        let directory = project.root().join(directory);
        release::ensure_directory(&directory)?;
        let manifest: Value = serde_json::from_str(&read_text(
            &directory.join("manifest.json"),
            8 * 1024 * 1024,
        )?)?;
        let before = manifest["before"]["id"]
            .as_str()
            .context("review manifest has no BEFORE release ID")?;
        let after = manifest["after"]["id"]
            .as_str()
            .context("review manifest has no AFTER release ID")?;
        let plan = announcements::prepare(project, before, after)?;
        // The manifest is the unchanged generation snapshot, not an editable source
        // of filenames, URLs, or revision metadata. Only the text files are editable.
        if manifest != serde_json::to_value(&plan)? {
            bail!(
                "review manifest does not match its verified release pair; export a new review directory"
            );
        }
        Ok(Self { directory, plan })
    }

    pub fn snapshots(&self, account: &str, document: Option<&str>) -> Result<Vec<Snapshot>, Error> {
        super::validate_account(account)?;
        let items = self
            .plan
            .drafts
            .iter()
            .filter(|item| document.is_none_or(|id| id == item.document.id.0));
        let snapshots = items
            .map(|item| {
                let name = &item.draft.artifact_name;
                if name.is_empty()
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                {
                    bail!("unsafe generated draft filename");
                }
                let text = read_text(&self.directory.join(format!("{name}.txt")), 64 * 1024)?;
                let snapshot = Snapshot {
                    schema_version: 1,
                    channel: Channel::LinkedIn,
                    account: account.into(),
                    before_release: self.plan.before.id.clone(),
                    after_release: self.plan.after.id.clone(),
                    website_url: self.plan.after.metadata.url.clone(),
                    website_destination: self.plan.after.metadata.destination.clone(),
                    // Release verification currently accepts only fingerprint version 1.
                    fingerprint_version: 1,
                    document: item.document.clone(),
                    kind: item.kind,
                    article_url: item.url.clone(),
                    text_sha256: release::digest(text.as_bytes()),
                    text,
                };
                snapshot.validate()?;
                Ok(snapshot)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        if document.is_some() && snapshots.is_empty() {
            bail!("document is not an announcement candidate in this review");
        }
        Ok(snapshots)
    }
}

pub(super) fn read_text(path: &Path, limit: u64) -> Result<String, Error> {
    release::ensure_file(path)?;
    let mut text = String::new();
    File::open(path)?
        .take(limit + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > limit {
        bail!(
            "review or state file exceeds its size limit: {}",
            path.display()
        );
    }
    Ok(text)
}
