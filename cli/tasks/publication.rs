//! One authoritative publisher per project. Intent is durable before remote writes.
//! Unknown outcomes are reconciled before another push; no distributed rollback.
mod github_pages;

use super::{
    output::ProjectLock,
    release::{self, WebsiteRelease},
};
use crate::project::Project;
use anyhow::{Context, Error, bail};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Commit {
    pub sha: String,
    pub parent: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Prepared,
    Unknown,
    Uploaded,
    Live,
    Failed,
    Conflict,
}

impl Status {
    fn after_observation(self, observed: Observation) -> Self {
        match observed {
            Observation::AtParent if matches!(self, Self::Prepared | Self::Unknown) => {
                Self::Prepared
            }
            // Seeing the old head is not proof that an acknowledged push never happened.
            Observation::AtParent | Observation::Conflict => Self::Conflict,
            Observation::Uploaded => Self::Uploaded,
            Observation::Live => Self::Live,
            Observation::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Publication {
    schema_version: u8,
    release: String,
    commit: Commit,
    pub status: Status,
    updated_at_unix_seconds: u64,
    live_observed_at_unix_seconds: Option<u64>,
}

impl Publication {
    fn prepared(release: &WebsiteRelease, commit: Commit) -> Self {
        Self {
            schema_version: 1,
            release: release.id.clone(),
            commit,
            status: Status::Prepared,
            updated_at_unix_seconds: 0,
            live_observed_at_unix_seconds: None,
        }
    }

    fn refresh(&mut self, remote: &mut impl PagesRemote) -> Result<(), Error> {
        self.status = self.status.after_observation(remote.observe(&self.commit)?);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Observation {
    /// The branch is at the recorded parent, or still absent for an initial push.
    AtParent,
    Uploaded,
    Live,
    Failed,
    Conflict,
}

// A narrow test boundary for this adapter, not a public plugin framework.
pub(super) trait PagesRemote {
    fn preflight(&mut self, release: &WebsiteRelease) -> Result<(), Error>;
    fn prepare(&mut self, release: &WebsiteRelease) -> Result<Commit, Error>;
    fn push(&mut self, commit: &Commit) -> Result<(), Error>;
    fn observe(&mut self, commit: &Commit) -> Result<Observation, Error>;
}

pub(crate) fn publish(project: &Project, id: &str, confirm: &str) -> Result<Publication, Error> {
    if id != confirm {
        bail!("confirmation must match the reviewed release ID");
    }
    release::validate_id(id)?;
    let _lock = ProjectLock::acquire(project.root())?;
    let release = release::open(project, id)?;
    let directory = release::state_directory(project.root(), "publications")?;
    let path = directory.join(format!("{id}.json"));
    let mut remote = github_pages::GitHubPages::new(&release)?;
    publish_with(&release, &path, &mut remote)
}

fn publish_with(
    release: &WebsiteRelease,
    path: &Path,
    remote: &mut impl PagesRemote,
) -> Result<Publication, Error> {
    let previous = read(path, &release.id)?;
    remote.preflight(release)?;
    if let Some(mut record) = previous {
        record.refresh(remote)?;
        save(path, &mut record)?;
        if record.status != Status::Prepared {
            return Ok(record);
        }
        let commit = remote.prepare(release)?;
        if commit != record.commit {
            record.status = Status::Conflict;
            save(path, &mut record)?;
            bail!("remote changed during recovery; inspect the publication before retrying");
        }
        push_and_record(path, record, remote)
    } else {
        let commit = remote.prepare(release)?;
        let record = Publication::prepared(release, commit);
        push_and_record(path, record, remote)
    }
}

fn push_and_record(
    path: &Path,
    mut record: Publication,
    remote: &mut impl PagesRemote,
) -> Result<Publication, Error> {
    record.status = Status::Prepared;
    save(path, &mut record)?;
    if let Err(error) = remote.push(&record.commit) {
        record.status = Status::Unknown;
        save(path, &mut record)?;
        return Err(error.context(format!(
            "push outcome is unknown; run 'bln publication {} --refresh' before retrying",
            record.release
        )));
    }
    record.status = Status::Uploaded;
    save(path, &mut record)?;
    // Upload acknowledgment is useful even when Pages cannot yet be queried.
    match record.refresh(remote) {
        Ok(()) => save(path, &mut record)?,
        Err(error) => log::warn!("Upload acknowledged; unable to refresh publication: {error:#}"),
    }
    Ok(record)
}

pub(crate) fn inspect(project: &Project, id: &str, refresh: bool) -> Result<Publication, Error> {
    release::validate_id(id)?;
    let _lock = ProjectLock::acquire(project.root())?;
    let release = release::open(project, id)?;
    let directory = project.root().join(".berlin/publications");
    if !super::state::active(project.root())? {
        release::ensure_directory(&directory)?;
    }
    let path = directory.join(format!("{id}.json"));
    let mut record = read(&path, id)?.context("release has no publication record")?;
    if refresh {
        let mut remote = github_pages::GitHubPages::new(&release)?;
        remote.preflight(&release)?;
        record.refresh(&mut remote)?;
        save(&path, &mut record)?;
    }
    Ok(record)
}

fn save(path: &Path, record: &mut Publication) -> Result<(), Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    record.updated_at_unix_seconds = now;
    if record.status == Status::Live && record.live_observed_at_unix_seconds.is_none() {
        record.live_observed_at_unix_seconds = Some(now);
    }
    if let Some(root) = state_root(path)
        && super::state::active(&root)?
    {
        return super::state::Store::open(&root)?.save_publication(&record.release, record);
    }
    release::atomic_json(path, record)
}

fn state_root(path: &Path) -> Option<std::path::PathBuf> {
    let parent = path.parent()?;
    let state = parent.parent()?;
    (parent.file_name()? == "publications" && state.file_name()? == ".berlin")
        .then(|| state.parent().unwrap().to_path_buf())
}
fn read(path: &Path, id: &str) -> Result<Option<Publication>, Error> {
    if let Some(root) = state_root(path)
        && super::state::active(&root)?
    {
        return super::state::Store::open(&root)?.publication(id);
    }
    read_file(path, id)
}
pub(crate) fn read_file(path: &Path, id: &str) -> Result<Option<Publication>, Error> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(metadata) if !metadata.is_file() => bail!("publication record is not a regular file"),
        Ok(_) => {}
    }
    decode(&fs::read(path)?, id).map(Some)
}
pub(crate) fn decode(bytes: &[u8], id: &str) -> Result<Publication, Error> {
    let record: Publication = serde_json::from_slice(bytes)?;
    if record.schema_version != 1 || record.release != id {
        bail!("publication record does not match this release");
    }
    for sha in std::iter::once(&record.commit.sha).chain(record.commit.parent.iter()) {
        if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("invalid publication commit ID");
        }
    }
    Ok(record)
}

#[cfg(test)]
mod tests;
