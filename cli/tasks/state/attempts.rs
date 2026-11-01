//! Captured pipeline executions preparing a release, separate from deployment.
use super::*;
use crate::tasks::receipt::{self, BuildReceipt};
use serde::Deserialize;

const ATTEMPT_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    Setup,
    Execution,
    Sealing,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Attempt {
    pub schema_version: u8,
    pub phase: Phase,
    pub receipt: BuildReceipt,
    pub release: Option<String>,
}

pub(super) fn schema(tx: &rusqlite::Transaction<'_>) -> Result<(), Error> {
    tx.execute_batch("CREATE TABLE release_attempts(
        id INTEGER PRIMARY KEY, pipeline TEXT NOT NULL, status TEXT NOT NULL,
        release_id TEXT REFERENCES sealed_releases(id), payload TEXT NOT NULL, digest TEXT NOT NULL);
        CREATE INDEX release_attempts_pipeline ON release_attempts(pipeline,id);
        CREATE INDEX release_attempts_release ON release_attempts(release_id,id);")?;
    Ok(())
}

impl Store {
    fn attempt_payload(&self, attempt: &Attempt) -> Result<String, Error> {
        if attempt.schema_version != 1 {
            bail!("unsupported release attempt version");
        }
        builds::payload(&attempt.receipt)?;
        let expected_stage = if attempt.phase == Phase::Setup {
            "setup"
        } else {
            "execution"
        };
        if attempt.receipt.stage.as_deref() != Some(expected_stage) {
            bail!("release attempt stage mismatch");
        }
        match (&attempt.release, attempt.receipt.status.as_str()) {
            (Some(id), "succeeded") if attempt.phase == Phase::Sealing => {
                let manifest = self.manifest(id)?;
                let expected = receipt::sealed_outputs(id, &manifest);
                if attempt.receipt.pipeline != manifest.pipeline
                    || attempt.receipt.berlin_version != manifest.berlin_version
                    || attempt.receipt.outputs != expected
                {
                    bail!("release attempt differs from its sealed manifest");
                }
            }
            (None, "failed") => {}
            _ => bail!("release attempt outcome mismatch"),
        }
        let payload = serde_json::to_string(attempt)?;
        if payload.len() > ATTEMPT_LIMIT {
            bail!("release attempt exceeds its size limit");
        }
        Ok(payload)
    }

