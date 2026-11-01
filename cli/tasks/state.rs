//! Project-local durable lifecycle storage. No pipeline evaluation or remote I/O.
use super::{announcement, output::ProjectLock, publication, release};
use crate::project::Project;
use anyhow::{Context, Error, bail};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) mod builds;
pub(crate) mod documents;
pub(crate) mod metrics;

const VERSION: i64 = 4;
const APPLICATION: i64 = 0x424c4e31;
const LIMIT: usize = 128 * 1024;

pub(crate) fn path(root: &Path) -> PathBuf {
    root.join(".berlin/state.sqlite")
}
pub(crate) fn active(root: &Path) -> Result<bool, Error> {
    let directory = root.join(".berlin");
    if !directory.try_exists()? {
        return Ok(false);
    }
    release::ensure_directory(&directory)?;
    match fs::symlink_metadata(path(root)) {
        Ok(_) => {
            release::ensure_file(&path(root))?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if directory.join("state-storage.json").try_exists()? {
                bail!(
                    "SQLite state is missing; restore a backup rather than falling back to archived JSON"
                );
            }
            Ok(false)
        }
        Err(e) => Err(e.into()),
    }
}

pub(crate) struct Store {
    connection: Connection,
}
impl Store {
    pub(crate) fn open(root: &Path) -> Result<Self, Error> {
        if !active(root)? {
            bail!("project state has not been migrated; run 'bln state migrate'");
        }
        let db = Self::connect(&path(root), false)?;
        if db.version()? != VERSION {
            bail!("state database needs upgrading; run 'bln state migrate'");
        }
        Ok(db)
    }
    fn connect(path: &Path, initialize: bool) -> Result<Self, Error> {
        release::ensure_file(path)?;
        // macOS temp roots commonly contain /var -> /private/var. SQLite's
        // NOFOLLOW also rejects ancestor symlinks; resolve the checked file first.
        let path = path.canonicalize()?;
        for suffix in ["-wal", "-shm", "-journal"] {
            let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
            if fs::symlink_metadata(&sidecar).is_ok() {
                release::ensure_file(&sidecar)?;
            }
        }
        let mut connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        connection.busy_timeout(Duration::from_secs(3))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        if initialize {
            let tx = connection.transaction()?;
            tx.execute_batch("CREATE TABLE approvals(id TEXT PRIMARY KEY, payload TEXT NOT NULL, digest TEXT NOT NULL);
                CREATE TABLE reports(id TEXT PRIMARY KEY REFERENCES approvals(id), payload TEXT NOT NULL, digest TEXT NOT NULL);
                CREATE TABLE publications(id TEXT PRIMARY KEY, payload TEXT NOT NULL, digest TEXT NOT NULL);
                CREATE TABLE publication_events(sequence INTEGER PRIMARY KEY, id TEXT NOT NULL REFERENCES publications(id), payload TEXT NOT NULL, digest TEXT NOT NULL);
                PRAGMA application_id=1112297009; PRAGMA user_version=1;")?;
            tx.commit()?;
        }
        let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let app: i64 = connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
        if !(1..=VERSION).contains(&version) || app != APPLICATION {
            bail!("unsupported Berlin state database schema/application ID");
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        Ok(Self { connection })
    }
    fn version(&self) -> Result<i64, Error> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }
    fn upgrade(&mut self, root: &Path) -> Result<(), Error> {
        self.validate()?;
        if self.version()? == VERSION {
            return Ok(());
        }
        let version = self.version()?;
        let receipts = if version < 2 {
            builds::import_legacy(root)?
        } else {
            vec![]
        };
        let releases = if version < 3 {
            documents::scan(&Project::new(root.to_owned(), vec![]))?
        } else {
            vec![]
        };
        let tx = self.connection.transaction()?;
        if version < 2 {
            builds::schema(&tx)?;
            for (name, receipt) in receipts {
                builds::insert(&tx, &receipt, Some(&name))?;
            }
        }
        if version < 3 {
            documents::schema(&tx)?;
            Store::import_releases(&tx, &releases)?;
        }
        metrics::schema(&tx)?;
        tx.pragma_update(None, "user_version", VERSION)?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn history(&self) -> Result<Vec<announcement::HistoryEntry>, Error> {
        let tx = self.connection.unchecked_transaction()?;
        let history =
            announcement::store::validate(self.records("approvals")?, self.records("reports")?)?;
        tx.commit()?;
        Ok(history)
    }
    pub(crate) fn records<T: DeserializeOwned>(
        &self,
        table: &str,
    ) -> Result<BTreeMap<String, T>, Error> {
        let sql = match table {
            "approvals" => "SELECT id,payload,digest FROM approvals ORDER BY id",
            "reports" => "SELECT id,payload,digest FROM reports ORDER BY id",
            "publications" => "SELECT id,payload,digest FROM publications ORDER BY id",
            _ => bail!("invalid lifecycle record type"),
        };
        let mut statement = self.connection.prepare(sql)?;
        let rows = statement.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut records = BTreeMap::new();
        for row in rows {
            let (id, payload, digest) = row?;
            release::validate_id(&id)?;
            verify(&payload, &digest)?;
            records.insert(id, serde_json::from_str(&payload)?);
        }
        Ok(records)
    }
    pub(crate) fn insert(
        &self,
        table: &str,
        id: &str,
        value: &impl Serialize,
    ) -> Result<(), Error> {
        release::validate_id(id)?;
        let payload = encode(value)?;
        let sql = match table {
            "approvals" => "INSERT INTO approvals VALUES (?1,?2,?3)",
            "reports" => "INSERT INTO reports VALUES (?1,?2,?3)",
            _ => bail!("invalid append-only lifecycle record type"),
        };
        self.connection.execute(
            sql,
            params![id, payload, release::digest(payload.as_bytes())],
        )?;
        Ok(())
    }
    pub(crate) fn publication(&self, id: &str) -> Result<Option<publication::Publication>, Error> {
        let row: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT payload,digest FROM publications WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        row.map(|(payload, digest)| {
            verify(&payload, &digest)?;
            publication::decode(payload.as_bytes(), id)
        })
        .transpose()
    }
    pub(crate) fn save_publication(
        &mut self,
        id: &str,
        value: &publication::Publication,
    ) -> Result<(), Error> {
        publication::decode(&serde_json::to_vec(value)?, id)?;
        let payload = encode(value)?;
        let digest = release::digest(payload.as_bytes());
        let tx = self.connection.transaction()?;
        tx.execute("INSERT INTO publications VALUES (?1,?2,?3) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload,digest=excluded.digest",params![id,payload,digest])?;
        tx.execute(
            "INSERT INTO publication_events(id,payload,digest) VALUES (?1,?2,?3)",
            params![id, payload, digest],
        )?;
        tx.commit()?;
        Ok(())
    }
    fn validate(&self) -> Result<(), Error> {
        let tx = self.connection.unchecked_transaction()?;
        let version: i64 = self
            .connection
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        let app: i64 = self
            .connection
            .pragma_query_value(None, "application_id", |r| r.get(0))?;
        if !(1..=VERSION).contains(&version) || app != APPLICATION {
            bail!("unsupported Berlin state database schema/application ID");
        }
        let check: String = self
            .connection
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if check != "ok" {
            bail!("SQLite integrity check failed: {check}");
        }
        if self
            .connection
            .prepare("PRAGMA foreign_key_check")?
            .exists([])?
        {
            bail!("SQLite foreign key check failed");
        }
        announcement::store::validate(self.records("approvals")?, self.records("reports")?)?;
        for (id, value) in self.records::<publication::Publication>("publications")? {
            publication::decode(&serde_json::to_vec(&value)?, &id)?;
        }
        let mut query = self
            .connection
            .prepare("SELECT id,payload,digest FROM publication_events ORDER BY sequence")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let payload: String = row.get(1)?;
            let digest: String = row.get(2)?;
            verify(&payload, &digest)?;
            publication::decode(payload.as_bytes(), &id)?;
        }
        if version >= 2 {
            self.validate_builds()?;
        }
        if version >= 3 {
            self.validate_documents()?;
        }
        if version >= 4 {
            self.validate_metrics()?;
        }
        tx.commit()?;
        Ok(())
    }
}
fn encode(value: &impl Serialize) -> Result<String, Error> {
    let payload = serde_json::to_string(value)?;
    if payload.len() > LIMIT {
        bail!("lifecycle record exceeds its size limit");
    }
    Ok(payload)
}
fn verify(payload: &str, digest: &str) -> Result<(), Error> {
    if payload.len() > LIMIT || release::digest(payload.as_bytes()) != digest {
        bail!("lifecycle record hash/size mismatch");
    }
    Ok(())
}

pub(crate) fn migrate(project: &Project) -> Result<(), Error> {
    let _build = ProjectLock::acquire_without_recovery(project.root())?;
    let _announcements = announcement::store::lock(project)?;
    if active(project.root())? {
        let mut db = Store::connect(&path(project.root()), false)?;
        db.upgrade(project.root())?;
        db.validate()?;
        release::atomic_json(
            &project.root().join(".berlin/state-storage.json"),
            &"sqlite",
        )?;
        return Ok(());
    }
    require_absent_sidecars(project.root())?;
    // Validate all legacy domain records before creating the database.
    let history = announcement::store::load_legacy(project)?;
    let directory = project.root().join(".berlin/publications");
    let mut publications = Vec::new();
    if directory.try_exists()? {
        release::ensure_directory(&directory)?;
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .context("invalid publication filename")?;
            if name.starts_with(".pending-") {
                continue;
            }
            let id = name
                .strip_suffix(".json")
                .context("unexpected file in publication state")?;
            release::validate_id(id)?;
            let value = publication::read_file(&path, id)?
                .context("publication disappeared during migration")?;
            publications.push((id.to_string(), value));
        }
    }
    let parent = project.root().join(".berlin");
    let temp = tempfile::NamedTempFile::new_in(&parent)?;
    let mut db = Store::connect(temp.path(), true)?;
    {
        let tx = db.connection.transaction()?;
        for item in history {
            let payload = encode(&item.approval)?;
            tx.execute(
                "INSERT INTO approvals VALUES (?1,?2,?3)",
                params![
                    item.approval.review_id,
                    payload,
                    release::digest(payload.as_bytes())
                ],
            )?;
            if let Some(report) = item.publication {
                let payload = encode(&report)?;
                tx.execute(
                    "INSERT INTO reports VALUES (?1,?2,?3)",
                    params![
                        report.review_id,
                        payload,
                        release::digest(payload.as_bytes())
                    ],
                )?;
            }
        }
        for (id, value) in publications {
            let payload = encode(&value)?;
            let digest = release::digest(payload.as_bytes());
            tx.execute(
                "INSERT INTO publications VALUES (?1,?2,?3)",
                params![id, payload, digest],
            )?;
            tx.execute(
                "INSERT INTO publication_events(id,payload,digest) VALUES (?1,?2,?3)",
                params![id, payload, digest],
            )?;
        }
        tx.commit()?;
    }
    db.upgrade(project.root())?;
    db.validate()?;
    // Close/checkpoint WAL before the no-clobber authority switch.
    db.connection
        .pragma_update(None, "journal_mode", "DELETE")?;
    drop(db);
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path(project.root()))
        .map_err(|e| e.error)?;
    File::open(&parent)?.sync_all()?;
    release::atomic_json(&parent.join("state-storage.json"), &"sqlite")?;
    Ok(())
}
pub(crate) fn status(project: &Project) -> Result<serde_json::Value, Error> {
    if !active(project.root())? {
        return Ok(serde_json::json!({"backend":"legacy_json","schema_version":null}));
    }
    let db = Store::connect(&path(project.root()), false)?;
    db.validate()?;
    let mut counts = serde_json::Map::new();
    let version = db.version()?;
    let mut tables = vec!["approvals", "reports", "publications", "publication_events"];
    if version >= 2 {
        tables.extend([
            "build_runs",
            "build_inputs",
            "build_outputs",
            "build_diagnostics",
            "build_catalogs",
            "catalog_resources",
        ]);
    }
    if version >= 3 {
        tables.extend(["sealed_releases", "release_documents"]);
    }
    if version >= 4 {
        tables.extend([
            "metric_definitions",
            "metric_publications",
            "metric_observations",
        ]);
    }
    for table in tables {
        let count: i64 =
            db.connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
        counts.insert(table.into(), count.into());
    }
    Ok(
        serde_json::json!({"backend":"sqlite","schema_version":version,"migration_required":version < VERSION,"integrity":"ok","counts":counts}),
    )
}
pub(crate) fn backup(project: &Project, destination: &Path) -> Result<(), Error> {
    if !active(project.root())? {
        bail!("project state has not been migrated");
    }
    let db = Store::connect(&path(project.root()), false)?;
    db.validate()?;
    snapshot(&db, destination)
}
fn snapshot(db: &Store, destination: &Path) -> Result<(), Error> {
    let parent = destination.parent().context("backup has no parent")?;
    release::ensure_directory(parent)?;
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    let mut target = Connection::open(temp.path())?;
    rusqlite::backup::Backup::new(&db.connection, &mut target)?.run_to_completion(
        128,
        Duration::from_millis(10),
        None,
    )?;
    drop(target);
    Store::connect(temp.path(), false)?.validate()?;
    // A closed backup must be self contained, including its last committed WAL.
    let verify = Store::connect(temp.path(), false)?;
    verify
        .connection
        .pragma_update(None, "journal_mode", "DELETE")?;
    drop(verify);
    temp.as_file().sync_all()?;
    temp.persist_noclobber(destination).map_err(|e| e.error)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
fn require_absent_sidecars(root: &Path) -> Result<(), Error> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", path(root).display()));
        match fs::symlink_metadata(sidecar) {
            Ok(_) => bail!(
                "state activation requires absent database sidecars; preserve and inspect existing recovery files"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
pub(crate) fn restore(project: &Project, source: &Path) -> Result<(), Error> {
    let _build = ProjectLock::acquire_without_recovery(project.root())?;
    let _announcements = announcement::store::lock(project)?;
    if fs::symlink_metadata(path(project.root())).is_ok() {
        bail!("restore requires an absent state.sqlite; existing state is never overwritten");
    }
    require_absent_sidecars(project.root())?;
    release::ensure_file(source)?;
    let db = Store {
        connection: Connection::open_with_flags(
            source.canonicalize()?,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?,
    };
    db.validate()?;
    snapshot(&db, &path(project.root()))?;
    release::atomic_json(
        &project.root().join(".berlin/state-storage.json"),
        &"sqlite",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn receipt() -> super::super::receipt::BuildReceipt {
        serde_json::from_value(serde_json::json!({
            "schema_version":1,"berlin_version":"test","pipeline":"site","status":"succeeded",
            "started_at_unix_seconds":1,"duration_ms":2,"inputs":[],"outputs":[],"diagnostics":[],"error":null
        })).unwrap()
    }
    fn version_one(root: &Path) {
        fs::create_dir_all(root.join(".berlin")).unwrap();
        File::create(path(root)).unwrap();
        Store::connect(&path(root), true).unwrap();
    }
    #[test]
    fn document_queries_reject_orphaned_index_rows() {
        let root = tempfile::tempdir().unwrap();
        migrate(&project(root.path())).unwrap();
        let db = Store::open(root.path()).unwrap();
        db.connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        db.connection
            .execute(
                "INSERT INTO release_documents VALUES (?1,'orphan','Title','note.html',?2,?2,1)",
                params!["a".repeat(64), "b".repeat(64)],
            )
            .unwrap();
        db.connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        assert!(db.documents(20).is_err());
        assert!(db.document("orphan", 20).is_err());
        assert!(db.validate().is_err());
    }
    #[test]
    fn invalid_bundle_leaves_schema_two_build_history_intact() {
        let root = tempfile::tempdir().unwrap();
        version_one(root.path());
        let mut db = Store::connect(&path(root.path()), false).unwrap();
        {
            let tx = db.connection.transaction().unwrap();
            builds::schema(&tx).unwrap();
            builds::insert(&tx, &receipt(), None).unwrap();
            tx.pragma_update(None, "user_version", 2).unwrap();
            tx.commit().unwrap();
        }
        let directory = root.path().join(".berlin/releases").join("a".repeat(64));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("manifest.json"), "{}").unwrap();
        assert!(migrate(&project(root.path())).is_err());
        assert_eq!(db.version().unwrap(), 2);
        assert_eq!(
            db.builds(None, 20).unwrap()["builds"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(
            db.connection
                .prepare("SELECT * FROM sealed_releases")
                .is_err()
        );
        db.validate().unwrap();
        let old_backup = root.path().join("schema-two.sqlite");
        backup(&project(root.path()), &old_backup).unwrap();
        let restored = tempfile::tempdir().unwrap();
        let restored_project = project(restored.path());
        restore(&restored_project, &old_backup).unwrap();
        assert_eq!(status(&restored_project).unwrap()["schema_version"], 2);
        migrate(&restored_project).unwrap();
        assert_eq!(
            status(&restored_project).unwrap()["schema_version"],
            VERSION
        );
        assert_eq!(
            Store::open(restored.path())
                .unwrap()
                .builds(None, 20)
                .unwrap()["builds"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn schema_one_upgrade_imports_receipts_once_and_old_backups_remain_restorable() {
        let root = tempfile::tempdir().unwrap();
        let p = project(root.path());
        version_one(root.path());
        let receipts = root.path().join(".berlin/receipts");
        fs::create_dir(&receipts).unwrap();
        let bytes = serde_json::to_vec_pretty(&receipt()).unwrap();
        fs::write(receipts.join("site.json"), &bytes).unwrap();
        assert_eq!(status(&p).unwrap()["migration_required"], true);
        assert!(Store::open(root.path()).is_err());
        let old_backup = root.path().join("v1.sqlite");
        backup(&p, &old_backup).unwrap();
        migrate(&p).unwrap();
        migrate(&p).unwrap();
        assert_eq!(status(&p).unwrap()["counts"]["build_runs"], 1);
        let mut db = Store::open(root.path()).unwrap();
        db.record_build(&receipt()).unwrap();
        db.record_build(&receipt()).unwrap();
        assert_eq!(
            db.builds(Some("site"), 20).unwrap()["builds"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            db.builds(Some("other"), 20).unwrap()["builds"],
            serde_json::json!([])
        );
        assert_eq!(fs::read(receipts.join("site.json")).unwrap(), bytes);
        let v2_backup = root.path().join("v2.sqlite");
        backup(&p, &v2_backup).unwrap();
        let restored = tempfile::tempdir().unwrap();
        restore(&project(restored.path()), &v2_backup).unwrap();
        assert_eq!(
            status(&project(restored.path())).unwrap(),
            status(&p).unwrap()
        );
        let restored_old = tempfile::tempdir().unwrap();
        let old_project = project(restored_old.path());
        restore(&old_project, &old_backup).unwrap();
        assert_eq!(status(&old_project).unwrap()["schema_version"], 1);
        migrate(&old_project).unwrap();
        assert_eq!(status(&old_project).unwrap()["schema_version"], VERSION);
    }
    #[test]
    fn invalid_receipt_leaves_schema_one_and_domain_history_untouched() {
        let root = tempfile::tempdir().unwrap();
        version_one(root.path());
        let directory = root.path().join(".berlin/receipts");
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("site.json"), "{}").unwrap();
        assert!(migrate(&project(root.path())).is_err());
        let db = Store::connect(&path(root.path()), false).unwrap();
        assert_eq!(db.version().unwrap(), 1);
        assert!(db.connection.prepare("SELECT * FROM build_runs").is_err());
    }
    #[test]
    fn build_projections_are_transactional_and_checked_against_receipts() {
        let root = tempfile::tempdir().unwrap();
        migrate(&project(root.path())).unwrap();
        let mut db = Store::open(root.path()).unwrap();
        let mut r = receipt();
        r.inputs.push(super::super::receipt::InputReceipt {
            path: "captured.org".into(),
            sha256: "a".repeat(64),
            content_id: Some("note".into()),
        });
        db.record_build(&r).unwrap();
        {
            let tx = db.connection.transaction().unwrap();
            builds::insert(&tx, &r, None).unwrap();
            // Simulate interruption before commit.
        }
        assert_eq!(
            db.builds(None, 20).unwrap()["builds"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        db.validate().unwrap();
        db.connection
            .execute("UPDATE build_inputs SET sha256=?1", ["b".repeat(64)])
            .unwrap();
        assert!(db.validate().is_err());
    }
    #[test]
    fn upgrade_and_build_insertion_roll_back_on_projection_failure() {
        let root = tempfile::tempdir().unwrap();
        version_one(root.path());
        let mut r = receipt();
        let catalog = builds::CatalogSnapshot {
            node: "duplicate".into(),
            input_digest: "a".repeat(64),
            original_urls_digest: "b".repeat(64),
            inputs: vec![],
            resources: vec![],
        };
        r.catalogs = vec![catalog.clone(), catalog];
        let directory = root.path().join(".berlin/receipts");
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("site.json"), serde_json::to_vec(&r).unwrap()).unwrap();
        let p = project(root.path());
        // A duplicate node violates SQL constraints after the schema DDL and run insert.
        assert!(migrate(&p).is_err());
        let db = Store::connect(&path(root.path()), false).unwrap();
        assert_eq!(db.version().unwrap(), 1);
        assert!(db.connection.prepare("SELECT * FROM build_runs").is_err());
        drop(db);
        fs::remove_file(directory.join("site.json")).unwrap();
        migrate(&p).unwrap();
        let mut db = Store::open(root.path()).unwrap();
        assert!(db.record_build(&r).is_err());
        assert_eq!(
            db.builds(None, 20).unwrap()["builds"],
            serde_json::json!([])
        );
        r.catalogs.clear();
        r.status = "failed".into();
        r.error = Some("rendering refused".into());
        r.stage = Some("execution".into());
        r.diagnostics.push(super::super::receipt::Diagnostic {
            node: "render".into(),
            content_id: "note".into(),
            code: "Unsupported".into(),
            message: "fixture diagnostic".into(),
        });
        db.record_build(&r).unwrap();
        assert_eq!(
            db.build(1).unwrap()["receipt"]["diagnostics"][0]["message"],
            "fixture diagnostic"
        );
        db.validate().unwrap();
    }
    fn project(root: &Path) -> Project {
        Project::new(root.to_path_buf(), vec![])
    }
    fn record(id: &str) -> publication::Publication {
        publication::decode(
            serde_json::to_string(&serde_json::json!({
                "schema_version":1,"release":id,"commit":{"sha":"a".repeat(40),"parent":null},
                "status":"unknown","updated_at_unix_seconds":1,"live_observed_at_unix_seconds":null
            }))
            .unwrap()
            .as_bytes(),
            id,
        )
        .unwrap()
    }
    #[test]
    fn migration_backup_restore_and_subsequent_writes_preserve_history() {
        let root = tempfile::tempdir().unwrap();
        let p = project(root.path());
        let id = "a".repeat(64);
        let directory = release::state_directory(root.path(), "publications").unwrap();
        let legacy = directory.join(format!("{id}.json"));
        release::atomic_json(&legacy, &record(&id)).unwrap();
        let original = fs::read(&legacy).unwrap();
        migrate(&p).unwrap();
        migrate(&p).unwrap();
        assert_eq!(status(&p).unwrap()["counts"]["publication_events"], 1);
        assert_eq!(fs::read(&legacy).unwrap(), original);
        let mut db = Store::open(root.path()).unwrap();
        db.save_publication(&id, &record(&id)).unwrap();
        assert_eq!(status(&p).unwrap()["counts"]["publication_events"], 2);
        let backup_file = root.path().join("backup.sqlite");
        backup(&p, &backup_file).unwrap();
        assert!(backup(&p, &backup_file).is_err());
        let backup_bytes = fs::read(&backup_file).unwrap();
        let restored = tempfile::tempdir().unwrap();
        let dest = project(restored.path());
        restore(&dest, &backup_file).unwrap();
        assert_eq!(status(&dest).unwrap(), status(&p).unwrap());
        assert_eq!(fs::read(&backup_file).unwrap(), backup_bytes);
        assert!(restore(&dest, &backup_file).is_err());
        // Archived JSON is no longer consulted after activation.
        fs::write(&legacy, "broken archive").unwrap();
        assert!(
            Store::open(root.path())
                .unwrap()
                .publication(&id)
                .unwrap()
                .is_some()
        );
    }
    #[test]
    fn state_maintenance_does_not_run_output_recovery() {
        let root = tempfile::tempdir().unwrap();
        let journal = root.path().join(".berlin-transaction.json");
        fs::write(&journal, "unrelated interrupted output").unwrap();
        migrate(&project(root.path())).unwrap();
        assert_eq!(
            fs::read_to_string(journal).unwrap(),
            "unrelated interrupted output"
        );
    }
    #[test]
    fn migration_preserves_orphaned_recovery_files() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".berlin")).unwrap();
        let wal = root.path().join(".berlin/state.sqlite-wal");
        fs::write(&wal, "recovery evidence").unwrap();
        assert!(migrate(&project(root.path())).is_err());
        assert!(!path(root.path()).exists());
        assert_eq!(fs::read_to_string(wal).unwrap(), "recovery evidence");
    }
    #[test]
    fn invalid_legacy_data_cannot_activate_a_partial_database() {
        let root = tempfile::tempdir().unwrap();
        let p = project(root.path());
        let directory = release::state_directory(root.path(), "publications").unwrap();
        let bad = directory.join(format!("{}.json", "a".repeat(64)));
        fs::write(&bad, "{}").unwrap();
        assert!(migrate(&p).is_err());
        assert!(!path(root.path()).exists());
        assert_eq!(fs::read_to_string(bad).unwrap(), "{}");
    }
    #[test]
    fn missing_or_incompatible_database_never_falls_back_to_json() {
        let root = tempfile::tempdir().unwrap();
        let p = project(root.path());
        assert_eq!(status(&p).unwrap()["backend"], "legacy_json");
        assert!(!root.path().join(".berlin").exists());
        migrate(&p).unwrap();
        let db = Store::open(root.path()).unwrap();
        db.connection
            .pragma_update(None, "user_version", 999)
            .unwrap();
        drop(db);
        assert!(status(&p).is_err());
        fs::remove_file(path(root.path())).unwrap();
        assert!(active(root.path()).is_err());
    }
    #[test]
    fn rollback_and_record_hashes_protect_committed_state() {
        let root = tempfile::tempdir().unwrap();
        let p = project(root.path());
        migrate(&p).unwrap();
        let id = "a".repeat(64);
        let mut db = Store::open(root.path()).unwrap();
        db.save_publication(&id, &record(&id)).unwrap();
        {
            let tx = db.connection.transaction().unwrap();
            tx.execute("UPDATE publications SET payload='broken'", [])
                .unwrap();
            // Interruption before commit: transaction rollback on drop.
        }
        assert!(db.publication(&id).unwrap().is_some());
        db.connection
            .execute("UPDATE publications SET payload='broken'", [])
            .unwrap();
        assert!(db.publication(&id).is_err());
        assert!(db.validate().is_err());
    }
    #[cfg(unix)]
    #[test]
    fn database_and_recovery_sidecars_cannot_be_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let p = project(root.path());
        migrate(&p).unwrap();
        let target = root.path().join("sentinel");
        fs::write(&target, "untouched").unwrap();
        let sidecar = PathBuf::from(format!("{}-wal", path(root.path()).display()));
        std::os::unix::fs::symlink(&target, &sidecar).unwrap();
        assert!(Store::open(root.path()).is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "untouched");
    }
}
