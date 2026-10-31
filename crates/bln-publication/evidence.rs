//! Deterministic query evidence, separate from the catalog's complete input set.
use crate::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{self, Write},
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedRevision {
    pub id: ResourceId,
    pub revision: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryEvidence {
    pub query: ResourceQuery,
    pub query_digest: String,
    pub selection_digest: String,
    pub selected: Vec<SelectedRevision>,
}
pub type QueryEvidenceTable = BTreeMap<String, QueryEvidence>;

impl ResourceSelection {
    pub fn evidence(&self) -> QueryEvidence {
        QueryEvidence {
            query: self.query().clone(),
            query_digest: self.query_digest().into(),
            selection_digest: self.selection_digest().into(),
            selected: self
                .iter()
                .map(|entry| SelectedRevision {
                    id: entry.descriptor().id.clone(),
                    revision: entry.descriptor().revision.clone(),
                })
                .collect(),
        }
    }
}

/// Counts serialized bytes without allocating an encoded copy. This also bounds
/// zero-result queries, which otherwise consume no selected-item budget.
pub fn query_evidence_bytes(
    table: &QueryEvidenceTable,
    limits: &PublicationLimits,
) -> Result<usize> {
    if table.is_empty() {
        return Ok(0);
    }
    struct Counter {
        bytes: usize,
        limit: usize,
    }
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes) {
                return Err(io::Error::other("query evidence byte limit exceeded"));
            }
            self.bytes += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    if table.len() > limits.occurrences
        || table
            .values()
            .try_fold(0usize, |sum, evidence| {
                sum.checked_add(evidence.selected.len())
            })
            .is_none_or(|n| n > limits.occurrences)
    {
        return Err(evidence_error(
            Code::ResourceLimit,
            "query evidence count limit exceeded",
        ));
    }
    let mut counter = Counter {
        bytes: 0,
        limit: limits.property_bytes,
    };
    serde_json::to_writer(&mut counter, table)
        .map_err(|_| evidence_error(Code::ResourceLimit, "query evidence byte limit exceeded"))?;
    Ok(counter.bytes)
}

pub fn record_selection(
    table: &mut QueryEvidenceTable,
    selection: &ResourceSelection,
    limits: &PublicationLimits,
) -> Result<()> {
    if let Some(existing) = table.get(selection.query_digest()) {
        if existing.selection_digest != selection.selection_digest() {
            return Err(evidence_error(
                Code::InvalidQuery,
                "one normalized query resolved against different catalog snapshots",
            ));
        }
        return Ok(());
    }
    // The selection evaluator already bounds one result; table insertion remains
    // provisional until cumulative evidence limits pass.
    let key = selection.query_digest().to_owned();
    table.insert(key.clone(), selection.evidence());
    if let Err(error) = query_evidence_bytes(table, limits) {
        table.remove(&key);
        return Err(error);
    }
    Ok(())
}

pub fn validate_query_evidence(
    table: &QueryEvidenceTable,
    catalog: &PublicationCatalog,
) -> Result<()> {
    query_evidence_bytes(table, catalog.limits())?;
    if table
        .len()
        .checked_mul(catalog.len().max(1))
        .is_none_or(|n| n > catalog.options().resource_limits.query_work)
    {
        return Err(evidence_error(
            Code::ResourceLimit,
            "query evidence validation work exceeded",
        ));
    }
    for (key, evidence) in table {
        let selected = catalog.select(&evidence.query)?;
        if key != selected.query_digest() || evidence != &selected.evidence() {
            return Err(evidence_error(
                Code::InvalidQuery,
                "query evidence does not match the consumed catalog snapshot",
            ));
        }
    }
    Ok(())
}
fn evidence_error(code: Code, detail: &str) -> Diagnostic {
    Diagnostic::new(code, Stage::Resolution, detail)
}
