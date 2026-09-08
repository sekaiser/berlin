//! Channel-neutral document values and their publication-time invariants.
//!
//! Parsers and scripts may assemble mutable [`Document`] values, but every value must pass
//! [`Document::validate`] before it crosses into a publishable collection.

#![deny(clippy::print_stderr)]
#![deny(clippy::print_stdout)]

mod code_references;
mod date;
mod references;
pub use date::{InvalidPublicationDate, PublicationDate};
pub use references::{DocumentLink, ReferenceOccurrence, plain_text};

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub id: ContentId,
    #[serde(default)]
    pub kind: DocumentKind,
    pub metadata: Metadata,
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    pub provenance: Provenance,
}

impl Document {
    pub fn validate(&self) -> Result<(), DocumentValidationError> {
        validate_identifier("content ID", &self.id.0)?;
        validate_metadata(&self.metadata)?;
        validate_provenance(&self.provenance)?;
        self.relations.iter().try_for_each(validate_relation)?;
        validate_blocks(&self.blocks)?;
        let anchors = code_references::validate(&self.blocks)?;
        references::validate(self, anchors)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DocumentValidationError {
    #[error("{field} must not be empty or contain control characters (got '{value}')")]
    InvalidIdentifier { field: &'static str, value: String },
    #[error("{field} must not be empty")]
    EmptyValue { field: &'static str, value: String },
    #[error("heading level must be between 1 and 6, got {0}")]
    InvalidHeadingLevel(u8),
    #[error("source hash must be a 64-character hexadecimal SHA-256 digest, got '{0}'")]
    InvalidSourceHash(String),
    #[error("invalid code reference: {0}")]
    InvalidCodeReference(String),
    #[error("invalid document reference: {0}")]
    InvalidDocumentReference(String),
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), DocumentValidationError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(DocumentValidationError::InvalidIdentifier {
            field,
            value: value.into(),
        });
    }
    Ok(())
}

fn validate_metadata(metadata: &Metadata) -> Result<(), DocumentValidationError> {
    if let Some(preview) = &metadata.preview {
        validate_identifier("preview source", &preview.source)?;
        validate_non_empty("preview alternative text", &preview.alt)?;
    }
    metadata
        .tags
        .iter()
        .try_for_each(|tag| validate_non_empty("tag", tag))
}

fn validate_provenance(provenance: &Provenance) -> Result<(), DocumentValidationError> {
    validate_non_empty("provenance source", &provenance.source)?;
    validate_source_hash(&provenance.source_hash)
}

fn validate_relation(relation: &Relation) -> Result<(), DocumentValidationError> {
    validate_non_empty("relation kind", &relation.kind)?;
    validate_identifier("relation target", &relation.target.0)
}

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), DocumentValidationError> {
    if value.trim().is_empty() {
        return Err(DocumentValidationError::EmptyValue {
            field,
            value: value.into(),
        });
    }

    Ok(())
}

fn validate_source_hash(source_hash: &str) -> Result<(), DocumentValidationError> {
    if source_hash.len() != 64 || !source_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(DocumentValidationError::InvalidSourceHash(
            source_hash.into(),
        ));
    }

    Ok(())
}

fn validate_blocks(blocks: &[Block]) -> Result<(), DocumentValidationError> {
    for block in blocks {
        match block {
            Block::Heading { level, .. } => validate_heading_level(*level)?,
            Block::Section {
                id, level, blocks, ..
            } => {
                validate_identifier("section ID", &id.0)?;
                validate_heading_level(*level)?;
                validate_blocks(blocks)?;
            }
            Block::Component {
                id, name, blocks, ..
            } => {
                validate_identifier("component ID", &id.0)?;
                validate_non_empty("component name", name)?;
                validate_blocks(blocks)?;
            }
            Block::BlockQuote { blocks } | Block::FootnoteDefinition { blocks, .. } => {
                validate_blocks(blocks)?;
            }
            Block::List(list) => {
                for item in &list.items {
                    validate_blocks(&item.blocks)?;
                }
            }
            Block::Paragraph { .. }
            | Block::Code(_)
            | Block::Figure(_)
            | Block::Html { .. }
            | Block::Table(_)
            | Block::ThematicBreak => {}
        }
    }
    Ok(())
}

