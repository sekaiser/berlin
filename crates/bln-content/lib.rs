//! Validated content collections and derived publication assemblies.

#![deny(clippy::print_stderr)]
#![deny(clippy::print_stdout)]

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use berlin_document::ContentId;
use berlin_document::Document;
use serde::Deserialize;
use serde::Serialize;

mod references;
mod routes;
pub use references::{Backlink, ReferenceAnalysis, ReferenceIndex, UnresolvedReference};
pub use routes::{RouteError, document_route, validated_document_slug};

mod authoring;
pub use authoring::{
    AuthoringFinding, AuthoringIssue, AuthoringLocation, AuthoringOrigin, AuthoringReport,
    OriginHeading, Severity,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Collection<T> {
    items: Vec<T>,
}

impl<T> Default for Collection<T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<T> Collection<T> {
    pub fn new(items: impl Into<Vec<T>>) -> Self {
        Self {
            items: items.into(),
        }
    }

    pub fn as_slice(&self) -> &[T] {
        &self.items
    }

    pub fn into_vec(self) -> Vec<T> {
        self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn map<U>(self, mut mapper: impl FnMut(T) -> U) -> Collection<U> {
        Collection::new(self.items.into_iter().map(&mut mapper).collect::<Vec<_>>())
    }
}

impl<T> From<Vec<T>> for Collection<T> {
    fn from(items: Vec<T>) -> Self {
        Self::new(items)
    }
}

impl<T> IntoIterator for Collection<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

pub type DocumentCollection = Collection<Document>;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentRef(pub ContentId);

impl From<&Document> for DocumentRef {
    fn from(document: &Document) -> Self {
        Self(document.id.clone())
    }
}

impl Collection<Document> {
    pub fn validated(self) -> Result<Self, CollectionError> {
        self.validate()?;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), CollectionError> {
        let mut ids = HashSet::with_capacity(self.items.len());
        for document in &self.items {
            validate_collection_document(document)?;

            if !ids.insert(&document.id) {
                return Err(CollectionError::DuplicateDocument(document.id.clone()));
            }
        }
        Ok(())
    }

    pub fn get(&self, reference: &DocumentRef) -> Option<&Document> {
        self.items
            .iter()
            .find(|document| document.id == reference.0)
    }

    pub fn references(&self) -> Collection<DocumentRef> {
        Collection::new(self.items.iter().map(DocumentRef::from).collect::<Vec<_>>())
    }

    /// Borrows documents that are not marked as drafts, preserving collection order.
    pub fn non_drafts(&self) -> impl Iterator<Item = &Document> {
        self.items
            .iter()
            .filter(|document| !document.metadata.draft)
    }

    /// Returns newest-first references, with missing dates last and IDs breaking ties.
    pub fn references_by_published_desc(&self) -> Collection<DocumentRef> {
        let mut documents: Vec<&Document> = self.items.iter().collect();
        documents.sort_by(|left, right| {
            right
                .metadata
                .published
                .cmp(&left.metadata.published)
                .then_with(|| left.id.0.cmp(&right.id.0))
        });
        Collection::new(
            documents
                .into_iter()
                .map(DocumentRef::from)
                .collect::<Vec<_>>(),
        )
    }

    pub fn index_by_tag(&self) -> TagIndex {
        TagIndex::from_documents(self)
    }
}

fn validate_collection_document(document: &Document) -> Result<(), CollectionError> {
    document
        .validate()
        .map_err(CollectionError::InvalidDocument)?;

    if document.id.0 == document.provenance.source {
        return Err(CollectionError::MissingDocumentId {
            source: document.provenance.source.clone(),
        });
    }

    Ok(())
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TagIndex {
    groups: BTreeMap<String, TagGroup>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TagGroup {
    pub documents: Collection<DocumentRef>,
}

impl TagIndex {
    pub fn from_documents(documents: &DocumentCollection) -> Self {
        let mut index = Self::default();
        for document in documents.as_slice() {
            index.add_document(document);
        }
        index
    }

    fn add_document(&mut self, document: &Document) {
        for tag in effective_tags(&document.metadata.tags) {
            self.groups
                .entry(tag.to_owned())
                .or_default()
                .documents
                .items
                .push(DocumentRef::from(document));
        }
    }

    pub fn groups(&self) -> &BTreeMap<String, TagGroup> {
        &self.groups
    }

    pub fn documents<'a>(
        &'a self,
        tag: &str,
        source: &'a DocumentCollection,
    ) -> impl Iterator<Item = &'a Document> {
        self.groups
            .get(tag)
            .into_iter()
            .flat_map(|group| group.documents.as_slice())
            .filter_map(|reference| source.get(reference))
    }
}

fn effective_tags(tags: &[String]) -> impl Iterator<Item = &str> {
    tags.iter()
        .map(String::as_str)
        .chain(tags.is_empty().then_some("uncategorized"))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CollectionError {
    InvalidDocument(berlin_document::DocumentValidationError),
    MissingDocumentId {
        source: String,
    },
    DuplicateDocument(ContentId),
    UnresolvedReference {
        source: String,
        anchor: String,
        target: ContentId,
    },
}

impl fmt::Display for CollectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedReference {
                source,
                anchor,
                target,
            } => write!(
                formatter,
                "unresolved document reference in '{source}#{anchor}': target '{}' is not in the published collection",
                target.0
            ),
            Self::InvalidDocument(error) => error.fmt(formatter),
            Self::MissingDocumentId { source } => {
                write!(formatter, "document '{source}' has no stable content ID")
            }
            Self::DuplicateDocument(id) => {
                write!(
                    formatter,
                    "document collection contains duplicate ID '{}'",
                    id.0
                )
            }
        }
    }
}

