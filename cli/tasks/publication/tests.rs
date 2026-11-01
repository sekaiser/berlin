use super::*;
use crate::tasks::release::Manifest;

struct FakeRemote {
    path: std::path::PathBuf,
    pushes: usize,
    prepares: usize,
    head: Option<String>,
    build: Observation,
    fail_preflight: bool,
    fail_before_push: bool,
    fail_after_push: bool,
    fail_observe: bool,
    restore_parent_after_push: bool,
    change_head_on_prepare: bool,
}
fn commit() -> Commit {
    Commit {
        sha: "a".repeat(40),
        parent: Some("b".repeat(40)),
    }
}
impl PagesRemote for FakeRemote {
    fn preflight(&mut self, _: &WebsiteRelease) -> Result<(), Error> {
        if self.fail_preflight {
            bail!("preflight unavailable");
        }
        Ok(())
    }
    fn prepare(&mut self, release: &WebsiteRelease) -> Result<Commit, Error> {
        self.prepares += 1;
        if self.change_head_on_prepare {
            self.head = Some("d".repeat(40));
        }
        Ok(Commit {
            sha: release.id[..40].into(),
            parent: self.head.clone(),
        })
    }
    fn push(&mut self, commit: &Commit) -> Result<(), Error> {
        let record: Publication = if state_root(&self.path).is_some() {
            read(
                &self.path,
                &self.path.file_stem().unwrap().to_string_lossy(),
            )?
            .unwrap()
        } else {
            serde_json::from_slice(&fs::read(&self.path)?)?
        };
        assert_eq!(record.status, Status::Prepared, "intent must precede push");
        assert_eq!(&record.commit, commit);
        assert_eq!(
            self.head, commit.parent,
            "the expected parent must still hold"
        );
        self.pushes += 1;
        if self.fail_before_push {
            bail!("connection failed before upload");
        }
        if !self.restore_parent_after_push {
            self.head = Some(commit.sha.clone());
        }
        if self.fail_after_push {
            bail!("response lost");
        }
        Ok(())
    }
    fn observe(&mut self, commit: &Commit) -> Result<Observation, Error> {
        if self.fail_observe {
            bail!("remote branch unavailable");
        }
        Ok(if self.head.as_deref() == Some(commit.sha.as_str()) {
            self.build
        } else if self.head == commit.parent {
            Observation::AtParent
        } else {
            Observation::Conflict
        })
    }
}
fn fixture(root: &Path) -> (WebsiteRelease, FakeRemote) {
    (
        WebsiteRelease {
            id: "a".repeat(64),
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
                documents: None,
            },
        },
        FakeRemote {
            path: root.join("publication.json"),
            pushes: 0,
            prepares: 0,
            head: Some("b".repeat(40)),
            build: Observation::Live,
            fail_preflight: false,
            fail_before_push: false,
            fail_after_push: false,
            fail_observe: false,
            restore_parent_after_push: false,
            change_head_on_prepare: false,
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
fn acknowledged_push_at_parent_is_not_retryable() {
    let root = tempfile::tempdir().unwrap();
    let (release, mut remote) = fixture(root.path());
    remote.restore_parent_after_push = true;
    let path = remote.path.clone();

    assert_eq!(
        publish_with(&release, &path, &mut remote).unwrap().status,
        Status::Conflict
    );
    assert_eq!(
        publish_with(&release, &path, &mut remote).unwrap().status,
        Status::Conflict
    );
    assert_eq!(
        remote.pushes, 1,
        "a confirmed push must not become retryable"
    );
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
    remote.head = Some("c".repeat(40));
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

#[test]
fn observations_follow_one_transition_table_for_all_recorded_states() {
    for current in [
        Status::Prepared,
        Status::Unknown,
        Status::Uploaded,
        Status::Live,
        Status::Failed,
        Status::Conflict,
    ] {
        let at_parent = if matches!(current, Status::Prepared | Status::Unknown) {
            Status::Prepared
        } else {
            Status::Conflict
        };
        for (observation, expected) in [
            (Observation::AtParent, at_parent),
            (Observation::Uploaded, Status::Uploaded),
            (Observation::Live, Status::Live),
            (Observation::Failed, Status::Failed),
            (Observation::Conflict, Status::Conflict),
        ] {
            assert_eq!(
                current.after_observation(observation),
                expected,
                "{current:?} after {observation:?}"
            );
        }
    }
}

#[test]
fn a_push_that_never_arrived_can_retry_the_exact_recorded_commit() {
    let root = tempfile::tempdir().unwrap();
    let (release, mut remote) = fixture(root.path());
    let path = remote.path.clone();
    remote.fail_before_push = true;
    assert!(publish_with(&release, &path, &mut remote).is_err());
    let interrupted = read(&path, &release.id).unwrap().unwrap();
    assert_eq!(interrupted.status, Status::Unknown);
    remote.fail_before_push = false;

    let recovered = publish_with(&release, &path, &mut remote).unwrap();
    assert_eq!(recovered.commit, interrupted.commit);
    assert_eq!(recovered.status, Status::Live);
    assert_eq!(remote.pushes, 2);
}

#[test]
fn losing_remote_visibility_preserves_acknowledgment_and_blocks_blind_retries() {
    let root = tempfile::tempdir().unwrap();
    let (release, mut remote) = fixture(root.path());
    let path = remote.path.clone();
    remote.fail_observe = true;
    assert_eq!(
        publish_with(&release, &path, &mut remote).unwrap().status,
        Status::Uploaded
    );
    let acknowledged = fs::read(&path).unwrap();

    assert!(publish_with(&release, &path, &mut remote).is_err());
    assert_eq!(fs::read(&path).unwrap(), acknowledged);
    assert_eq!(remote.pushes, 1);
    assert_eq!(remote.prepares, 1);

    remote.fail_observe = false;
    assert_eq!(
        publish_with(&release, &path, &mut remote).unwrap().status,
        Status::Live
    );
    assert_eq!(remote.pushes, 1);
}

#[test]
fn recovery_records_a_conflict_if_the_parent_changes_during_preparation() {
    let root = tempfile::tempdir().unwrap();
    let (release, mut remote) = fixture(root.path());
    let path = remote.path.clone();
    let mut record = Publication::prepared(&release, commit());
    save(&path, &mut record).unwrap();
    remote.change_head_on_prepare = true;

    assert!(publish_with(&release, &path, &mut remote).is_err());
    let recorded = read(&path, &release.id).unwrap().unwrap();
    assert_eq!(recorded.status, Status::Conflict);
    assert_eq!(recorded.commit, record.commit);
    assert_eq!(remote.pushes, 0);
}

#[test]
fn a_successive_release_keeps_its_parent_and_never_republishes_the_old_release() {
    let root = tempfile::tempdir().unwrap();
    let (first, mut remote) = fixture(root.path());
    let first_path = remote.path.clone();
    let first_record = publish_with(&first, &first_path, &mut remote).unwrap();
    let second = WebsiteRelease {
        id: "c".repeat(64),
        directory: first.directory.clone(),
        manifest: first.manifest.clone(),
    };
    remote.path = root.path().join("second.json");
    let second_path = remote.path.clone();
    let second_record = publish_with(&second, &second_path, &mut remote).unwrap();
    assert_eq!(second_record.commit.parent, Some(first_record.commit.sha));
    assert_eq!(
        publish_with(&second, &second_path, &mut remote)
            .unwrap()
            .status,
        Status::Live
    );

    remote.path = first_path.clone();
    let old = publish_with(&first, &first_path, &mut remote).unwrap();
    assert_eq!(old.status, Status::Conflict);
    assert_eq!(
        old.live_observed_at_unix_seconds,
        first_record.live_observed_at_unix_seconds
    );
    assert_eq!(remote.head, Some(second_record.commit.sha));
    assert_eq!(remote.pushes, 2);
    assert_eq!(remote.prepares, 2);
}

#[test]
fn pending_and_failed_builds_are_refreshed_without_reuploading() {
    let root = tempfile::tempdir().unwrap();
    let (release, mut remote) = fixture(root.path());
    let path = remote.path.clone();
    for (build, expected) in [
        (Observation::Uploaded, Status::Uploaded),
        (Observation::Failed, Status::Failed),
        (Observation::Live, Status::Live),
    ] {
        remote.build = build;
        assert_eq!(
            publish_with(&release, &path, &mut remote).unwrap().status,
            expected
        );
    }
    assert_eq!(remote.pushes, 1);
    assert_eq!(remote.prepares, 1);
}

#[test]
fn preflight_failure_leaves_existing_records_and_remote_branch_untouched() {
    let root = tempfile::tempdir().unwrap();
    let (release, mut remote) = fixture(root.path());
    let path = remote.path.clone();
    remote.fail_preflight = true;
    assert!(publish_with(&release, &path, &mut remote).is_err());
    assert!(!path.exists());
    assert_eq!(remote.prepares, 0);

    let mut record = Publication::prepared(&release, commit());
    save(&path, &mut record).unwrap();
    let original = fs::read(&path).unwrap();
    assert!(publish_with(&release, &path, &mut remote).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(remote.pushes, 0);
}

#[test]
fn sqlite_intent_and_lost_acknowledgment_survive_reopening_without_duplicate_push() {
    let root = tempfile::tempdir().unwrap();
    let project = crate::project::Project::new(root.path().to_path_buf(), vec![]);
    crate::tasks::state::migrate(&project).unwrap();
    let (release, mut remote) = fixture(root.path());
    let directory = release::state_directory(root.path(), "publications").unwrap();
    remote.path = directory.join(format!("{}.json", release.id));
    let path = remote.path.clone();
    remote.fail_after_push = true;
    assert!(publish_with(&release, &path, &mut remote).is_err());
    assert_eq!(
        read(&path, &release.id).unwrap().unwrap().status,
        Status::Unknown
    );
    assert!(!path.exists(), "SQLite is the only writer after migration");
    assert_eq!(
        publish_with(&release, &path, &mut remote).unwrap().status,
        Status::Live
    );
    assert_eq!(remote.pushes, 1);
    assert_eq!(
        crate::tasks::state::status(&project).unwrap()["counts"]["publication_events"],
        3
    );
}