fn validate_heading_level(level: u8) -> Result<(), DocumentValidationError> {
    if !(1..=6).contains(&level) {
        return Err(DocumentValidationError::InvalidHeadingLevel(level));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentId(pub String);

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    #[default]
    Article,
    Guide,
    Note,
    SocialPost,
    Custom(String),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ComponentId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    pub kind: String,
    pub target: ContentId,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    /// Optional content-owned artwork used when presenting this document in collections.
    #[serde(default)]
    pub preview: Option<Preview>,
    pub title: Option<String>,
    /// Optional stable website slug; independent of the title and content ID.
    #[serde(default)]
    pub slug: Option<String>,
    /// Former website slugs redirected directly to `slug` during publication.
    #[serde(default)]
    pub previous_slugs: Vec<String>,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub published: Option<PublicationDate>,
    pub modified: Option<PublicationDate>,
    pub tags: Vec<String>,
    pub draft: bool,
    /// Author opt-in to a discussion; provider configuration belongs to the website.
    #[serde(default)]
    pub comments: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Preview {
    pub source: String,
    pub alt: String,
    pub width: std::num::NonZeroU32,
    pub height: std::num::NonZeroU32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub source: String,
    pub source_format: SourceFormat,
    pub source_hash: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    Org,
    Markdown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Paragraph {
        content: Vec<Inline>,
    },
    Heading {
        level: u8,
        id: Option<String>,
        content: Vec<Inline>,
    },
    BlockQuote {
        blocks: Vec<Block>,
    },
    List(List),
    Code(CodeBlock),
    Figure(Figure),
    Html {
        value: String,
    },
    Table(Table),
    ThematicBreak,
    FootnoteDefinition {
        name: String,
        blocks: Vec<Block>,
    },
    Section {
        id: ComponentId,
        #[serde(default = "default_section_level")]
        level: u8,
        role: Option<String>,
        title: Vec<Inline>,
        blocks: Vec<Block>,
    },
    Component {
        id: ComponentId,
        name: String,
        properties: BTreeMap<String, PropertyValue>,
        blocks: Vec<Block>,
    },
}

fn default_section_level() -> u8 {
    2
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum PropertyValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
    List(Vec<PropertyValue>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct List {
    pub ordered: bool,
    pub start: Option<usize>,
    pub tight: bool,
    pub items: Vec<ListItem>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ListItem {
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CodeBlock {
    pub language: Option<String>,
    pub value: String,
    #[serde(default)]
    pub references: CodeReferences,
    /// Caption content belongs to this listing, independent of its output channel.
    #[serde(default)]
    pub caption: Option<Vec<Inline>>,
    /// One-based source-line ranges, independent of displayed line numbering.
    #[serde(default)]
    pub highlights: Vec<CodeLineRange>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CodeLineRange {
    pub start: usize,
    pub end: usize,
}

/// Author-assigned listing identity and exported line-reference coordinates.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CodeReferences {
    pub id: Option<String>,
    pub line_anchor_prefix: Option<String>,
    pub first_line: usize,
}

impl Default for CodeReferences {
    fn default() -> Self {
        Self {
            id: None,
            line_anchor_prefix: None,
            first_line: 1,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Figure {
    pub source: String,
    pub caption: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub alignments: Vec<TableAlignment>,
    pub rows: Vec<TableRow>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TableRow {
    pub header: bool,
    pub cells: Vec<Vec<Inline>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Inline {
    Text {
        value: String,
    },
    Emphasis {
        content: Vec<Inline>,
    },
    Strong {
        content: Vec<Inline>,
    },
    Strikethrough {
        content: Vec<Inline>,
    },
    Code {
        value: String,
    },
    Link {
        destination: String,
        title: Option<String>,
        content: Vec<Inline>,
    },
    DocumentLink(DocumentLink),
    Image {
        source: String,
        title: Option<String>,
        description: Vec<Inline>,
    },
    Html {
        value: String,
    },
    SoftBreak,
    LineBreak,
    FootnoteReference {
        name: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> Document {
        Document {
            id: ContentId("article-1".into()),
            kind: DocumentKind::Article,
            metadata: Metadata {
                published: Some("2026-09-06".parse().unwrap()),
                ..Metadata::default()
            },
            blocks: Vec::new(),
            relations: Vec::new(),
            provenance: Provenance {
                source: "file:///content/article.md".into(),
                source_format: SourceFormat::Markdown,
                source_hash: "0".repeat(64),
            },
        }
    }

    #[test]
    fn validates_semantic_invariants() {
        assert_eq!(document().validate(), Ok(()));

        let mut invalid = document();
        invalid.id = ContentId(String::new());
        assert!(matches!(
            invalid.validate(),
            Err(DocumentValidationError::InvalidIdentifier {
                field: "content ID",
                ..
            })
        ));

        let mut invalid = document();
        invalid.blocks.push(Block::Section {
            id: ComponentId("section".into()),
            level: 7,
            role: None,
            title: Vec::new(),
            blocks: Vec::new(),
        });
        assert_eq!(
            invalid.validate(),
            Err(DocumentValidationError::InvalidHeadingLevel(7))
        );
    }
}
