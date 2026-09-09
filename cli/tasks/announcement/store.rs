//! Small append-only local ledger. Its lock never runs build/output recovery.
use super::{Approval, HistoryEntry, Publication};
use crate::{project::Project, tasks::release};
use anyhow::{Context, Error, bail};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

const MAX_RECORD_BYTES: usize = 128 * 1024;

pub(super) fn lock(project: &Project) -> Result<File, Error> {
    let directory = release::state_directory(project.root(), "announcements")?;
    let path = directory.join(".lock");
    if exists(&path)? {
        release::ensure_file(&path)?;
    }
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock()
        .context("another announcement writer is active; retry after it finishes")?;
    Ok(file)
}

pub(super) fn load(project: &Project) -> Result<Vec<HistoryEntry>, Error> {
    let state = project.root().join(".berlin");
    for directory in [&state, &state.join("announcements")] {
        if !exists(directory)? {
            return Ok(Vec::new());
        }
        release::ensure_directory(directory)?;
    }
    let approvals = read_records::<Approval>(&state.join("announcements/approvals"))?;
    let mut publications = read_records::<Publication>(&state.join("announcements/reported"))?;
    let mut history = Vec::<HistoryEntry>::new();
    for (id, approval) in approvals {
        approval.snapshot.validate()?;
        if approval.review_id != id
            || approval.snapshot.id()? != id
            || !approval.snapshot.approval_allowed()
        {
            bail!("announcement approval does not match its review ID or text policy");
        }
        let publication = publications.remove(&id);
        if let Some(publication) = &publication {
            if publication.schema_version != 1
                || publication.review_id != id
                || super::post_url(publication.post_url.as_str())? != publication.post_url
            {
                bail!("invalid manual publication record");
            }
            for entry in &history {
                if let Some(previous) = &entry.publication
                    && (previous.post_url == publication.post_url
                        || entry.approval.snapshot.same_revision(&approval.snapshot))
                {
                    bail!("conflicting duplicate manual publication records");
                }
            }
        }
        history.push(HistoryEntry {
            approval,
            publication,
        });
    }
    if !publications.is_empty() {
        bail!("manual publication record has no matching approval");
    }
    Ok(history)
}

pub(super) fn approve(project: &Project, approval: &Approval) -> Result<(), Error> {
    let directory = release::state_directory(project.root(), "announcements/approvals")?;
    write_new(
        &directory.join(format!("{}.json", approval.review_id)),
        approval,
    )
}

pub(super) fn record(project: &Project, publication: &Publication) -> Result<(), Error> {
    let directory = release::state_directory(project.root(), "announcements/reported")?;
    write_new(
        &directory.join(format!("{}.json", publication.review_id)),
        publication,
    )
}

fn write_new(path: &Path, record: &impl Serialize) -> Result<(), Error> {
    let mut bytes = serde_json::to_vec_pretty(record)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_RECORD_BYTES {
        bail!("announcement record exceeds its size limit");
    }
    let parent = path.parent().context("announcement record has no parent")?;
    let mut file = tempfile::Builder::new()
        .prefix(".pending-")
        .tempfile_in(parent)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path).map_err(|error| error.error).context("announcement record already exists or could not be saved; no existing record was replaced")?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn read_records<T: DeserializeOwned>(directory: &Path) -> Result<BTreeMap<String, T>, Error> {
    if !exists(directory)? {
        return Ok(BTreeMap::new());
    }
    release::ensure_directory(directory)?;
    let mut paths = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<PathBuf>, _>>()?;
    paths.sort();
    let mut records = BTreeMap::new();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("invalid announcement record filename")?;
        // Uncommitted writes are not records. A later successful write is exclusive.
        if name.starts_with(".pending-") {
            continue;
        }
        let id = name
            .strip_suffix(".json")
            .context("unexpected file in announcement state")?;
        release::validate_id(id)?;
        let text = super::review::read_text(&path, MAX_RECORD_BYTES as u64)?;
        records.insert(
            id.to_owned(),
            serde_json::from_str(&text)
                .with_context(|| format!("invalid announcement record {name}"))?,
        );
    }
    Ok(records)
}

fn exists(path: &Path) -> Result<bool, Error> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_are_exclusive_and_reject_records_the_reader_cannot_accept() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("record.json");
        write_new(&path, &"original").unwrap();
        assert!(write_new(&path, &"replacement").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "\"original\"\n");
        let oversized = root.path().join("oversized.json");
        assert!(write_new(&oversized, &"x".repeat(MAX_RECORD_BYTES)).is_err());
        assert!(!oversized.exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
