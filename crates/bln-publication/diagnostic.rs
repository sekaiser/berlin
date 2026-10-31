use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, Diagnostic>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Construction,
    Resolution,
    Capability,
    Presentation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Code {
    UnknownVersion,
    InvalidIdentifier,
    UnknownComponent,
    DuplicateComponent,
    DuplicateOccurrenceKey,
    InvalidProperty,
    UnknownProperty,
    MissingProperty,
    UnknownSlot,
    MissingSlot,
    EmptySlot,
    InvalidSource,
    DraftSelection,
    SourceDigestMismatch,
    MissingExcerpt,
    UnresolvedReference,
    UnsupportedChannelComponent,
    UnsupportedChannelFeature,
    MissingTemplate,
    TemplateContextFailure,
    SlotOmission,
    SlotRepetition,
    UnsafeUrl,
    ResourceLimit,
    OutputConflict,
    HeadingDepthExceeded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{code:?} ({stage:?}) at {path}: {detail}")]
pub struct Diagnostic {
    pub code: Code,
    pub stage: Stage,
    pub publication_id: Option<Box<str>>,
    pub path: Box<str>,
    pub source_id: Option<Box<str>>,
    pub selector: Option<Box<str>>,
    pub template: Option<Box<str>>,
    pub detail: Box<str>,
}

impl Diagnostic {
    pub fn new(code: Code, stage: Stage, detail: impl Into<String>) -> Self {
        Self {
            code,
            stage,
            publication_id: None,
            path: "root".into(),
            source_id: None,
            selector: None,
            template: None,
            detail: detail.into().into_boxed_str(),
        }
    }

    pub fn at(mut self, publication: &str, path: &str) -> Self {
        self.publication_id = Some(publication.into());
        self.path = path.into();
        self
    }

    pub fn source(mut self, id: &str, selector: Option<&str>) -> Self {
        self.source_id = Some(id.into());
        self.selector = selector.map(Into::into);
        self
    }
}
