//! Append-only observations of completed attempts, never execution inputs.
use super::*;
use crate::tasks::receipt::BuildReceipt;
use berlin_publication::{
    PublicationCatalog, ResourceDescriptor, ResourceHandle, ResourceInputEvidence, ResourceSource,
};
use serde::{Deserialize, Serialize};

const BUILD_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CatalogSnapshot {
    pub node: String,
    pub input_digest: String,
    pub original_urls_digest: String,
    pub inputs: Vec<ResourceInputEvidence>,
    pub resources: Vec<ResourceObservation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResourceObservation {
    pub descriptor: ResourceDescriptor,
    pub source: ResourceSource,
    pub original_url: Option<String>,
    pub document_digest: Option<String>,
}
impl CatalogSnapshot {
    pub(crate) fn capture(node: String, catalog: &PublicationCatalog) -> Self {
        Self {
            node,
            input_digest: catalog.input_digest().into(),
            original_urls_digest: catalog.original_urls_digest().into(),
            inputs: catalog.input_sources().to_vec(),
            resources: catalog
                .resources()
                .map(|entry| ResourceObservation {
                    descriptor: entry.descriptor().clone(),
                    source: entry.source().clone(),
                    original_url: entry.original_url().map(str::to_owned),
                    document_digest: match entry.handle() {
                        ResourceHandle::Local(entry) => Some(entry.document_digest.clone()),
                        ResourceHandle::External(_) => None,
                    },
                })
                .collect(),
        }
    }
}

pub(super) fn schema(tx: &rusqlite::Transaction<'_>) -> Result<(), Error> {
    tx.execute_batch("CREATE TABLE build_runs(
        id INTEGER PRIMARY KEY, pipeline TEXT NOT NULL, status TEXT NOT NULL,
        started INTEGER NOT NULL, duration_ms TEXT NOT NULL, stage TEXT,
        legacy_key TEXT UNIQUE, payload TEXT NOT NULL, digest TEXT NOT NULL);
        CREATE INDEX build_runs_pipeline ON build_runs(pipeline,id);
        CREATE TABLE build_inputs(run_id INTEGER NOT NULL REFERENCES build_runs(id), ordinal INTEGER NOT NULL,
            path TEXT NOT NULL, sha256 TEXT NOT NULL, content_id TEXT, PRIMARY KEY(run_id,ordinal));
        CREATE TABLE build_outputs(run_id INTEGER NOT NULL REFERENCES build_runs(id), ordinal INTEGER NOT NULL,
            path TEXT NOT NULL, sha256 TEXT NOT NULL, PRIMARY KEY(run_id,ordinal));
        CREATE TABLE build_diagnostics(run_id INTEGER NOT NULL REFERENCES build_runs(id), ordinal INTEGER NOT NULL,
            node TEXT NOT NULL, content_id TEXT NOT NULL, code TEXT NOT NULL, message TEXT NOT NULL,
            PRIMARY KEY(run_id,ordinal));
        CREATE TABLE build_catalogs(run_id INTEGER NOT NULL REFERENCES build_runs(id), node TEXT NOT NULL,
            input_digest TEXT NOT NULL, original_urls_digest TEXT NOT NULL, PRIMARY KEY(run_id,node));
        CREATE TABLE catalog_resources(run_id INTEGER NOT NULL, node TEXT NOT NULL, resource_id TEXT NOT NULL,
            payload TEXT NOT NULL, PRIMARY KEY(run_id,node,resource_id),
            FOREIGN KEY(run_id,node) REFERENCES build_catalogs(run_id,node));
        CREATE INDEX catalog_resources_identity ON catalog_resources(resource_id,run_id);")?;
    Ok(())
}

pub(super) fn payload(receipt: &BuildReceipt) -> Result<String, Error> {
    if receipt.schema_version != 1 || !matches!(receipt.status.as_str(), "succeeded" | "failed") {
        bail!("unsupported build receipt schema/status");
    }
    if (receipt.status == "failed") != receipt.error.is_some()
        || (receipt.status == "failed" && !receipt.outputs.is_empty())
    {
        bail!("inconsistent build receipt outcome");
    }
    if receipt
        .stage
        .as_deref()
        .is_some_and(|s| !matches!(s, "execution" | "setup"))
    {
        bail!("unsupported build receipt stage");
    }
    for hash in receipt
        .inputs
        .iter()
        .map(|r| &r.sha256)
        .chain(receipt.outputs.iter().map(|r| &r.sha256))
    {
        release::validate_id(hash)?;
    }
    let encoded = serde_json::to_string(receipt)?;
    if encoded.len() > BUILD_LIMIT {
        bail!("build receipt exceeds its size limit");
    }
    Ok(encoded)
}

pub(super) fn import_legacy(root: &Path) -> Result<Vec<(String, BuildReceipt)>, Error> {
    let directory = root.join(".berlin/receipts");
    if !directory.try_exists()? {
        return Ok(vec![]);
    }
    release::ensure_directory(&directory)?;
    let mut files = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()?;
    files.sort();
    let mut receipts = vec![];
    for path in files {
        // NamedTempFile can remain after a process interruption; it never became authoritative.
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .context("invalid receipt filename")?;
        if name.starts_with('.') {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            bail!("unexpected file in build receipts");
        }
        release::ensure_file(&path)?;
        if fs::metadata(&path)?.len() > BUILD_LIMIT as u64 {
            bail!("legacy build receipt exceeds its size limit");
        }
        let receipt: BuildReceipt = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("invalid build receipt {}", path.display()))?;
        payload(&receipt)?;
        receipts.push((name.to_owned(), receipt));
    }
    Ok(receipts)
}

