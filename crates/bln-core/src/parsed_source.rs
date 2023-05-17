use serde::Deserialize;

use crate::MediaType;
use std::{any::Any, sync::Arc};

#[derive(Clone, Debug)]
struct ParsedSourceInner {
    specifier: String,
    media_type: MediaType,
    data: Option<String>,
    front_matter: Option<FrontMatter>,
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
    pub fn new(
        specifier: String,
        media_type: MediaType,
        data: Option<String>,
        front_matter: Option<FrontMatter>,
    ) -> Self {
        ParsedSource {
            inner: Arc::new(ParsedSourceInner {
                specifier,
                media_type,
                data,
                front_matter,
            }),
        }
    }

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
}
