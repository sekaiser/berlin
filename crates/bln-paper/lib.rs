//! Pure conversion of a supported Pandoc LaTeX AST and matching TeX labels.
//! Compilation, file access, trust decisions and bundle installation belong to the CLI.
mod aux;
mod bibliography;
mod pandoc;
mod profile;
pub mod tex;
pub use aux::Labels;
pub use bibliography::{BibliographyEntry, bibtex_entries};
pub use profile::{SourceDetails, inspect_source, validate_source};

use berlin_document::{ContentId, Document, Metadata, Provenance};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unsupported or invalid paper: {0}")]
    Unsupported(String),
    #[error("invalid Pandoc JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Document(#[from] berlin_document::DocumentValidationError),
    #[error(transparent)]
    Math(#[from] berlin_math::Error),
}

fn unsupported(message: impl Into<String>) -> Error {
    Error::Unsupported(message.into())
}

/// Publication identity is supplied by the author, not invented from a title or
/// a typesetting date. Title, authors and abstract come from the paper itself.
pub struct Publication {
    pub id: ContentId,
    pub metadata: Metadata,
    pub provenance: Provenance,
    pub pdf_url: String,
}

/// Requires an AST from the same source compilation as `labels`. The CLI creates
/// both in one fresh working directory; this pure function never executes TeX.
pub fn import(
    json: &str,
    labels: &Labels,
    bibliography: &[BibliographyEntry],
    publication: Publication,
) -> Result<Document, Error> {
    if json.len() > 16 * 1024 * 1024 {
        return Err(unsupported("Pandoc document exceeds 16 MiB"));
    }
    pandoc::import(
        serde_json::from_str(json)?,
        labels,
        bibliography,
        publication,
    )
}
