use std::fmt::Display;

use errors::error::generic_error;
use libs::{serde_json, slugify::slugify};
use parser::{FrontMatter, ParsedSource};
use serde::Serialize;

use super::tag::Tag;

#[derive(Serialize, Default, Clone)]
pub struct Article {
    pub title: String,
    pub description: String,
    pub author: String,
    pub date: String,
    pub target: String,
    pub tags: Vec<Tag>,
}

impl Display for Article {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = serde_json::to_string(self).map_err(|e| {
            generic_error(format!(
                "Serializing feed item into JSON string failed: {}",
                e.to_string()
            ))
        });

        write!(f, "{}", result.unwrap_or_default())
    }
}

impl Into<libs::serde_json::Value> for Article {
    fn into(self) -> libs::serde_json::Value {
        serde_json::to_value(self).expect("Converting Article to serde_json::Value failed!")
    }
}

impl From<ParsedSource> for Article {
    fn from(value: ParsedSource) -> Self {
        if let Some(front_matter) = value.front_matter() {
            let err_msg = |f: &str| format!("Field {} is not set!", f);

            let FrontMatter {
                author,
                tags,
                title,
                description,
                published,
                ..
            } = front_matter;

            let parsed_tags: Vec<Tag> = tags
                .iter()
                .flatten()
                .map(String::as_str)
                .map(Tag::from)
                .collect();

            let author = author
                .as_ref()
                .map(|v| v.join(", "))
                .expect(&err_msg("author"));
            let title = title.as_ref().expect(&err_msg("title")).clone();
            let description = description.as_ref().expect(&err_msg("description")).clone();
            let description =
                markdown::string_to_html(&description, &markdown::MarkdownOptions::default());
            let date = published.as_ref().expect(&err_msg("date")).clone();
            let target = format!("/notes/{}.html", slugify!(&title));
            return Article {
                title,
                description,
                author,
                date,
                tags: parsed_tags,
                target,
            };
        }

        Article::default()
    }

    // fn from_parsed_source(parsed_source: ParsedSource) -> Result<Article, Error> {
    //     if let Some(front_matter) = parsed_source.front_matter() {
    //         let err_msg = |f: &str| format!("Field {} is not set!", f);

    //         let FrontMatter {
    //             author,
    //             tags,
    //             title,
    //             description,
    //             published,
    //             ..
    //         } = front_matter;

    //         let mut parsed_tags: Vec<Tag> = Vec::new();
    //         if let Some(tags) = tags.as_ref() {
    //             for tag in tags {
    //                 parsed_tags.push(Tag::new(tag.clone()));
    //             }
    //         }

    //         let author = author
    //             .as_ref()
    //             .map(|v| v.join(", "))
    //             .expect(&err_msg("author"));
    //         let title = title.as_ref().expect(&err_msg("title")).clone();
    //         let description = description.as_ref().expect(&err_msg("description")).clone();
    //         let description =
    //             markdown::string_to_html(&description, &markdown::MarkdownOptions::default());
    //         let date = published.as_ref().expect(&err_msg("date")).clone();
    //         let target = format!("/notes/{}.html", slugify!(&title));
    //         return Ok(Article {
    //             title,
    //             description,
    //             author,
    //             date,
    //             tags: parsed_tags,
    //             target,
    //         });
    //     }

    //     return Err(generic_error("front matter is not set!"));
    // }
}

#[cfg(test)]
mod tests {
    use crate::model::article::Article;

    #[test]
    fn should_implement_trait_display() {
        assert_eq!(
            r#"{"title":"","description":"","author":"","date":"","target":"","tags":[]}"#,
            Article::default().to_string()
        );
    }
}
