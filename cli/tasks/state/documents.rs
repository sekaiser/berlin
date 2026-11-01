//! An index of verified sealed manifests. Bundles remain the authority for bytes.
use super::*;
use release::{Manifest, WebsiteRelease, documents::DocumentRevision};

const MANIFEST_LIMIT: usize = 16 * 1024 * 1024;

pub(super) fn schema(tx: &rusqlite::Transaction<'_>) -> Result<(), Error> {
    tx.execute_batch(
        "CREATE TABLE sealed_releases(id TEXT PRIMARY KEY, manifest TEXT NOT NULL);
        CREATE TABLE release_documents(
            release_id TEXT NOT NULL REFERENCES sealed_releases(id), document_id TEXT NOT NULL,
            title TEXT NOT NULL, path TEXT NOT NULL, content_sha256 TEXT NOT NULL,
            metadata_sha256 TEXT NOT NULL, fingerprint_version INTEGER NOT NULL,
            PRIMARY KEY(release_id,document_id));
        CREATE INDEX release_documents_identity ON release_documents(document_id,release_id);",
    )?;
    Ok(())
}

pub(super) fn scan(project: &Project) -> Result<Vec<WebsiteRelease>, Error> {
    let directory = project.root().join(".berlin/releases");
    if !directory.try_exists()? {
        return Ok(vec![]);
    }
    release::ensure_directory(&directory)?;
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    let mut releases = vec![];
    for path in paths {
        let id = path
            .file_name()
            .and_then(|s| s.to_str())
            .context("invalid release directory name")?;
        if id.starts_with(".preparing-") {
            continue;
        }
        releases.push(
            release::open(project, id)
                .with_context(|| format!("cannot index sealed release {id}"))?,
        );
    }
    Ok(releases)
}

fn entries(manifest: &Manifest) -> impl Iterator<Item = (&DocumentRevision, u8)> {
    manifest.documents.iter().flat_map(|inventory| {
        inventory
            .entries
            .iter()
            .map(|entry| (entry, inventory.fingerprint_version))
    })
}

fn insert(tx: &rusqlite::Transaction<'_>, release: &WebsiteRelease) -> Result<(), Error> {
    release::validate_manifest(&release.manifest, &release.id)?;
    let payload = serde_json::to_string(&release.manifest)?;
    if payload.len() > MANIFEST_LIMIT {
        bail!("sealed manifest exceeds lifecycle index size limit");
    }
    let existing: Option<String> = tx
        .query_row(
            "SELECT manifest FROM sealed_releases WHERE id=?1",
            [&release.id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing != payload {
            bail!("indexed release manifest differs from verified bundle");
        }
        check_entries(tx, &release.id, &release.manifest)?;
        return Ok(());
    }
    tx.execute(
        "INSERT INTO sealed_releases VALUES (?1,?2)",
        params![release.id, payload],
    )?;
    for (entry, version) in entries(&release.manifest) {
        tx.execute(
            "INSERT INTO release_documents VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                release.id,
                entry.id.0,
                entry.title,
                entry.path,
                entry.content_sha256,
                entry.metadata_sha256,
                version
            ],
        )?;
    }
    Ok(())
}

fn check_entries(connection: &Connection, id: &str, manifest: &Manifest) -> Result<(), Error> {
    let mut query = connection.prepare("SELECT document_id,title,path,content_sha256,metadata_sha256,fingerprint_version FROM release_documents WHERE release_id=?1 ORDER BY document_id")?;
    let mut rows = query.query([id])?;
    for (entry, version) in entries(manifest) {
        let row = rows.next()?.context("missing indexed document revision")?;
        if row.get::<_, String>(0)? != entry.id.0
            || row.get::<_, String>(1)? != entry.title
            || row.get::<_, String>(2)? != entry.path
            || row.get::<_, String>(3)? != entry.content_sha256
            || row.get::<_, String>(4)? != entry.metadata_sha256
            || row.get::<_, u8>(5)? != version
        {
            bail!("indexed document revision differs from sealed manifest");
        }
    }
    if rows.next()?.is_some() {
        bail!("extra indexed document revision");
    }
    Ok(())
}

