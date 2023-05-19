use serde::Deserialize;

use crate::MediaType;
use std::{any::Any, fs::Metadata, sync::Arc};

#[derive(Clone, Debug)]
struct ParsedSourceInner {
    specifier: String,
    media_type: MediaType,
    data: Option<String>,
    front_matter: Option<FrontMatter>,
    metadata: Option<Metadata>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FrontMatter {
    pub title: Option<String>,
    #[serde(rename = "date")]
    pub published: Option<String>,
    pub author: Option<Vec<String>>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub id: Option<String>,
}

impl FrontMatter {
    pub fn get_fields(&self) -> Vec<(&str, Option<Box<dyn Any>>)> {
        vec![
            (
                "title",
                self.title
                    .clone()
                    .map(|v| Box::new(v) as Box<dyn Any>)
                    .or(None),
            ),
            (
                "description",
                self.description
                    .clone()
                    .map(|v| Box::new(v) as Box<dyn Any>)
                    .or(None),
            ),
            (
                "tags",
                self.tags
                    .clone()
                    .map(|v| Box::new(v) as Box<dyn Any>)
                    .or(None),
            ),
            (
                "published",
                self.published
                    .clone()
                    .map(|v| Box::new(v) as Box<dyn Any>)
                    .or(None),
            ),
            (
                "author",
                self.author
                    .clone()
                    .map(|v| Box::new(v) as Box<dyn Any>)
                    .or(None),
            ),
            (
                "id",
                self.id
                    .clone()
                    .map(|v| Box::new(v) as Box<dyn Any>)
                    .or(None),
            ),
        ]
    }
}

#[derive(Clone, Debug)]
pub struct ParsedSource {
    inner: Arc<ParsedSourceInner>,
}

impl ParsedSource {
    /// Gets the module specifier of the module.
    pub fn specifier(&self) -> &str {
        &self.inner.specifier
    }

    /// Gets the media type of the module.
    pub fn media_type(&self) -> MediaType {
        self.inner.media_type
    }

    pub fn has_data(&self) -> bool {
        self.inner.data.is_some()
    }

    pub fn data(&self) -> &String {
        self.inner
            .data
            .as_ref()
            .expect("Data not found because it was not captured during parsing.")
    }

    pub fn front_matter(&self) -> Option<&FrontMatter> {
        self.inner.front_matter.as_ref()
    }

    pub fn metadata(&self) -> Option<&Metadata> {
        self.inner.metadata.as_ref()
    }
}

#[derive(Clone, Debug)]
pub struct ParsedSourceBuilder {
    specifier: String,
    media_type: MediaType,
    content: Option<String>,
    front_matter: Option<FrontMatter>,
    metadata: Option<Metadata>,
}

impl ParsedSourceBuilder {
    pub fn new(specifier: String, media_type: MediaType) -> Self {
        ParsedSourceBuilder {
            specifier,
            media_type,
            content: None,
            front_matter: None,
            metadata: None,
        }
    }

    pub fn content(mut self, content: String) -> Self {
        self.content = Some(content);
        self
    }

    pub fn maybe_content(mut self, maybe_content: Option<String>) -> Self {
        self.content = maybe_content;
        self
    }

    pub fn front_matter(mut self, front_matter: FrontMatter) -> Self {
        self.front_matter = Some(front_matter);
        self
    }

    pub fn maybe_front_matter(mut self, maybe_front_matter: Option<FrontMatter>) -> Self {
        self.front_matter = maybe_front_matter;
        self
    }

    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn build(self) -> ParsedSource {
        ParsedSource {
            inner: Arc::new(ParsedSourceInner {
                specifier: self.specifier,
                media_type: self.media_type,
                data: self.content,
                front_matter: self.front_matter,
                metadata: self.metadata,
            }),
        }
    }
}
