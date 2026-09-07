use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Error;
use anyhow::bail;
use tempfile::TempDir;

const JOURNAL: &str = ".berlin-transaction.json";

pub(super) struct ProjectLock {
    _file: File,
}

impl ProjectLock {
    pub fn acquire(project_root: &Path) -> Result<Self, Error> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(project_root.join(".berlin.lock"))?;
        file.lock()
            .context("Unable to acquire the Berlin project build lock")?;
        recover_interrupted_commit(project_root)?;
        Ok(Self { _file: file })
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct CommitJournal {
    output: PathBuf,
    staging_directory: PathBuf,
    backup_directory: PathBuf,
    had_destination: bool,
}

#[derive(Debug)]
pub(super) struct OutputTransaction {
    project_root: PathBuf,
    staging: TempDir,
    backup: TempDir,
    owned_roots: Vec<PathBuf>,
}

impl OutputTransaction {
    pub fn new<'a>(
        project_root: &Path,
        output_paths: impl IntoIterator<Item = &'a Path>,
    ) -> Result<Self, Error> {
        let owned_roots = validated_output_roots(output_paths)?;
        let staging = tempfile::Builder::new()
            .prefix(".berlin-stage-")
            .tempdir_in(project_root)?;
        let backup = tempfile::Builder::new()
            .prefix(".berlin-backup-")
            .tempdir_in(project_root)?;

        Ok(Self {
            project_root: project_root.to_path_buf(),
            staging,
            backup,
            owned_roots,
        })
    }

    pub fn staging_root(&self) -> &Path {
        self.staging.path()
    }

    pub fn owned_roots(&self) -> &[PathBuf] {
        &self.owned_roots
    }

    pub fn commit(self) -> Result<(), Error> {
        self.commit_with_rename(|from, to| fs::rename(from, to))
    }

    // Keep filesystem failure injection local to the commit protocol.
    fn commit_with_rename(
        self,
        mut rename: impl FnMut(&Path, &Path) -> std::io::Result<()>,
    ) -> Result<(), Error> {
        let Some(relative) = self.owned_roots.first() else {
            return Ok(());
        };
        let paths = self.prepare_commit(relative)?;

        if let Err(error) = paths.back_up(&mut rename) {
            self.remove_journal("Output backup failed");
            return Err(error);
        }
        if let Err(error) = rename(&paths.staged, &paths.destination) {
            return Err(self.restore_or_preserve(&paths, &mut rename, error));
        }

        self.remove_journal("Published output");
        Ok(())
    }

    fn prepare_commit(&self, relative: &Path) -> Result<CommitPaths, Error> {
        let staged = self.staging.path().join(relative);
        if !staged.exists() {
            bail!(
                "pipeline did not materialize declared output '{}'",
                relative.display()
            );
        }

        let destination = self.project_root.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let paths = CommitPaths {
            staged,
            had_destination: destination.exists(),
            destination,
            backup: self.backup.path().join("output"),
        };
        write_journal(
            &self.project_root,
            &CommitJournal {
                output: relative.to_path_buf(),
                staging_directory: directory_name(self.staging.path())?,
                backup_directory: directory_name(self.backup.path())?,
                had_destination: paths.had_destination,
            },
        )?;
        Ok(paths)
    }

    fn restore_or_preserve(
        self,
        paths: &CommitPaths,
        rename: &mut impl FnMut(&Path, &Path) -> std::io::Result<()>,
        install_error: std::io::Error,
    ) -> Error {
        if let Err(recovery_error) = paths.restore(rename) {
            // TempDir cleanup must not discard the only remaining copy of old output.
            let staging_directory = self.staging.keep();
            let backup_directory = self.backup.keep();
            return anyhow::anyhow!(
                "Unable to install output {}: {install_error}; restoring the previous output \
                 also failed: {recovery_error}. Recovery data preserved in {} and {}; journal: {}",
                paths.destination.display(),
                staging_directory.display(),
                backup_directory.display(),
                self.project_root.join(JOURNAL).display(),
            );
        }

        self.remove_journal("Restored output after failed installation");
        Error::new(install_error).context(format!(
            "Unable to install output {}",
            paths.destination.display()
        ))
    }

    fn remove_journal(&self, outcome: &str) {
        if let Err(error) = fs::remove_file(self.project_root.join(JOURNAL)) {
            log::warn!("{outcome} but could not remove transaction journal: {error}");
        }
    }
}