pub(super) fn insert(
    tx: &rusqlite::Transaction<'_>,
    receipt: &BuildReceipt,
    legacy: Option<&str>,
) -> Result<i64, Error> {
    insert_at(tx, receipt, legacy, None)
}

fn insert_at(
    tx: &rusqlite::Transaction<'_>,
    receipt: &BuildReceipt,
    legacy: Option<&str>,
    id: Option<i64>,
) -> Result<i64, Error> {
    let encoded = payload(receipt)?;
    tx.execute("INSERT INTO build_runs(id,pipeline,status,started,duration_ms,stage,legacy_key,payload,digest)
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![id, receipt.pipeline, receipt.status,
        i64::try_from(receipt.started_at_unix_seconds)?, receipt.duration_ms.to_string(), receipt.stage, legacy,
        encoded, release::digest(encoded.as_bytes())])?;
    let run = tx.last_insert_rowid();
    for (i, r) in receipt.inputs.iter().enumerate() {
        tx.execute(
            "INSERT INTO build_inputs VALUES (?1,?2,?3,?4,?5)",
            params![run, i as i64, r.path, r.sha256, r.content_id],
        )?;
    }
    for (i, r) in receipt.outputs.iter().enumerate() {
        tx.execute(
            "INSERT INTO build_outputs VALUES (?1,?2,?3,?4)",
            params![run, i as i64, r.path, r.sha256],
        )?;
    }
    for (i, r) in receipt.diagnostics.iter().enumerate() {
        tx.execute(
            "INSERT INTO build_diagnostics VALUES (?1,?2,?3,?4,?5,?6)",
            params![run, i as i64, r.node, r.content_id, r.code, r.message],
        )?;
    }
    for catalog in &receipt.catalogs {
        tx.execute(
            "INSERT INTO build_catalogs VALUES (?1,?2,?3,?4)",
            params![
                run,
                catalog.node,
                catalog.input_digest,
                catalog.original_urls_digest
            ],
        )?;
        for resource in &catalog.resources {
            tx.execute(
                "INSERT INTO catalog_resources VALUES (?1,?2,?3,?4)",
                params![
                    run,
                    catalog.node,
                    resource.descriptor.id.to_string(),
                    serde_json::to_string(resource)?
                ],
            )?;
        }
    }
    Ok(run)
}

