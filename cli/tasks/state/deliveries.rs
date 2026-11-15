//! Durable remote-attempt boundaries. A crash after start always requires reconciliation.
use super::*;
use crate::tasks::announcement::delivery::Event;

pub(super) fn schema(tx: &rusqlite::Transaction<'_>) -> Result<(), Error> {
    tx.execute_batch("CREATE TABLE delivery_events(sequence INTEGER PRIMARY KEY, review_id TEXT NOT NULL REFERENCES approvals(id), payload TEXT NOT NULL, digest TEXT NOT NULL); CREATE INDEX delivery_review ON delivery_events(review_id,sequence);")?;
    Ok(())
}
impl Store {
    pub(crate) fn delivery_events(&self, review: Option<&str>) -> Result<Vec<Event>, Error> {
        let mut query=self.connection.prepare("SELECT review_id,payload,digest FROM delivery_events WHERE (?1 IS NULL OR review_id=?1) ORDER BY sequence")?;
        let mut rows = query.query([review])?;
        let mut out = vec![];
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let payload: String = row.get(1)?;
            let digest: String = row.get(2)?;
            verify(&payload, &digest)?;
            let event: Event = serde_json::from_str(&payload)?;
            event.validate()?;
            if event.review != id {
                bail!("delivery event identity mismatch");
            }
            out.push(event);
        }
        Ok(out)
    }
    pub(crate) fn begin_delivery(&mut self, event: &Event) -> Result<Option<Event>, Error> {
        event.validate()?;
        if event.state != "started" {
            bail!("delivery must start with an intent");
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (payload, digest): (String, String) = tx.query_row(
            "SELECT payload,digest FROM approvals WHERE id=?1",
            [&event.review],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        verify_bounded(&payload, &digest, announcement::MAX_APPROVAL_BYTES)?;
        let approval: announcement::Approval = serde_json::from_str(&payload)?;
        event.matches_snapshot(&approval.snapshot)?;
        let previous:Option<(String,String)>=tx.query_row("SELECT payload,digest FROM delivery_events WHERE review_id=?1 ORDER BY sequence DESC LIMIT 1",[&event.review],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((payload, digest)) = previous {
            verify(&payload, &digest)?;
            let old: Event = serde_json::from_str(&payload)?;
            old.validate()?;
            if old.settled() {
                return Ok(Some(old));
            }
            if old.state != "rejected" {
                bail!("previous delivery is uncertain; reconcile it before retrying");
            }
        }
        let payload = encode(event)?;
        tx.execute(
            "INSERT INTO delivery_events(review_id,payload,digest) VALUES (?1,?2,?3)",
            params![event.review, payload, release::digest(payload.as_bytes())],
        )?;
        tx.commit()?;
        Ok(None)
    }
    pub(crate) fn finish_delivery(&mut self, event: &Event) -> Result<(), Error> {
        event.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (payload,digest):(String,String)=tx.query_row("SELECT payload,digest FROM delivery_events WHERE review_id=?1 ORDER BY sequence DESC LIMIT 1",[&event.review],|r|Ok((r.get(0)?,r.get(1)?))).context("no delivery intent to resolve")?;
        verify(&payload, &digest)?;
        let previous: Event = serde_json::from_str(&payload)?;
        previous.validate()?;
        if !matches!(
            previous.state.as_str(),
            "started" | "submitting" | "unknown"
        ) {
            bail!("delivery already settled");
        }
        if !event.follows(Some(&previous)) {
            bail!("invalid delivery transition");
        }
        let (approval, digest): (String, String) = tx.query_row(
            "SELECT payload,digest FROM approvals WHERE id=?1",
            [&event.review],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        verify_bounded(&approval, &digest, announcement::MAX_APPROVAL_BYTES)?;
        let approval: announcement::Approval = serde_json::from_str(&approval)?;
        event.matches_snapshot(&approval.snapshot)?;
        if let Some(publication) = event.publication() {
            {
                let mut statement = tx.prepare("SELECT r.payload,r.digest,a.payload,a.digest FROM reports r JOIN approvals a ON a.id=r.id WHERE r.id<>?1")?;
                let rows = statement.query_map([&event.review], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })?;
                for row in rows {
                    let (report, digest, context, context_digest) = row?;
                    verify(&report, &digest)?;
                    verify_bounded(&context, &context_digest, announcement::MAX_APPROVAL_BYTES)?;
                    let old: announcement::Publication = serde_json::from_str(&report)?;
                    let old_approval: announcement::Approval = serde_json::from_str(&context)?;
                    if old.post_url == publication.post_url
                        || old_approval.snapshot.same_revision(&approval.snapshot)
                    {
                        bail!(
                            "this post URL or article revision is already recorded under another review"
                        );
                    }
                }
            }
            let prior: Option<(String, String)> = tx
                .query_row(
                    "SELECT payload,digest FROM reports WHERE id=?1",
                    [&event.review],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if let Some((old, digest)) = prior {
                verify(&old, &digest)?;
                let old: announcement::Publication = serde_json::from_str(&old)?;
                if old.post_url != publication.post_url {
                    bail!("publication URL already recorded differently");
                }
            } else {
                let payload = encode(&publication)?;
                tx.execute(
                    "INSERT INTO reports VALUES (?1,?2,?3)",
                    params![event.review, payload, release::digest(payload.as_bytes())],
                )?;
            }
        }
        let payload = encode(event)?;
        tx.execute(
            "INSERT INTO delivery_events(review_id,payload,digest) VALUES (?1,?2,?3)",
            params![event.review, payload, release::digest(payload.as_bytes())],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub(super) fn validate_deliveries(&self) -> Result<(), Error> {
        let mut last = BTreeMap::<String, Event>::new();
        let approvals = self.records::<announcement::Approval>("approvals")?;
        for event in self.delivery_events(None)? {
            if !event.follows(last.get(&event.review)) {
                bail!("invalid delivery event sequence");
            }
            event.matches_snapshot(
                &approvals
                    .get(&event.review)
                    .context("missing delivery approval")?
                    .snapshot,
            )?;
            last.insert(event.review.clone(), event);
        }
        for (id, report) in self.records::<announcement::Publication>("reports")? {
            if report.requires_api_receipt()
                && last.get(&id).is_none_or(|e| !report.api_receipt_matches(e))
            {
                bail!("API publication record has no matching delivery receipt");
            }
        }
        Ok(())
    }
}
