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

pub(super) enum Observation {
    NotUploaded,
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
        let observed = remote.observe(&record.commit)?;
        if !matches!(observed, Observation::NotUploaded) {
            record.status = status(observed);
            save(path, &mut record)?;
            return Ok(record);
        }
        if !matches!(record.status, Status::Prepared | Status::Unknown) {
            record.status = Status::Conflict;
            save(path, &mut record)?;
            return Ok(record);
        }
        let commit = remote.prepare(release)?;
        if commit != record.commit {
            bail!("remote changed during recovery; inspect the publication before retrying");
        }
        push_and_record(path, record, remote)
    } else {
        let commit = remote.prepare(release)?;
        let record = Publication {
            schema_version: 1,
            release: release.id.clone(),
            commit,
            status: Status::Prepared,
            updated_at_unix_seconds: 0,
            live_observed_at_unix_seconds: None,
        };
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
        return Err(
            error.context("push outcome is unknown; use publication --refresh before retrying")
        );
    }
    record.status = Status::Uploaded;
    save(path, &mut record)?;
    // Upload acknowledgment is useful even when Pages cannot yet be queried.
    if let Ok(observed) = remote.observe(&record.commit) {
        record.status = match observed {
            Observation::NotUploaded => Status::Unknown,
            other => status(other),
        };
        save(path, &mut record)?;
    }
    Ok(record)
}