impl Store {
    pub(crate) fn record_build(&mut self, receipt: &BuildReceipt) -> Result<(), Error> {
        let tx = self.connection.transaction()?;
        insert(&tx, receipt, None)?;
        tx.commit()?;
        Ok(())
    }
    fn read_build(&self, id: i64) -> Result<BuildReceipt, Error> {
        let (encoded, digest): (String, String) = self.connection.query_row(
            "SELECT payload,digest FROM build_runs WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if encoded.len() > BUILD_LIMIT || release::digest(encoded.as_bytes()) != digest {
            bail!("build receipt hash/size mismatch");
        }
        let receipt: BuildReceipt = serde_json::from_str(&encoded)?;
        payload(&receipt)?;
        Ok(receipt)
    }
    pub(crate) fn build(&self, id: i64) -> Result<serde_json::Value, Error> {
        let receipt = self.read_build(id)?;
        Ok(serde_json::json!({"id":id,"receipt":receipt}))
    }
    pub(crate) fn builds(
        &self,
        pipeline: Option<&str>,
        limit: usize,
    ) -> Result<serde_json::Value, Error> {
        if !(1..=1000).contains(&limit) {
            bail!("build history limit must be between 1 and 1000");
        }
        let mut query = self.connection.prepare(
            "SELECT id FROM build_runs WHERE (?1 IS NULL OR pipeline=?1) ORDER BY id DESC LIMIT ?2",
        )?;
        let ids = query
            .query_map(params![pipeline, limit as i64], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut runs = vec![];
        for id in ids {
            let r = self.read_build(id)?;
            runs.push(
                serde_json::json!({"id":id,"pipeline":r.pipeline,"status":r.status,"stage":r.stage,
                "started_at_unix_seconds":r.started_at_unix_seconds,"duration_ms":r.duration_ms,
                "inputs":r.inputs.len(),"outputs":r.outputs.len(),"diagnostics":r.diagnostics.len(),
                "catalogs":r.catalogs.len(),"error":r.error}),
            );
        }
        Ok(serde_json::json!({"builds":runs}))
    }
    pub(super) fn validate_builds(&self) -> Result<(), Error> {
        // Reconstruct one run at a time so validation memory is bounded by a
        // receipt, rather than the lifetime size of the project's build history.
        let mut expected = Connection::open_in_memory()?;
        {
            let tx = expected.transaction()?;
            schema(&tx)?;
            tx.commit()?;
        }
        let mut query = self
            .connection
            .prepare("SELECT id,legacy_key FROM build_runs ORDER BY id")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let legacy: Option<String> = row.get(1)?;
            let r = self.read_build(id)?;
            let tx = expected.transaction()?;
            insert_at(&tx, &r, legacy.as_deref(), Some(id))?;
            tx.commit()?;
            for table in [
                "build_runs",
                "build_inputs",
                "build_outputs",
                "build_diagnostics",
                "build_catalogs",
                "catalog_resources",
            ] {
                let key = if table == "build_runs" {
                    "id"
                } else {
                    "run_id"
                };
                let sql = format!("SELECT * FROM {table} WHERE {key}=?1 ORDER BY 1,2,3");
                let mut actual = self.connection.prepare(&sql)?;
                let mut wanted = expected.prepare(&sql)?;
                let columns = actual.column_count();
                let mut a = actual.query([id])?;
                let mut b = wanted.query([id])?;
                loop {
                    match (a.next()?, b.next()?) {
                        (None, None) => break,
                        (Some(a), Some(b)) => {
                            for i in 0..columns {
                                if a.get_ref(i)? != b.get_ref(i)? {
                                    bail!("build history projection mismatch in {table}");
                                }
                            }
                        }
                        _ => bail!("build history projection mismatch in {table}"),
                    }
                }
            }
            expected.execute_batch("DELETE FROM catalog_resources; DELETE FROM build_catalogs;
                DELETE FROM build_diagnostics; DELETE FROM build_outputs; DELETE FROM build_inputs; DELETE FROM build_runs;")?;
        }
        Ok(())
    }
}
