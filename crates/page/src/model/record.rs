use std::fmt::Display;

use super::tag::Tag;
use errors::error::generic_error;
use libs::serde_json;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Record {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Author")]
    author: String,
    #[serde(rename = "Title")]
    pub title: String,
    #[serde(rename = "Url")]
    pub url: String,
    #[serde(rename = "Date")]
    date: String,
    #[serde(rename = "Date Added")]
    pub date_added: String,
    #[serde(rename = "Manual Tags")]
    #[serde(deserialize_with = "deserialize_tags")]
    pub tags: Vec<Tag>,
}

fn deserialize_tags<'de, D>(deserializer: D) -> Result<Vec<Tag>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut tags = Vec::new();
    let buf = String::deserialize(deserializer)?;
    for tag in buf.split("; ") {
        tags.push(Tag::new(tag.to_string()));
    }

    Ok(tags)
}

impl Default for Record {
    fn default() -> Self {
        Self {
            key: "".into(),
            author: "".into(),
            title: "".into(),
            url: "https://example.com".into(),
            date: "".into(),
            date_added: "".into(),
            tags: vec![],
        }
    }
}

impl Display for Record {
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

#[cfg(test)]
mod tests {
    use crate::model::record::Record;

    #[test]
    fn should_implement_trait_display() {
        assert_eq!(
            r#"{"Key":"","Author":"","Title":"","Url":"https://example.com","Date":"","Date Added":"","Manual Tags":[]}"#,
            Record::default().to_string()
        );
    }
}