/// Paths and rollback information for one journaled output replacement.
struct CommitPaths {
    staged: PathBuf,
    destination: PathBuf,
    backup: PathBuf,
    had_destination: bool,
}

impl CommitPaths {
    fn back_up(
        &self,
        rename: &mut impl FnMut(&Path, &Path) -> std::io::Result<()>,
    ) -> Result<(), Error> {
        if self.had_destination {
            rename(&self.destination, &self.backup).with_context(|| {
                format!("Unable to back up output {}", self.destination.display())
            })?;
        }
        Ok(())
    }

    fn restore(
        &self,
        rename: &mut impl FnMut(&Path, &Path) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        if self.had_destination {
            rename(&self.backup, &self.destination)?;
        }
        Ok(())
    }
}
/// Selects the outermost declared output paths and requires at most one atomic root.
/// Never infers a common ancestor: replacing it could remove undeclared content.
/// Path confinement is checked by pipeline validation before transaction creation.
fn validated_output_roots<'a>(
    output_paths: impl IntoIterator<Item = &'a Path>,
) -> Result<Vec<PathBuf>, Error> {
    let mut paths = output_paths
        .into_iter()
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| path.components().count());

    let mut owned_roots = Vec::<PathBuf>::new();
    for path in paths {
        if !owned_roots.iter().any(|root| path.starts_with(root)) {
            owned_roots.push(path);
        }
    }
    if owned_roots.len() > 1 {
        bail!(
            "pipeline outputs must share one atomic root; found: {}",
            owned_roots
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(owned_roots)
}

fn directory_name(path: &Path) -> Result<PathBuf, Error> {
    path.file_name()
        .map(PathBuf::from)
        .context("Temporary transaction directory has no name")
}

fn write_journal(project_root: &Path, journal: &CommitJournal) -> Result<(), Error> {
    let mut temporary = tempfile::NamedTempFile::new_in(project_root)?;
    serde_json::to_writer(&mut temporary, journal)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(project_root.join(JOURNAL))
        .map_err(|error| error.error)?;
    Ok(())
}

fn recover_interrupted_commit(project_root: &Path) -> Result<(), Error> {
    let journal_path = project_root.join(JOURNAL);
    if !journal_path.is_file() {
        return Ok(());
    }
    let journal: CommitJournal = serde_json::from_slice(&fs::read(&journal_path)?)?;
    if !is_confined(&journal.output)
        || !is_temporary_directory(&journal.staging_directory, ".berlin-stage-")
        || !is_temporary_directory(&journal.backup_directory, ".berlin-backup-")
    {
        bail!("Refusing unsafe transaction recovery metadata");
    }

    let destination = project_root.join(&journal.output);
    let staging_directory = project_root.join(&journal.staging_directory);
    let staged = staging_directory.join(&journal.output);
    let backup_directory = project_root.join(&journal.backup_directory);
    let backup = backup_directory.join("output");

    if journal.had_destination && backup.exists() {
        if !destination.exists() {
            fs::rename(&backup, &destination)
                .with_context(|| format!("Unable to restore output from {}", backup.display()))?;
        } else if staged.exists() {
            // Installation has not completed. An unexpected destination must not
            // cause recovery to discard the previous output or overwrite new data.
            bail!(
                "Unable to recover output: destination {} already exists; backup preserved at {}; journal: {}",
                destination.display(),
                backup.display(),
                journal_path.display(),
            );
        }
    }

    remove_path(&staging_directory)?;
    remove_path(&backup_directory)?;
    fs::remove_file(journal_path)?;
    log::warn!("Recovered an interrupted Berlin output transaction");
    Ok(())
}

fn is_confined(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn is_temporary_directory(path: &Path, prefix: &str) -> bool {
    path.components().count() == 1
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(prefix))
}

fn remove_path(path: &Path) -> Result<(), Error> {
    if path.is_dir() {
        fs::remove_dir_all(path)?;
    } else if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_outputs_share_their_outer_owned_root() {
        let project = tempfile::tempdir().unwrap();
        let transaction = OutputTransaction::new(
            project.path(),
            [
                Path::new("_site/css/styles.css"),
                Path::new("_site"),
                Path::new("_site/static"),
            ],
        )
        .unwrap();

        assert_eq!(transaction.owned_roots, vec![PathBuf::from("_site")]);
    }

    #[test]
    fn separate_output_roots_are_rejected() {
        let project = tempfile::tempdir().unwrap();
        let error = OutputTransaction::new(
            project.path(),
            [Path::new("_site"), Path::new("_elsewhere")],
        )
        .unwrap_err();

        assert!(error.to_string().contains("one atomic root"));
    }

    #[test]
    fn empty_outputs_have_no_owned_root() {
        assert!(validated_output_roots([]).unwrap().is_empty());
    }

    #[test]
    fn repeated_output_paths_share_one_root() {
        let roots = validated_output_roots([Path::new("_site"), Path::new("_site")]).unwrap();

        assert_eq!(roots, vec![PathBuf::from("_site")]);
    }

    #[test]
    fn sibling_outputs_do_not_imply_ownership_of_their_parent() {
        let error =
            validated_output_roots([Path::new("_site/articles"), Path::new("_site/assets")])
                .unwrap_err();

        assert_eq!(
            error.to_string(),
            "pipeline outputs must share one atomic root; found: _site/articles, _site/assets"
        );
    }

    #[test]
    fn root_validation_precedes_filesystem_setup() {
        let project = tempfile::tempdir().unwrap();
        let missing_root = project.path().join("missing");
        let error =
            OutputTransaction::new(&missing_root, [Path::new("_site"), Path::new("_elsewhere")])
                .unwrap_err();

        assert!(error.to_string().contains("one atomic root"));
        assert_eq!(fs::read_dir(project.path()).unwrap().count(), 0);
    }

    #[test]
    fn commit_replaces_the_complete_owned_tree() {
        let project = tempfile::tempdir().unwrap();
        let old_site = project.path().join("_site");
        fs::create_dir(&old_site).unwrap();
        fs::write(old_site.join("stale.html"), "stale").unwrap();

        let transaction = OutputTransaction::new(project.path(), [Path::new("_site")]).unwrap();
        let staged_site = transaction.staging_root().join("_site");
        fs::create_dir(&staged_site).unwrap();
        fs::write(staged_site.join("index.html"), "new").unwrap();
        transaction.commit().unwrap();

        assert_eq!(
            fs::read_to_string(old_site.join("index.html")).unwrap(),
            "new"
        );
        assert!(!old_site.join("stale.html").exists());
        assert!(!project.path().join(JOURNAL).exists());
    }

    #[test]
    fn installation_failure_restores_previous_output() {
        let project = tempfile::tempdir().unwrap();
        let transaction = replacement_transaction(project.path());
        let staged = transaction.staging.path().to_path_buf();
        let backup = transaction.backup.path().to_path_buf();
        let mut calls = 0;

        let error = transaction
            .commit_with_rename(|from, to| {
                calls += 1;
                if calls == 2 {
                    return Err(std::io::Error::other("injected install failure"));
                }
                fs::rename(from, to)
            })
            .unwrap_err();

        assert_eq!(calls, 3);
        assert!(format!("{error:#}").contains("injected install failure"));
        assert_eq!(
            fs::read_to_string(project.path().join("_site/index.html")).unwrap(),
            "old"
        );
        assert!(!project.path().join(JOURNAL).exists());
        assert!(!staged.exists());
        assert!(!backup.exists());
    }

    #[test]
    fn failed_restoration_preserves_recovery_data_for_the_next_build() {
        let project = tempfile::tempdir().unwrap();
        let transaction = replacement_transaction(project.path());
        let staged = transaction.staging.path().to_path_buf();
        let backup = transaction.backup.path().to_path_buf();
        let mut calls = 0;

        let error = transaction
            .commit_with_rename(|from, to| {
                calls += 1;
                match calls {
                    1 => fs::rename(from, to),
                    2 => Err(std::io::Error::other("injected install failure")),
                    _ => Err(std::io::Error::other("injected restore failure")),
                }
            })
            .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("injected install failure"));
        assert!(message.contains("injected restore failure"));
        assert!(message.contains(&backup.display().to_string()));
        assert!(message.contains(&project.path().join(JOURNAL).display().to_string()));
        assert_eq!(
            fs::read_to_string(backup.join("output/index.html")).unwrap(),
            "old"
        );
        assert_eq!(
            fs::read_to_string(staged.join("_site/index.html")).unwrap(),
            "new"
        );
        assert!(project.path().join(JOURNAL).is_file());

        // A conflicting destination must not cause the next recovery to discard
        // the preserved backup. Once resolved, ordinary recovery can restore it.
        fs::write(project.path().join("_site"), "conflicting output").unwrap();
        let error = recover_interrupted_commit(project.path()).unwrap_err();
        assert!(error.to_string().contains("backup preserved"));
        assert_eq!(
            fs::read_to_string(backup.join("output/index.html")).unwrap(),
            "old"
        );
        assert_eq!(
            fs::read_to_string(project.path().join("_site")).unwrap(),
            "conflicting output"
        );
        assert!(project.path().join(JOURNAL).is_file());
        fs::remove_file(project.path().join("_site")).unwrap();

        let _lock = ProjectLock::acquire(project.path()).unwrap();
        assert_eq!(
            fs::read_to_string(project.path().join("_site/index.html")).unwrap(),
            "old"
        );
        assert!(!project.path().join(JOURNAL).exists());
        assert!(!staged.exists());
        assert!(!backup.exists());
    }

    #[test]
    fn backup_failure_leaves_previous_output_untouched() {
        let project = tempfile::tempdir().unwrap();
        let transaction = replacement_transaction(project.path());
        let error = transaction
            .commit_with_rename(|_, _| Err(std::io::Error::other("injected backup failure")))
            .unwrap_err();

        assert!(format!("{error:#}").contains("injected backup failure"));
        assert_eq!(
            fs::read_to_string(project.path().join("_site/index.html")).unwrap(),
            "old"
        );
        assert!(!project.path().join(JOURNAL).exists());
    }

    #[test]
    fn installation_failure_without_previous_output_needs_no_restoration() {
        let project = tempfile::tempdir().unwrap();
        let transaction = OutputTransaction::new(project.path(), [Path::new("_site")]).unwrap();
        fs::write(transaction.staging_root().join("_site"), "new").unwrap();
        let mut calls = 0;
        let error = transaction
            .commit_with_rename(|_, _| {
                calls += 1;
                Err(std::io::Error::other("injected install failure"))
            })
            .unwrap_err();

        assert_eq!(calls, 1);
        assert!(format!("{error:#}").contains("injected install failure"));
        assert!(!project.path().join("_site").exists());
        assert!(!project.path().join(JOURNAL).exists());
    }

    fn replacement_transaction(project_root: &Path) -> OutputTransaction {
        let destination = project_root.join("_site");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("index.html"), "old").unwrap();
        let transaction = OutputTransaction::new(project_root, [Path::new("_site")]).unwrap();
        let staged = transaction.staging_root().join("_site");
        fs::create_dir(&staged).unwrap();
        fs::write(staged.join("index.html"), "new").unwrap();
        transaction
    }

    #[test]
    fn recovery_keeps_installed_output_when_only_journal_cleanup_was_interrupted() {
        let project = tempfile::tempdir().unwrap();
        let transaction = replacement_transaction(project.path());
        let paths = transaction.prepare_commit(Path::new("_site")).unwrap();
        paths.back_up(&mut |from, to| fs::rename(from, to)).unwrap();
        fs::rename(&paths.staged, &paths.destination).unwrap();
        // Simulate a crash after installation, before journal and TempDir cleanup.
        let staged = transaction.staging.keep();
        let backup = transaction.backup.keep();

        recover_interrupted_commit(project.path()).unwrap();

        assert_eq!(
            fs::read_to_string(project.path().join("_site/index.html")).unwrap(),
            "new"
        );
        assert!(!project.path().join(JOURNAL).exists());
        assert!(!staged.exists());
        assert!(!backup.exists());
    }

    #[test]
    fn recovers_a_crash_after_backing_up_the_previous_output() {
        let project = tempfile::tempdir().unwrap();
        let staging_directory = PathBuf::from(".berlin-stage-test");
        let backup_directory = PathBuf::from(".berlin-backup-test");
        let staged = project.path().join(&staging_directory).join("_site");
        let backup = project.path().join(&backup_directory).join("output");
        fs::create_dir_all(&staged).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(staged.join("index.html"), "new").unwrap();
        fs::write(backup.join("index.html"), "old").unwrap();
        write_journal(
            project.path(),
            &CommitJournal {
                output: "_site".into(),
                staging_directory,
                backup_directory,
                had_destination: true,
            },
        )
        .unwrap();

        recover_interrupted_commit(project.path()).unwrap();

        assert_eq!(
            fs::read_to_string(project.path().join("_site/index.html")).unwrap(),
            "old"
        );
        assert!(!project.path().join(JOURNAL).exists());
    }
}
