use super::*;

fn fixture() -> (tempfile::TempDir, Store, String) {
    let root = tempfile::tempdir().unwrap();
    let project = Project::new(root.path().into(), vec![]);
    migrate(&project).unwrap();
    let mut db = Store::open(root.path()).unwrap();
    let id = article(&mut db, "initial", "notes/note.html");
    (root, db, id)
}

/// Synthetic manifests exercise the storage contract without any personal content.
fn article(db: &mut Store, body: &str, path: &str) -> String {
    let manifest = release::Manifest {
        schema_version: 2,
        berlin_version: "test".into(),
        pipeline: "site".into(),
        website_node: "site".into(),
        url: url::Url::parse("https://example.test/notebook").unwrap(),
        destination: berlin_core::DeploymentTarget::GitHubPages {
            repository: "example/site".into(),
        },
        files: vec![release::ReleaseFile {
            path: path.into(),
            bytes: 0,
            sha256: release::digest(b""),
        }],
        documents: Some(release::documents::DocumentInventory {
            fingerprint_version: 1,
            entries: vec![release::documents::DocumentRevision {
                id: berlin_document::ContentId("note".into()),
                title: "Synthetic note".into(),
                path: path.into(),
                content_sha256: release::digest(body.as_bytes()),
                metadata_sha256: release::digest(b"metadata"),
            }],
        }),
    };
    let id = release::digest(&serde_json::to_vec(&manifest).unwrap());
    db.record_release(&release::WebsiteRelease {
        id: id.clone(),
        manifest,
        directory: PathBuf::new(),
    })
    .unwrap();
    id
}
fn definition() -> Definition {
    Definition {
        name: "example.reads".into(),
        version: 1,
        description: "Synthetic lifetime counter used only in tests.".into(),
        unit: Unit::Count,
        kind: Kind::LifetimeTotal,
    }
}
fn observation(definition: &str, release: &str) -> Observation {
    Observation {
        schema_version: 1,
        definition: definition.into(),
        target: Target::Website {
            release: release.into(),
            document: "note".into(),
        },
        observed_at_unix_seconds: 500,
        window: None,
        value: Value::Count(20),
        source: Source {
            collector: "synthetic-test-account".into(),
            record_key: "snapshot-1".into(),
            method: CollectionMethod::Manual,
        },
    }
}
fn count(db: &Store, table: &str) -> i64 {
    db.connection
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn definitions_are_versioned_and_replays_never_overwrite_observations() {
    let (_root, mut db, release) = fixture();
    let mut definition = definition();
    let id = db.register_metric(&definition).unwrap();
    assert_eq!(db.register_metric(&definition).unwrap(), id);
    definition.description = "Changed meaning".into();
    assert!(db.register_metric(&definition).is_err());
    definition.version = 2;
    assert_ne!(db.register_metric(&definition).unwrap(), id);
    let mut data = observation(&id, &release);
    data.value = Value::Count(u64::MAX);
    let first = db.record_metric(&data).unwrap();
    assert_eq!(db.record_metric(&data).unwrap(), first);
    assert_eq!(count(&db, "metric_observations"), 1);
    assert_eq!(
        db.metrics(Some("note"), 20).unwrap()["observations"][0]["observation"]["value"]["value"]
            .as_u64(),
        Some(u64::MAX)
    );
    data.value = Value::Count(1);
    assert!(db.record_metric(&data).is_err());
    data.source.record_key = "snapshot-2".into();
    data.observed_at_unix_seconds = 499; // Late or corrected snapshots may be lower/older.
    assert_ne!(db.record_metric(&data).unwrap(), first);
    let history = db.metrics(Some("note"), 1).unwrap();
    assert_eq!(history["total"], 2);
    assert_eq!(
        history["observations"][0]["observation"]["value"]["value"],
        1
    );
    assert_eq!(count(&db, "metric_definitions"), 2);
    db.validate().unwrap();
}

#[test]
fn url_metrics_span_revisions_and_keep_route_and_collector_scopes_separate() {
    let (_root, mut db, first_release) = fixture();
    let definition = db.register_metric(&definition()).unwrap();
    let mut data = observation(&definition, &first_release);
    db.record_metric(&data).unwrap();
    let second_release = article(&mut db, "edited", "notes/note.html");
    data.target = Target::Website {
        release: second_release,
        document: "note".into(),
    };
    data.source.record_key = "snapshot-2".into();
    db.record_metric(&data).unwrap();
    assert_eq!(count(&db, "metric_publications"), 1);
    let history = db.metrics(Some("note"), 20).unwrap();
    assert_eq!(
        history["observations"][0]["publication"]["url"],
        "https://example.test/notebook/notes/note.html"
    );
    assert_eq!(
        history["observations"][0]["publication_id"],
        history["observations"][1]["publication_id"]
    );
    data.source.collector = "other-dataset".into();
    db.record_metric(&data).unwrap();
    let third_release = article(&mut db, "edited", "notes/renamed.html");
    data.target = Target::Website {
        release: third_release,
        document: "note".into(),
    };
    db.record_metric(&data).unwrap();
    assert_eq!(count(&db, "metric_publications"), 2);
    assert_eq!(db.metrics(Some("other"), 20).unwrap()["total"], 0);
    assert_eq!(db.metrics(None, 20).unwrap()["total"], 4);
}

#[test]
fn values_and_reporting_periods_must_match_definitions() {
    let (_root, mut db, release) = fixture();
    let mut def = definition();
    def.kind = Kind::IntervalTotal;
    let id = db.register_metric(&def).unwrap();
    let mut data = observation(&id, &release);
    assert!(db.record_metric(&data).is_err());
    for (start, end) in [(300, 200), (200, 200), (200, 501)] {
        data.window = Some(Window {
            start_unix_seconds: start,
            end_unix_seconds: end,
        });
        assert!(db.record_metric(&data).is_err());
    }
    data.window = Some(Window {
        start_unix_seconds: 200,
        end_unix_seconds: 500,
    });
    db.record_metric(&data).unwrap();
    def.name = "example.ratio".into();
    def.unit = Unit::Ratio;
    assert!(db.register_metric(&def).is_err());
    def.kind = Kind::Gauge;
    let ratio = db.register_metric(&def).unwrap();
    data = observation(&ratio, &release);
    for value in [
        Value::Count(1),
        Value::Measure(f64::NAN),
        Value::Measure(f64::INFINITY),
        Value::Measure(-0.1),
        Value::Measure(1.1),
    ] {
        data.value = value;
        assert!(db.record_metric(&data).is_err());
    }
    data.value = Value::Measure(0.25);
    db.record_metric(&data).unwrap();
    data.source.record_key = "future".into();
    data.observed_at_unix_seconds = u64::MAX;
    assert!(db.record_metric(&data).is_err());
    assert_eq!(count(&db, "metric_observations"), 2);
}

#[test]
fn unknown_targets_and_storage_errors_do_not_leave_partial_records() {
    let (_root, mut db, release) = fixture();
    let definition = db.register_metric(&definition()).unwrap();
    let mut data = observation(&definition, &"f".repeat(64));
    assert!(db.record_metric(&data).is_err());
    data.target = Target::Website {
        release: release.clone(),
        document: "unknown".into(),
    };
    assert!(db.record_metric(&data).is_err());
    data = observation(&definition, &release);
    db.connection.execute_batch("CREATE TRIGGER refuse_metric BEFORE INSERT ON metric_observations BEGIN SELECT RAISE(ABORT,'synthetic refusal'); END;").unwrap();
    assert!(db.record_metric(&data).is_err());
    assert_eq!(count(&db, "metric_publications"), 0);
    assert_eq!(count(&db, "metric_observations"), 0);
    db.connection
        .execute_batch("DROP TRIGGER refuse_metric")
        .unwrap();
    let id = db.record_metric(&data).unwrap();
    db.connection
        .execute(
            "UPDATE metric_observations SET collector='forged' WHERE id=?1",
            [id],
        )
        .unwrap();
    assert!(db.metrics(None, 20).is_err());
    assert!(db.record_metric(&data).is_err());
    assert!(db.validate().is_err());
}

#[test]
fn social_targets_require_a_publication_report_and_remain_account_scoped() {
    let (_root, mut db, release) = fixture();
    let manifest = db.manifest(&release).unwrap();
    let snapshot:announcement::Snapshot=serde_json::from_value(serde_json::json!({
        "schema_version":1,"channel":"linked_in","account":"synthetic","before_release":release,"after_release":release,
        "website_url":manifest.url,"website_destination":manifest.destination,"fingerprint_version":1,
        "document":manifest.documents.as_ref().unwrap().entries[0],"kind":"updated",
        "article_url":"https://example.test/notebook/notes/note.html","text":"Synthetic test announcement.",
        "text_sha256":release::digest(b"Synthetic test announcement.")
    })).unwrap();
    let review_id = release::digest(&serde_json::to_vec(&snapshot).unwrap());
    let approval = announcement::Approval {
        review_id: review_id.clone(),
        snapshot,
        approved_at_unix_seconds: 1,
    };
    db.insert("approvals", &review_id, &approval).unwrap();
    let definition = db.register_metric(&definition()).unwrap();
    let mut data = observation(&definition, &release);
    data.target = Target::ReportedAnnouncement {
        review_id: review_id.clone(),
    };
    assert!(db.record_metric(&data).is_err());
    let report:announcement::Publication=serde_json::from_value(serde_json::json!({"schema_version":1,"review_id":review_id,
        "post_url":"https://www.linkedin.com/posts/synthetic-test","evidence":"user_reported","recorded_at_unix_seconds":2})).unwrap();
    db.insert("reports", &review_id, &report).unwrap();
    db.record_metric(&data).unwrap();
    let history = db.metrics(Some("note"), 20).unwrap();
    assert_eq!(
        history["observations"][0]["publication"]["channel"],
        "linkedin"
    );
    assert_eq!(
        history["observations"][0]["publication"]["account"],
        "synthetic"
    );
    assert_eq!(history["remote_verified"], false);
    db.validate().unwrap();
}

#[test]
fn backups_restore_definitions_and_observations_without_collectors_or_bundle_files() {
    let (root, mut db, release) = fixture();
    let definition = db.register_metric(&definition()).unwrap();
    db.record_metric(&observation(&definition, &release))
        .unwrap();
    let history = db.metrics(None, 20).unwrap();
    let project = Project::new(root.path().into(), vec![]);
    let file = root.path().join("metrics.sqlite");
    backup(&project, &file).unwrap();
    let restored = tempfile::tempdir().unwrap();
    restore(&Project::new(restored.path().into(), vec![]), &file).unwrap();
    let restored = Store::open(restored.path()).unwrap();
    assert_eq!(restored.metrics(None, 20).unwrap(), history);
    assert_eq!(
        restored.metric_definitions(20).unwrap(),
        db.metric_definitions(20).unwrap()
    );
    restored.validate().unwrap();
}

#[test]
fn article_inspection_reports_metric_coverage_without_combining_measurements() {
    let (_root, mut db, release) = fixture();
    let empty = db.document("note", 20).unwrap()["metrics"].clone();
    assert_eq!(empty["status"], "no_observations");
    assert_eq!(empty["observation_count"], 0);
    assert!(empty["last_observed_at_unix_seconds"].is_null());
    let definition_id = db.register_metric(&definition()).unwrap();
    let mut data = observation(&definition_id, &release);
    db.record_metric(&data).unwrap();
    let mut ratio = definition();
    ratio.name = "example.ratio".into();
    ratio.unit = Unit::Ratio;
    ratio.kind = Kind::Gauge;
    let ratio_id = db.register_metric(&ratio).unwrap();
    data.definition = ratio_id;
    data.value = Value::Measure(0.25);
    data.observed_at_unix_seconds = 400;
    db.record_metric(&data).unwrap();
    let second = article(&mut db, "edited", "notes/note.html");
    data.target = Target::Website {
        release: second,
        document: "note".into(),
    };
    data.source.record_key = "snapshot-2".into();
    db.record_metric(&data).unwrap();
    let inspected = db.document("note", 20).unwrap();
    assert_eq!(inspected["total"], 2);
    let coverage = &inspected["metrics"];
    assert_eq!(coverage["status"], "observations_recorded");
    assert_eq!(coverage["observation_count"], 3);
    assert_eq!(coverage["publication_count"], 1);
    assert_eq!(coverage["definition_count"], 2);
    assert_eq!(coverage["last_observed_at_unix_seconds"], 500);
    assert_eq!(coverage["collectors_enabled"], false);
    assert_eq!(coverage["validation"], "metadata_only");
    assert_eq!(
        db.document("other", 20).unwrap()["metrics"]["status"],
        "no_observations"
    );
}

#[test]
fn limited_queries_verify_returned_records_while_full_checks_cover_all_history() {
    let (root, mut db, release) = fixture();
    let definition = db.register_metric(&definition()).unwrap();
    let mut data = observation(&definition, &release);
    let first = db.record_metric(&data).unwrap();
    data.source.record_key = "snapshot-2".into();
    let second = db.record_metric(&data).unwrap();
    db.connection
        .execute(
            "UPDATE metric_observations SET payload='broken' WHERE id=?1",
            [first],
        )
        .unwrap();
    let limited = db.metrics(Some("note"), 1).unwrap();
    assert_eq!(limited["total"], 2);
    assert_eq!(limited["observations"][0]["id"], second);
    assert_eq!(limited["validation"], "returned_observations");
    assert!(db.metrics(Some("note"), 2).is_err());
    assert_eq!(
        db.metric_definitions(20).unwrap()["definitions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        db.metric_definitions(20).unwrap()["validation"],
        "returned_definitions"
    );
    let project = Project::new(root.path().into(), vec![]);
    assert!(status(&project).is_err());
    assert!(backup(&project, &root.path().join("must-not-exist.sqlite")).is_err());
    assert!(!root.path().join("must-not-exist.sqlite").exists());
}

#[test]
fn schema_three_upgrade_is_atomic_and_does_not_reimport_archives_or_require_bundles() {
    let (root, db, release) = fixture();
    let old_history = db.document("note", 20).unwrap();
    let old_manifest = db.manifest(&release).unwrap();
    db.connection.execute_batch("DROP TABLE metric_observations; DROP TABLE metric_publications; DROP TABLE metric_definitions; PRAGMA user_version=3;").unwrap();
    let project = Project::new(root.path().into(), vec![]);
    let backup_file = root.path().join("schema-three.sqlite");
    backup(&project, &backup_file).unwrap();
    db.connection
        .execute_batch("CREATE TABLE metric_definitions(block_upgrade TEXT);")
        .unwrap();
    assert!(migrate(&project).is_err());
    assert_eq!(db.version().unwrap(), 3);
    assert!(
        db.connection
            .prepare("SELECT * FROM metric_publications")
            .is_err()
    );
    assert_eq!(db.manifest(&release).unwrap(), old_manifest);
    db.validate_documents().unwrap();
    db.connection
        .execute_batch("DROP TABLE metric_definitions")
        .unwrap();
    let directory = root.path().join(".berlin/releases").join("a".repeat(64));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("manifest.json"), "broken archive").unwrap();
    fs::create_dir_all(root.path().join(".berlin/receipts")).unwrap();
    fs::write(
        root.path().join(".berlin/receipts/site.json"),
        "broken archive",
    )
    .unwrap();
    migrate(&project).unwrap();
    assert_eq!(db.version().unwrap(), VERSION);
    assert_eq!(db.document("note", 20).unwrap(), old_history);
    assert_eq!(count(&db, "metric_observations"), 0);
    let restored = tempfile::tempdir().unwrap();
    let restored_project = Project::new(restored.path().into(), vec![]);
    restore(&restored_project, &backup_file).unwrap();
    assert_eq!(status(&restored_project).unwrap()["schema_version"], 3);
    migrate(&restored_project).unwrap();
    assert_eq!(
        Store::open(restored.path())
            .unwrap()
            .document("note", 20)
            .unwrap(),
        old_history
    );
}