impl Error for CollectionError {}

#[cfg(test)]
mod tests {
    use berlin_document::DocumentKind;
    use berlin_document::Metadata;
    use berlin_document::Provenance;
    use berlin_document::SourceFormat;

    use super::*;

    fn document(id: &str, published: &str, tags: &[&str], draft: bool) -> Document {
        Document {
            id: ContentId(id.into()),
            kind: DocumentKind::Article,
            metadata: Metadata {
                published: Some(published.parse().unwrap()),
                tags: tags.iter().map(|tag| (*tag).into()).collect(),
                draft,
                ..Metadata::default()
            },
            blocks: Vec::new(),
            relations: Vec::new(),
            provenance: Provenance {
                source: format!("file:///{id}.md"),
                source_format: SourceFormat::Markdown,
                source_hash: "0".repeat(64),
            },
        }
    }

    #[test]
    fn publication_references_have_deterministic_order_without_reordering_documents() {
        let mut undated = document("undated", "2026-01-01", &[], false);
        undated.metadata.published = None;
        let documents = DocumentCollection::new(vec![
            undated,
            document("b", "2026-01-01", &[], false),
            document("older", "2025-01-01", &[], false),
            document("a", "2026-01-01", &[], true),
        ]);
        let references = documents.references_by_published_desc();
        let ids: Vec<_> = references
            .as_slice()
            .iter()
            .map(|reference| reference.0.0.as_str())
            .collect();
        assert_eq!(ids, ["a", "b", "older", "undated"]);
        assert_eq!(documents.as_slice()[0].id.0, "undated");
    }

    #[test]
    fn collections_validate_identity_and_derive_views() {
        let documents = DocumentCollection::new(vec![
            document("older", "2025-01-01", &["rust"], false),
            document("newer", "2026-01-01", &["rust", "berlin"], false),
            document("draft", "2027-01-01", &[], true),
        ]);

        assert_eq!(documents.validate(), Ok(()));
        assert_eq!(documents.non_drafts().count(), 2);
        assert_eq!(
            documents.references_by_published_desc().as_slice()[0].0.0,
            "draft"
        );

        let tags = documents.index_by_tag();
        assert_eq!(tags.groups()["rust"].documents.len(), 2);
        assert_eq!(tags.groups()["uncategorized"].documents.len(), 1);
        assert_eq!(
            tags.documents("berlin", &documents)
                .map(|document| document.id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["newer"]
        );
    }

    #[test]
    fn collections_reject_duplicate_document_ids() {
        let documents = DocumentCollection::new(vec![
            document("same", "2025-01-01", &[], false),
            document("same", "2026-01-01", &[], false),
        ]);

        assert!(matches!(
            documents.validate(),
            Err(CollectionError::DuplicateDocument(ContentId(id))) if id == "same"
        ));
    }

    #[test]
    fn collections_reject_source_uris_as_fallback_ids() {
        let mut document = document("article", "2026-01-01", &[], false);
        document.id = ContentId(document.provenance.source.clone());
        let documents = DocumentCollection::new(vec![document]);

        assert!(matches!(
            documents.validate(),
            Err(CollectionError::MissingDocumentId { source })
                if source == "file:///article.md"
        ));
    }
}
