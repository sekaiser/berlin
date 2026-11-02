//! Immutable article captures and append-only reviewer/author dialogue.
use super::*;
use crate::tasks::review::{self, Event, Session, View};

const SESSION_LIMIT: usize = 8 * 1024 * 1024;
pub(super) fn schema(tx: &rusqlite::Transaction<'_>) -> Result<(), Error> {
    tx.execute_batch(
        "CREATE TABLE article_reviews(
        id INTEGER PRIMARY KEY, document_id TEXT NOT NULL, source_sha256 TEXT NOT NULL,
        payload TEXT NOT NULL, digest TEXT NOT NULL);
        CREATE INDEX article_reviews_document ON article_reviews(document_id,id);
        CREATE TABLE article_review_events(
        review_id INTEGER NOT NULL REFERENCES article_reviews(id), ordinal INTEGER NOT NULL,
        payload TEXT NOT NULL, digest TEXT NOT NULL, PRIMARY KEY(review_id,ordinal));",
    )?;
    Ok(())
}
fn payload(value: &impl Serialize, limit: usize) -> Result<String, Error> {
    let value = serde_json::to_string(value)?;
    if value.len() > limit {
        bail!("article review record exceeds its size limit");
    }
    Ok(value)
}
fn checked<T: DeserializeOwned>(value: &str, digest: &str, limit: usize) -> Result<T, Error> {
    if value.len() > limit || release::digest(value.as_bytes()) != digest {
        bail!("article review record hash/size mismatch");
    }
    Ok(serde_json::from_str(value)?)
}
impl Store {
    pub(crate) fn start_review(&self, session: Session) -> Result<i64, Error> {
        session.validate()?;
        session.snapshot.require_current()?;
        let payload = payload(&session, SESSION_LIMIT)?;
        self.connection.execute("INSERT INTO article_reviews(document_id,source_sha256,payload,digest) VALUES (?1,?2,?3,?4)",
            params![session.snapshot.document_id, session.snapshot.sha256, payload, release::digest(payload.as_bytes())])?;
        Ok(self.connection.last_insert_rowid())
    }
    fn read_review(&self, id: i64) -> Result<View, Error> {
        // Refuse oversized rows before transferring payloads into Rust.
        let size: i64 = self
            .connection
            .query_row(
                "SELECT length(CAST(payload AS BLOB)) FROM article_reviews WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .context("article review not found")?;
        if size > SESSION_LIMIT as i64 {
            bail!("article review snapshot exceeds its size limit");
        }
        let (document, sha, payload, digest): (String, String, String, String) =
            self.connection.query_row(
                "SELECT document_id,source_sha256,payload,digest FROM article_reviews WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        let session: Session = checked(&payload, &digest, SESSION_LIMIT)?;
        if document != session.snapshot.document_id || sha != session.snapshot.sha256 {
            bail!("article review identity projection mismatch");
        }
        let mut view = View::new(id, session)?;
        let (count, max_size): (i64, i64) = self.connection.query_row(
            "SELECT count(*),coalesce(max(length(CAST(payload AS BLOB))),0) FROM article_review_events WHERE review_id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?)))?;
        if count > review::EVENT_LIMIT as i64 || max_size > review::RECORD_LIMIT as i64 {
            bail!("article review event size/count limit exceeded");
        }
        let mut query = self.connection.prepare("SELECT ordinal,payload,digest FROM article_review_events WHERE review_id=?1 ORDER BY ordinal")?;
        let mut rows = query.query([id])?;
        while let Some(row) = rows.next()? {
            let ordinal: i64 = row.get(0)?;
            if ordinal != view.event_count as i64 {
                bail!("article review event sequence mismatch");
            }
            let payload: String = row.get(1)?;
            let digest: String = row.get(2)?;
            view.apply(checked(&payload, &digest, review::RECORD_LIMIT)?)?;
        }
        Ok(view)
    }
    pub(crate) fn review(&self, id: i64) -> Result<View, Error> {
        let tx = self.connection.unchecked_transaction()?;
        let view = self.read_review(id)?;
        tx.commit()?;
        Ok(view)
    }
    pub(crate) fn append_review(&self, id: i64, event: Event) -> Result<View, Error> {
        let tx = self.connection.unchecked_transaction()?;
        let mut view = self.read_review(id)?;
        view.session.snapshot.require_current()?;
        let ordinal = view.event_count as i64;
        let payload = payload(&event, review::RECORD_LIMIT)?;
        view.apply(event)?;
        tx.execute(
            "INSERT INTO article_review_events VALUES (?1,?2,?3,?4)",
            params![id, ordinal, payload, release::digest(payload.as_bytes())],
        )?;
        tx.commit()?;
        Ok(view)
    }
    pub(crate) fn reviews(
        &self,
        document: Option<&str>,
        limit: usize,
    ) -> Result<serde_json::Value, Error> {
        if !(1..=1000).contains(&limit) {
            bail!("review history limit must be between 1 and 1000");
        }
        let tx = self.connection.unchecked_transaction()?;
        let total: i64 = self.connection.query_row(
            "SELECT count(*) FROM article_reviews WHERE (?1 IS NULL OR document_id=?1)",
            [document],
            |r| r.get(0),
        )?;
        let mut query = self.connection.prepare("SELECT id FROM article_reviews WHERE (?1 IS NULL OR document_id=?1) ORDER BY id DESC LIMIT ?2")?;
        let ids = query
            .query_map(params![document, limit as i64], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut reviews = vec![];
        for id in ids {
            let view = self.read_review(id)?;
            reviews.push(serde_json::json!({"id":id,"document_id":view.session.snapshot.document_id,"title":view.session.snapshot.title,"source_sha256":view.session.snapshot.sha256,
                "step":view.current_step(),"status":if view.current_step().is_none(){"completed"}else{"in_progress"},"outstanding":view.outstanding()}));
        }
        tx.commit()?;
        Ok(
            serde_json::json!({"reviews":reviews,"total":total,"limit":limit,"revision_status":"not_checked; use review show"}),
        )
    }
    pub(super) fn validate_reviews(&self) -> Result<(), Error> {
        let mut query = self
            .connection
            .prepare("SELECT id FROM article_reviews ORDER BY id")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            self.read_review(row.get(0)?)?;
        }
        Ok(())
    }
}
