//! Collector-facing persistence contract. No collectors, imports or network I/O.
use super::*;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Definition {
    pub name: String,
    pub version: u32,
    pub description: String,
    pub unit: Unit,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Unit {
    Count,
    Ratio,
    Milliseconds,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    LifetimeTotal,
    IntervalTotal,
    Gauge,
}

/// The reference establishes article/publication context, not revision attribution.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Target {
    Website { release: String, document: String },
    ReportedAnnouncement { review_id: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub(crate) enum Value {
    Count(u64),
    Measure(f64),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Window {
    pub start_unix_seconds: u64,
    pub end_unix_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CollectionMethod {
    Manual,
    Api,
    FileImport,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Source {
    /// Stable collector namespace, including its dataset/account scope.
    pub collector: String,
    /// Stable source record key, reused only for byte-equivalent replay.
    pub record_key: String,
    pub method: CollectionMethod,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation {
    pub schema_version: u8,
    pub definition: String,
    pub target: Target,
    pub observed_at_unix_seconds: u64,
    pub window: Option<Window>,
    pub value: Value,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Publication {
    document: String,
    channel: String,
    account: Option<String>,
    url: url::Url,
}

pub(super) fn schema(tx: &rusqlite::Transaction<'_>) -> Result<(), Error> {
    tx.execute_batch("CREATE TABLE metric_definitions(
        id TEXT PRIMARY KEY, name TEXT NOT NULL, version INTEGER NOT NULL, payload TEXT NOT NULL,
        UNIQUE(name,version));
        CREATE TABLE metric_publications(
        id TEXT PRIMARY KEY, document TEXT NOT NULL, payload TEXT NOT NULL);
        CREATE INDEX metric_publications_document ON metric_publications(document,id);
        CREATE TABLE metric_observations(
        sequence INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE,
        publication_id TEXT NOT NULL REFERENCES metric_publications(id),
        definition_id TEXT NOT NULL REFERENCES metric_definitions(id),
        collector TEXT NOT NULL, record_key TEXT NOT NULL, observed_at INTEGER NOT NULL,
        recorded_at INTEGER NOT NULL, payload TEXT NOT NULL,
        UNIQUE(publication_id,definition_id,collector,record_key));
        CREATE INDEX metric_observations_publication ON metric_observations(publication_id,sequence);")?;
    Ok(())
}

fn text(value: &str, limit: usize) -> Result<(), Error> {
    if value.trim().is_empty() || value.len() > limit || value.chars().any(char::is_control) {
        bail!("metric text is empty, too long or contains control characters");
    }
    Ok(())
}
fn definition_payload(definition: &Definition) -> Result<String, Error> {
    text(&definition.name, 128)?;
    text(&definition.description, 2048)?;
    if definition.version == 0
        || !definition.name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
    {
        bail!("metric name must be a lowercase identifier and version must be positive");
    }
    if definition.unit == Unit::Ratio && definition.kind != Kind::Gauge {
        bail!("ratio metrics must be gauges");
    }
    encode(definition)
}
fn validate_observation(observation: &Observation, definition: &Definition) -> Result<(), Error> {
    if observation.schema_version != 1 {
        bail!("unsupported metric observation version");
    }
    release::validate_id(&observation.definition)?;
    text(&observation.source.collector, 128)?;
    text(&observation.source.record_key, 256)?;
    i64::try_from(observation.observed_at_unix_seconds)?;
    match (&observation.value, &definition.unit) {
        (Value::Count(_), Unit::Count) => {}
        (Value::Measure(value), Unit::Ratio)
            if value.is_finite() && (0.0..=1.0).contains(value) => {}
        (Value::Measure(value), Unit::Milliseconds) if value.is_finite() && *value >= 0.0 => {}
        _ => bail!("metric value does not match its unit/range"),
    }
    match (&definition.kind, &observation.window) {
        (Kind::IntervalTotal, Some(window))
            if window.start_unix_seconds < window.end_unix_seconds
                && window.end_unix_seconds <= observation.observed_at_unix_seconds => {}
        (Kind::LifetimeTotal | Kind::Gauge, None) => {}
        _ => bail!(
            "interval metrics need a completed nonempty [start,end) window; other metrics have no window"
        ),
    }
    Ok(())
}

impl Store {
    /// Reserved storage boundary for future collectors; no write CLI is exposed.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn register_metric(&mut self, definition: &Definition) -> Result<String, Error> {
        let payload = definition_payload(definition)?;
        let id = release::digest(payload.as_bytes());
        let tx = self.connection.transaction()?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM metric_definitions WHERE name=?1 AND version=?2",
                params![definition.name, definition.version],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            if existing != id {
                bail!("metric definition changed; register a new version");
            }
            read_definition(&tx, &existing)?;
        } else {
            tx.execute(
                "INSERT INTO metric_definitions VALUES (?1,?2,?3,?4)",
                params![id, definition.name, definition.version, payload],
            )?;
        }
        tx.commit()?;
        Ok(id)
    }

    fn metric_publication(&self, target: &Target) -> Result<Publication, Error> {
        match target {
            Target::Website { release, document } => {
                let manifest = self.manifest(release)?;
                let entry = manifest
                    .documents
                    .as_ref()
                    .and_then(|d| d.entries.iter().find(|e| &e.id.0 == document))
                    .context("metric target is not an indexed article in this release")?;
                Ok(Publication {
                    document: document.clone(),
                    channel: "website".into(),
                    account: None,
                    url: release::announcements::document_url(&manifest.url, &entry.path)?,
                })
            }
            Target::ReportedAnnouncement { review_id } => {
                let history = announcement::store::validate(
                    self.records("approvals")?,
                    self.records("reports")?,
                )?;
                let entry = history
                    .iter()
                    .find(|e| &e.approval.review_id == review_id)
                    .context("unknown announcement metric target")?;
                let report = entry
                    .publication
                    .as_ref()
                    .context("announcement metric target has not been reported published")?;
                Ok(Publication {
                    document: entry.approval.snapshot.document.id.0.clone(),
                    channel: entry.approval.snapshot.channel().into(),
                    account: Some(entry.approval.snapshot.account.clone()),
                    url: report.post_url.clone(),
                })
            }
        }
    }

    /// Append a collector observation atomically. Identical replay is idempotent;
    /// a reused source key with changed data is an error, never an overwrite.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn record_metric(&self, observation: &Observation) -> Result<String, Error> {
        let tx = self.connection.unchecked_transaction()?;
        let definition = read_definition(&self.connection, &observation.definition)?;
        validate_observation(observation, &definition)?;
        let publication = self.metric_publication(&observation.target)?;
        let target_payload = encode(&publication)?;
        let target_id = release::digest(target_payload.as_bytes());
        let payload = encode(observation)?;
        let id = release::digest(payload.as_bytes());
        let existing: Option<String> = tx.query_row("SELECT id FROM metric_observations WHERE publication_id=?1 AND definition_id=?2 AND collector=?3 AND record_key=?4",
            params![target_id,observation.definition,observation.source.collector,observation.source.record_key],|r| r.get(0)).optional()?;
        if let Some(existing) = existing {
            if existing != id {
                bail!("metric source key was reused with different observation data");
            }
            self.read_metric(&existing)?;
        } else {
            tx.execute(
                "INSERT INTO metric_publications VALUES (?1,?2,?3) ON CONFLICT(id) DO NOTHING",
                params![target_id, publication.document, target_payload],
            )?;
            let stored: (String, String) = tx.query_row(
                "SELECT payload,document FROM metric_publications WHERE id=?1",
                [&target_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if stored.0 != target_payload || stored.1 != publication.document {
                bail!("metric publication projection mismatch");
            }
            let recorded = i64::try_from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
            )?;
            tx.execute("INSERT INTO metric_observations(id,publication_id,definition_id,collector,record_key,observed_at,recorded_at,payload)
                VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![id,target_id,observation.definition,observation.source.collector,observation.source.record_key,
                    observation.observed_at_unix_seconds as i64,recorded,payload])?;
        }
        tx.commit()?;
        Ok(id)
    }

    fn read_metric(&self, id: &str) -> Result<serde_json::Value, Error> {
        let (payload,target_id,definition_id,collector,key,observed,recorded): (String,String,String,String,String,i64,i64) = self.connection.query_row(
            "SELECT payload,publication_id,definition_id,collector,record_key,observed_at,recorded_at FROM metric_observations WHERE id=?1",[id],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?)))?;
        verify(&payload, id)?;
        let observation: Observation = serde_json::from_str(&payload)?;
        let definition = read_definition(&self.connection, &definition_id)?;
        validate_observation(&observation, &definition)?;
        let publication = self.metric_publication(&observation.target)?;
        let expected = encode(&publication)?;
        let (target_payload, document): (String, String) = self.connection.query_row(
            "SELECT payload,document FROM metric_publications WHERE id=?1",
            [&target_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        verify(&target_payload, &target_id)?;
        if target_payload != expected
            || document != publication.document
            || definition_id != observation.definition
            || collector != observation.source.collector
            || key != observation.source.record_key
            || observed < 0
            || observed as u64 != observation.observed_at_unix_seconds
            || recorded < 0
        {
            bail!("metric observation projection mismatch");
        }
        Ok(
            serde_json::json!({"id":id,"publication_id":target_id,"publication":publication,"definition":definition,
            "observation":observation,"recorded_at_unix_seconds":recorded}),
        )
    }

    pub(super) fn validate_metrics(&self) -> Result<(), Error> {
        let mut query = self
            .connection
            .prepare("SELECT id FROM metric_definitions ORDER BY id")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            read_definition(&self.connection, &row.get::<_, String>(0)?)?;
        }
        self.validate_metric_publications()?;
        let mut query = self
            .connection
            .prepare("SELECT id FROM metric_observations ORDER BY sequence")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            self.read_metric(&row.get::<_, String>(0)?)?;
        }
        Ok(())
    }

    fn validate_metric_publications(&self) -> Result<(), Error> {
        let mut query = self
            .connection
            .prepare("SELECT id,payload,document FROM metric_publications ORDER BY id")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let payload: String = row.get(1)?;
            verify(&payload, &id)?;
            let publication: Publication = serde_json::from_str(&payload)?;
            if publication.document != row.get::<_, String>(2)? {
                bail!("metric publication projection mismatch");
            }
        }
        Ok(())
    }

    /// Called inside the article inspection's read transaction. This reports
    /// observation metadata, never engagement totals or a full integrity verdict.
    pub(super) fn metric_coverage(&self, document: &str) -> Result<serde_json::Value, Error> {
        self.validate_metric_publications()?;
        let mut definitions = self.connection.prepare(
            "SELECT DISTINCT o.definition_id FROM metric_observations o
            JOIN metric_publications p ON p.id=o.publication_id WHERE p.document=?1",
        )?;
        let mut rows = definitions.query([document])?;
        while let Some(row) = rows.next()? {
            read_definition(&self.connection, &row.get::<_, String>(0)?)?;
        }
        let (observations,publications,definitions,last_observed,last_recorded): (i64,i64,i64,Option<i64>,Option<i64>) = self.connection.query_row(
            "SELECT count(*),count(DISTINCT o.publication_id),count(DISTINCT o.definition_id),max(o.observed_at),max(o.recorded_at)
            FROM metric_observations o JOIN metric_publications p ON p.id=o.publication_id WHERE p.document=?1",
            [document],|r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
        if last_observed.is_some_and(|v| v < 0) || last_recorded.is_some_and(|v| v < 0) {
            bail!("invalid metric observation timestamp");
        }
        Ok(
            serde_json::json!({"status":if observations == 0 {"no_observations"} else {"observations_recorded"},
            "observation_count":observations,"publication_count":publications,"definition_count":definitions,
            "last_observed_at_unix_seconds":last_observed,"last_recorded_at_unix_seconds":last_recorded,
            "collectors_enabled":false,"validation":"metadata_only"}),
        )
    }

    pub(crate) fn metric_definitions(&self, limit: usize) -> Result<serde_json::Value, Error> {
        limit_check(limit)?;
        let tx = self.connection.unchecked_transaction()?;
        let total: i64 =
            self.connection
                .query_row("SELECT count(*) FROM metric_definitions", [], |r| r.get(0))?;
        let mut query = self
            .connection
            .prepare("SELECT id FROM metric_definitions ORDER BY name,version LIMIT ?1")?;
        let ids = query
            .query_map([limit as i64], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let definitions = ids
            .into_iter()
            .map(|id| {
                Ok(serde_json::json!({"definition":read_definition(&self.connection,&id)?,"id":id}))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        tx.commit()?;
        Ok(
            serde_json::json!({"definitions":definitions,"total":total,"limit":limit,"collectors_enabled":false,"validation":"returned_definitions"}),
        )
    }
    pub(crate) fn metrics(
        &self,
        document: Option<&str>,
        limit: usize,
    ) -> Result<serde_json::Value, Error> {
        limit_check(limit)?;
        if let Some(document) = document {
            text(document, 1024)?;
        }
        let tx = self.connection.unchecked_transaction()?;
        self.validate_metric_publications()?;
        let total:i64=self.connection.query_row("SELECT count(*) FROM metric_observations o LEFT JOIN metric_publications p ON p.id=o.publication_id WHERE (?1 IS NULL OR p.document=?1)",[document],|r| r.get(0))?;
        let mut query=self.connection.prepare("SELECT o.id FROM metric_observations o LEFT JOIN metric_publications p ON p.id=o.publication_id
            WHERE (?1 IS NULL OR p.document=?1) ORDER BY o.sequence DESC LIMIT ?2")?;
        let ids = query
            .query_map(params![document, limit as i64], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let observations = ids
            .iter()
            .map(|id| self.read_metric(id))
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit()?;
        Ok(
            serde_json::json!({"observations":observations,"total":total,"limit":limit,"order":"recorded_sequence_desc",
            "remote_verified":false,"collectors_enabled":false,"validation":"returned_observations"}),
        )
    }
}

fn read_definition(connection: &Connection, id: &str) -> Result<Definition, Error> {
    let (payload, name, version): (String, String, u32) = connection.query_row(
        "SELECT payload,name,version FROM metric_definitions WHERE id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    verify(&payload, id)?;
    let definition: Definition = serde_json::from_str(&payload)?;
    definition_payload(&definition)?;
    if name != definition.name || version != definition.version {
        bail!("metric definition projection mismatch");
    }
    Ok(definition)
}
fn limit_check(limit: usize) -> Result<(), Error> {
    if !(1..=1000).contains(&limit) {
        bail!("metric query limit must be between 1 and 1000");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