    pub(crate) fn record_release_attempt(&self, attempt: &Attempt) -> Result<(), Error> {
        let tx = self.connection.unchecked_transaction()?;
        let payload = self.attempt_payload(attempt)?;
        tx.execute("INSERT INTO release_attempts(pipeline,status,release_id,payload,digest) VALUES (?1,?2,?3,?4,?5)",
            params![attempt.receipt.pipeline,attempt.receipt.status,attempt.release,payload,release::digest(payload.as_bytes())])?;
        tx.commit()?;
        Ok(())
    }
    fn read_attempt(&self, id: i64) -> Result<Attempt, Error> {
        let (pipeline, status, release, payload, digest): (
            String,
            String,
            Option<String>,
            String,
            String,
        ) = self.connection.query_row(
            "SELECT pipeline,status,release_id,payload,digest FROM release_attempts WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
        if payload.len() > ATTEMPT_LIMIT || release::digest(payload.as_bytes()) != digest {
            bail!("release attempt hash/size mismatch");
        }
        let attempt: Attempt = serde_json::from_str(&payload)?;
        self.attempt_payload(&attempt)?;
        if pipeline != attempt.receipt.pipeline
            || status != attempt.receipt.status
            || release != attempt.release
        {
            bail!("release attempt projection mismatch");
        }
        Ok(attempt)
    }
    pub(crate) fn release_attempt(&self, id: i64) -> Result<serde_json::Value, Error> {
        let tx = self.connection.unchecked_transaction()?;
        let attempt = self.read_attempt(id)?;
        tx.commit()?;
        Ok(
            serde_json::json!({"id":id,"attempt":attempt,"bundles_reverified":false,"remote_verified":false}),
        )
    }
    pub(crate) fn release_attempts(
        &self,
        pipeline: Option<&str>,
        limit: usize,
    ) -> Result<serde_json::Value, Error> {
        if !(1..=1000).contains(&limit) {
            bail!("release history limit must be between 1 and 1000");
        }
        let tx = self.connection.unchecked_transaction()?;
        let total: i64 = self.connection.query_row(
            "SELECT count(*) FROM release_attempts WHERE (?1 IS NULL OR pipeline=?1)",
            [pipeline],
            |r| r.get(0),
        )?;
        let mut query=self.connection.prepare("SELECT id FROM release_attempts WHERE (?1 IS NULL OR pipeline=?1) ORDER BY id DESC LIMIT ?2")?;
        let ids = query
            .query_map(params![pipeline, limit as i64], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut attempts = vec![];
        for id in ids {
            let attempt = self.read_attempt(id)?;
            attempts.push(serde_json::json!({"id":id,"pipeline":attempt.receipt.pipeline,"status":attempt.receipt.status,
                "phase":attempt.phase,"release":attempt.release,"started_at_unix_seconds":attempt.receipt.started_at_unix_seconds,
                "duration_ms":attempt.receipt.duration_ms,"inputs":attempt.receipt.inputs.len(),"outputs":attempt.receipt.outputs.len(),
                "diagnostics":attempt.receipt.diagnostics.len(),"catalogs":attempt.receipt.catalogs.len(),"error":attempt.receipt.error}));
        }
        tx.commit()?;
        Ok(
            serde_json::json!({"attempts":attempts,"total":total,"limit":limit,"order":"recorded_sequence_desc"}),
        )
    }
    pub(super) fn release_lineage(
        &self,
        release: &str,
        limit: usize,
    ) -> Result<serde_json::Value, Error> {
        let total: i64 = self.connection.query_row(
            "SELECT count(*) FROM release_attempts WHERE release_id=?1",
            [release],
            |r| r.get(0),
        )?;
        let mut query = self.connection.prepare(
            "SELECT id FROM release_attempts WHERE release_id=?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let ids = query
            .query_map(params![release, limit as i64], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for id in &ids {
            self.read_attempt(*id)?;
        }
        Ok(
            serde_json::json!({"status":if total==0 {"not_recorded"} else {"captured_build"},"attempts":ids,"total":total,"limit":limit}),
        )
    }
    pub(super) fn validate_attempts(&self) -> Result<(), Error> {
        let mut query = self
            .connection
            .prepare("SELECT id FROM release_attempts ORDER BY id")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            self.read_attempt(row.get(0)?)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn failed() -> Attempt {
        Attempt {schema_version:1,phase:Phase::Setup,release:None,receipt:serde_json::from_value(serde_json::json!({
            "schema_version":1,"berlin_version":"test","pipeline":"site","status":"failed","started_at_unix_seconds":1,
            "duration_ms":3,"inputs":[],"outputs":[],"diagnostics":[],"error":"synthetic setup refusal","stage":"setup"
        })).unwrap()}
    }
    #[test]
    fn attempts_append_validate_outcomes_and_detect_projection_changes() {
        let root = tempfile::tempdir().unwrap();
        let p = Project::new(root.path().into(), vec![]);
        migrate(&p).unwrap();
        let db = Store::open(root.path()).unwrap();
        let attempt = failed();
        db.record_release_attempt(&attempt).unwrap();
        db.record_release_attempt(&attempt).unwrap();
        assert_eq!(db.release_attempts(None, 1).unwrap()["total"], 2);
        let mut invalid = attempt.clone();
        invalid.receipt.status = "succeeded".into();
        invalid.receipt.error = None;
        assert!(db.record_release_attempt(&invalid).is_err());
        invalid = attempt.clone();
        invalid.receipt.outputs.push(receipt::FileReceipt {
            path: "invalid-output".into(),
            sha256: "a".repeat(64),
        });
        assert!(db.record_release_attempt(&invalid).is_err());
        invalid = attempt;
        invalid.phase = Phase::Execution;
        assert!(db.record_release_attempt(&invalid).is_err());
        assert_eq!(db.release_attempts(None, 20).unwrap()["total"], 2);
        db.validate().unwrap();
        db.connection
            .execute(
                "UPDATE release_attempts SET status='succeeded' WHERE id=1",
                [],
            )
            .unwrap();
        assert!(db.release_attempt(1).is_err());
        assert!(db.validate().is_err());
    }
    #[test]
    fn schema_four_upgrade_is_atomic_and_preserves_old_backup_semantics() {
        let root = tempfile::tempdir().unwrap();
        let p = Project::new(root.path().into(), vec![]);
        migrate(&p).unwrap();
        let db = Store::open(root.path()).unwrap();
        db.connection
            .execute_batch("DROP TABLE release_attempts; PRAGMA user_version=4;")
            .unwrap();
        let file = root.path().join("schema-four.sqlite");
        backup(&p, &file).unwrap();
        db.connection
            .execute_batch("CREATE TABLE release_attempts(block_upgrade TEXT);")
            .unwrap();
        assert!(migrate(&p).is_err());
        assert_eq!(db.version().unwrap(), 4);
        db.connection
            .execute_batch("DROP TABLE release_attempts;")
            .unwrap();
        let bad = root.path().join(".berlin/releases").join("a".repeat(64));
        fs::create_dir_all(&bad).unwrap();
        fs::write(bad.join("manifest.json"), "not a sealed manifest").unwrap();
        migrate(&p).unwrap();
        assert_eq!(db.version().unwrap(), VERSION);
        assert_eq!(db.release_attempts(None, 20).unwrap()["total"], 0);
        let restored = tempfile::tempdir().unwrap();
        let restored_project = Project::new(restored.path().into(), vec![]);
        restore(&restored_project, &file).unwrap();
        assert_eq!(status(&restored_project).unwrap()["schema_version"], 4);
        migrate(&restored_project).unwrap();
        assert_eq!(
            Store::open(restored.path())
                .unwrap()
                .release_attempts(None, 20)
                .unwrap()["total"],
            0
        );
    }
}
