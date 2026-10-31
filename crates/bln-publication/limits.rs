use std::io::{self, Write};

use serde::{Deserialize, Serialize};

use crate::{Code, Diagnostic, Result, Stage};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PublicationLimits {
    pub occurrences: usize,
    pub depth: usize,
    pub query_sources: usize,
    pub string_bytes: usize,
    pub property_bytes: usize,
    pub slots: usize,
    pub output_bytes: usize,
    pub fragment_bytes: usize,
    pub excerpt_nodes: usize,
}

impl Default for PublicationLimits {
    fn default() -> Self {
        Self {
            occurrences: 10_000,
            depth: 32,
            query_sources: 1_000,
            string_bytes: 1_000_000,
            property_bytes: 8 * 1024 * 1024,
            slots: 16,
            output_bytes: 16 * 1024 * 1024,
            fragment_bytes: 64 * 1024 * 1024,
            excerpt_nodes: 10_000,
        }
    }
}

impl PublicationLimits {
    pub fn validate(&self) -> Result<()> {
        let maximum = Self::default();
        for (name, actual, max) in [
            ("occurrences", self.occurrences, maximum.occurrences),
            ("depth", self.depth, maximum.depth),
            ("query_sources", self.query_sources, maximum.query_sources),
            ("string_bytes", self.string_bytes, maximum.string_bytes),
            (
                "property_bytes",
                self.property_bytes,
                maximum.property_bytes,
            ),
            ("slots", self.slots, maximum.slots),
            ("output_bytes", self.output_bytes, maximum.output_bytes),
            (
                "fragment_bytes",
                self.fragment_bytes,
                maximum.fragment_bytes,
            ),
            ("excerpt_nodes", self.excerpt_nodes, maximum.excerpt_nodes),
        ] {
            if actual == 0 || actual > max {
                return Err(Diagnostic::new(
                    Code::ResourceLimit,
                    Stage::Construction,
                    format!("{name} must be between 1 and supported maximum {max}, got {actual}"),
                ));
            }
        }
        Ok(())
    }
}

/// Fallible sink. Rejects a write before growth rather than trimming afterward.
/// Counts payload bytes only; this does not bound allocator/process overhead.
pub struct BoundedWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl BoundedWriter {
    pub fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other(
                "ResourceLimit: rendered output byte budget exceeded",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