impl Store {
    pub(crate) fn record_release(&mut self, release: &WebsiteRelease) -> Result<(), Error> {
        let tx = self.connection.transaction()?;
        insert(&tx, release)?;
        tx.commit()?;
        Ok(())
    }
    pub(super) fn import_releases(
        tx: &rusqlite::Transaction<'_>,
        releases: &[WebsiteRelease],
    ) -> Result<(), Error> {
        for release in releases {
            insert(tx, release)?;
        }
        Ok(())
    }
    pub(super) fn manifest(&self, id: &str) -> Result<Manifest, Error> {
        let payload: String = self.connection.query_row(
            "SELECT manifest FROM sealed_releases WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        if payload.len() > MANIFEST_LIMIT {
            bail!("indexed release manifest exceeds size limit");
        }
        let manifest = serde_json::from_str(&payload)?;
        release::validate_manifest(&manifest, id)?;
        Ok(manifest)
    }
    pub(super) fn validate_documents(&self) -> Result<(), Error> {
        if self
            .connection
            .prepare("PRAGMA foreign_key_check(release_documents)")?
            .exists([])?
        {
            bail!("indexed document references a missing release");
        }
        let mut query = self
            .connection
            .prepare("SELECT id FROM sealed_releases ORDER BY id")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            check_entries(&self.connection, &id, &self.manifest(&id)?)?;
        }
        Ok(())
    }
    pub(crate) fn documents(&self, limit: usize) -> Result<serde_json::Value, Error> {
        validate_limit(limit)?;
        let tx = self.connection.unchecked_transaction()?;
        self.validate_documents()?;
        let total: i64 = self.connection.query_row(
            "SELECT count(DISTINCT document_id) FROM release_documents",
            [],
            |r| r.get(0),
        )?;
        let mut query = self.connection.prepare("SELECT document_id,count(*),count(DISTINCT content_sha256),count(DISTINCT metadata_sha256)
            FROM release_documents GROUP BY document_id ORDER BY document_id LIMIT ?1")?;
        let documents = query.query_map([limit as i64], |row| {
            let id: String = row.get(0)?;
            let mut names = self.connection.prepare("SELECT DISTINCT title FROM release_documents WHERE document_id=?1 ORDER BY title")?;
            let titles = names.query_map([&id], |r| r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
            Ok(serde_json::json!({"id":id,"titles":titles,"observed_releases":row.get::<_,i64>(1)?,
                "content_revisions":row.get::<_,i64>(2)?,"metadata_revisions":row.get::<_,i64>(3)?}))
        })?.collect::<Result<Vec<_>,_>>()?;
        tx.commit()?;
        Ok(
            serde_json::json!({"documents":documents,"total":total,"order":"document_id","limit":limit,"remote_verified":false,"bundles_reverified":false}),
        )
    }
    pub(crate) fn document(&self, id: &str, limit: usize) -> Result<serde_json::Value, Error> {
        validate_limit(limit)?;
        if id.trim().is_empty() || id.chars().any(char::is_control) {
            bail!("invalid document ID");
        }
        let tx = self.connection.unchecked_transaction()?;
        self.validate_documents()?;
        let history =
            announcement::store::validate(self.records("approvals")?, self.records("reports")?)?;
        let total: i64 = self.connection.query_row(
            "SELECT count(*) FROM release_documents WHERE document_id=?1",
            [id],
            |r| r.get(0),
        )?;
        let mut query = self.connection.prepare("SELECT release_id FROM release_documents WHERE document_id=?1 ORDER BY release_id LIMIT ?2")?;
        let releases = query
            .query_map(params![id, limit as i64], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut observations = vec![];
        for release_id in releases {
            let manifest = self.manifest(&release_id)?;
            check_entries(&self.connection, &release_id, &manifest)?;
            let (entry, version) = entries(&manifest)
                .find(|(entry, _)| entry.id.0 == id)
                .context("document missing from indexed manifest")?;
            let reports = history.iter().filter(|h| h.approval.snapshot.document.id.0 == id && h.approval.snapshot.after_release == release_id)
                .filter_map(|h| h.publication.as_ref().map(|p| serde_json::json!({"review_id":h.approval.review_id,"account":h.approval.snapshot.account,
                    "post_url":p.post_url,"evidence":"user_reported","recorded_at_unix_seconds":p.recorded_at_unix_seconds}))).collect::<Vec<_>>();
            observations.push(serde_json::json!({"release":release_id,"document":entry,"fingerprint_version":version,
                "website_url":manifest.url,"destination":manifest.destination,"pipeline":manifest.pipeline,
                "website_publication":self.publication(&release_id)?,"reported_announcements":reports,
                "build_lineage":self.release_lineage(&release_id,limit)?}));
        }
        let metrics = self.metric_coverage(id)?;
        tx.commit()?;
        Ok(
            serde_json::json!({"id":id,"observations":observations,"total":total,"order":"release_id","limit":limit,"remote_verified":false,"bundles_reverified":false,"metrics":metrics}),
        )
    }
}

fn validate_limit(limit: usize) -> Result<(), Error> {
    if !(1..=1000).contains(&limit) {
        bail!("history limit must be between 1 and 1000");
    }
    Ok(())
}

pub(crate) fn index(project: &Project) -> Result<(), Error> {
    let _build = ProjectLock::acquire_without_recovery(project.root())?;
    let mut db = Store::open(project.root())?;
    let releases = scan(project)?;
    let tx = db.connection.transaction()?;
    Store::import_releases(&tx, &releases)?;
    tx.commit()?;
    Ok(())
}
