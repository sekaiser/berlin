use errors::anyhow::Error;
use errors::error::generic_error;

use berlin_core::{url::Url, ParsedSource};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Serialize, Deserialize, Hash, Eq, PartialEq, Debug, PartialOrd, Ord, Clone)]
pub struct Tag {
    pub name: String,
    pub target: String,
}

impl Tag {
    pub fn new<S: Into<String>>(name: S) -> Self {
        let n = name.into();
        Self {
            target: format!("/tags/{}.html", &n),
            name: n,
        }
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Serialize, Deserialize, PartialOrd, Ord, Clone)]
pub struct Feed {
    pub title: String,
    pub date_added: String,
    pub url: String,
    pub host: String,
    pub tags: Vec<Tag>,
}

impl Feed {
    pub fn to_json_string(&self) -> Result<String, Error> {
        serde_json::to_string(self).map_err(|e| {
            generic_error(format!(
                "Serializing feed item into JSON string failed: {}",
                e.to_string()
            ))
        })
    }
}

impl From<&ParsedSource> for Feed {
    fn from(value: &ParsedSource) -> Self {
        serde_json::from_str(value.data()).unwrap()
    }
}

#[derive(Serialize)]
pub struct Article {
    pub title: String,
    pub description: String,
    pub author: String,
    pub date: String,
    pub target: String,
    pub tags: Vec<Tag>,
}

#[derive(Serialize)]
pub struct Picture<'a> {
    pub title: &'a str,
    pub src: &'a str,
    pub srcset: &'a str,
    pub target: &'a str,
}

#[derive(Deserialize)]
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
    tags: Vec<Tag>,
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

impl From<Record> for Feed {
    fn from(r: Record) -> Self {
        let Record {
            title,
            date_added,
            url,
            tags,
            ..
        } = r;
        let host = Url::parse(&url)
            .expect("Invalid URL!")
            .host()
            .expect("Host missing!")
            .to_string();

        Self {
            title,
            date_added,
            url,
            host,
            tags,
        }
    }
}