pub(crate) fn inspect(project: &Project, id: &str, refresh: bool) -> Result<Publication, Error> {
    release::validate_id(id)?;
    let _lock = ProjectLock::acquire(project.root())?;
    let release = release::open(project, id)?;
    let directory = project.root().join(".berlin/publications");
    release::ensure_directory(&directory)?;
    let path = directory.join(format!("{id}.json"));
    let mut record = read(&path, id)?.context("release has no publication record")?;
    if refresh {
        let mut remote = github_pages::GitHubPages::new(&release)?;
        remote.preflight(&release)?;
        let observed = remote.observe(&record.commit)?;
        record.status = match observed {
            Observation::NotUploaded
                if matches!(record.status, Status::Prepared | Status::Unknown) =>
            {
                Status::Prepared
            }
            Observation::NotUploaded => Status::Conflict,
            other => status(other),
        };
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
    release::atomic_json(path, record)
}

fn status(observed: Observation) -> Status {
    match observed {
        Observation::NotUploaded => Status::Prepared,
        Observation::Uploaded => Status::Uploaded,
        Observation::Live => Status::Live,
        Observation::Failed => Status::Failed,
        Observation::Conflict => Status::Conflict,
    }
}

fn read(path: &Path, id: &str) -> Result<Option<Publication>, Error> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(metadata) if !metadata.is_file() => bail!("publication record is not a regular file"),
        Ok(_) => {}
    }
    let record: Publication = serde_json::from_slice(&fs::read(path)?)?;
    if record.schema_version != 1 || record.release != id {
        bail!("publication record does not match this release");
    }
    for sha in std::iter::once(&record.commit.sha).chain(record.commit.parent.iter()) {
        if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("invalid publication commit ID");
        }
    }
    Ok(Some(record))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::release::Manifest;

    struct FakeRemote {
        path: std::path::PathBuf,
        pushes: usize,
        uploaded: bool,
        fail_after_push: bool,
        conflict: bool,
    }
    fn commit() -> Commit {
        Commit {
            sha: "a".repeat(40),
            parent: Some("b".repeat(40)),
        }
    }
    impl PagesRemote for FakeRemote {
        fn preflight(&mut self, _: &WebsiteRelease) -> Result<(), Error> {
            Ok(())
        }
        fn prepare(&mut self, _: &WebsiteRelease) -> Result<Commit, Error> {
            Ok(commit())
        }
        fn push(&mut self, _: &Commit) -> Result<(), Error> {
            let record: Publication = serde_json::from_slice(&fs::read(&self.path)?).unwrap();
            assert_eq!(record.status, Status::Prepared, "intent must precede push");
            self.pushes += 1;
            self.uploaded = true;
            if self.fail_after_push {
                bail!("response lost");
            }
            Ok(())
        }
        fn observe(&mut self, _: &Commit) -> Result<Observation, Error> {
            Ok(if self.conflict {
                Observation::Conflict
            } else if self.uploaded {
                Observation::Live
            } else {
                Observation::NotUploaded
            })
        }
    }
    fn fixture(root: &Path) -> (WebsiteRelease, FakeRemote) {
        (
            WebsiteRelease {
                id: "1".repeat(64),
                directory: root.join("release"),
                manifest: Manifest {
                    schema_version: 1,
                    berlin_version: "test".into(),
                    pipeline: "site".into(),
                    website_node: "website".into(),
                    url: "https://example.com".parse().unwrap(),
                    destination: berlin_core::DeploymentTarget::GitHubPages {
                        repository: "owner/site".into(),
                    },
                    files: vec![],
                },
            },
            FakeRemote {
                path: root.join("publication.json"),
                pushes: 0,
                uploaded: false,
                fail_after_push: false,
                conflict: false,
            },
        )
    }

    #[test]
    fn repeated_publish_reconciles_without_another_write() {
        let root = tempfile::tempdir().unwrap();
        let (release, mut remote) = fixture(root.path());
        let path = remote.path.clone();
        assert_eq!(
            publish_with(&release, &path, &mut remote).unwrap().status,
            Status::Live
        );
        assert_eq!(
            publish_with(&release, &path, &mut remote).unwrap().status,
            Status::Live
        );
        assert_eq!(remote.pushes, 1);
    }

    #[test]
    fn lost_response_does_not_duplicate_a_successful_push() {
        let root = tempfile::tempdir().unwrap();
        let (release, mut remote) = fixture(root.path());
        remote.fail_after_push = true;
        let path = remote.path.clone();
        assert!(publish_with(&release, &path, &mut remote).is_err());
        assert_eq!(
            read(&path, &release.id).unwrap().unwrap().status,
            Status::Unknown
        );
        assert_eq!(
            publish_with(&release, &path, &mut remote).unwrap().status,
            Status::Live
        );
        assert_eq!(remote.pushes, 1);
    }

    #[test]
    fn interrupted_prepared_operation_can_retry_only_the_same_commit() {
        let root = tempfile::tempdir().unwrap();
        let (release, mut remote) = fixture(root.path());
        let path = remote.path.clone();
        release::atomic_json(
            &path,
            &Publication {
                schema_version: 1,
                release: release.id.clone(),
                commit: commit(),
                status: Status::Prepared,
                updated_at_unix_seconds: 0,
                live_observed_at_unix_seconds: None,
            },
        )
        .unwrap();
        assert_eq!(
            publish_with(&release, &path, &mut remote).unwrap().status,
            Status::Live
        );
        assert_eq!(remote.pushes, 1);
    }

    #[test]
    fn branch_conflicts_never_overwrite_remote_changes() {
        let root = tempfile::tempdir().unwrap();
        let (release, mut remote) = fixture(root.path());
        let path = remote.path.clone();
        publish_with(&release, &path, &mut remote).unwrap();
        remote.conflict = true;
        assert_eq!(
            publish_with(&release, &path, &mut remote).unwrap().status,
            Status::Conflict
        );
        assert_eq!(remote.pushes, 1);
    }

    #[test]
    fn unavailable_ledger_prevents_remote_writes() {
        let root = tempfile::tempdir().unwrap();
        let (release, mut remote) = fixture(root.path());
        assert!(
            publish_with(
                &release,
                &root.path().join("missing/state.json"),
                &mut remote
            )
            .is_err()
        );
        assert_eq!(remote.pushes, 0);
    }
}
