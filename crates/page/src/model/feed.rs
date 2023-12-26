use std::fmt::Display;

use errors::error::generic_error;

use libs::{serde_json, url::Url};
use serde::{Deserialize, Serialize};

use crate::ParsedSource;

use super::{
    record::Record,
    tag::{Tag, Tags},
};

#[derive(Hash, Eq, PartialEq, Debug, Serialize, Deserialize, PartialOrd, Ord, Clone, Default)]
pub struct Feed {
    pub title: String,
    pub date_added: String,
    pub url: String,
    pub host: String,
    pub tags: Vec<Tag>,
}

impl Display for Feed {
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

impl From<&ParsedSource> for Feed {
    fn from(value: &ParsedSource) -> Self {
        serde_json::from_str(value.data()).unwrap()
    }
}

impl Into<libs::serde_json::Value> for Feed {
    fn into(self) -> libs::serde_json::Value {
        serde_json::to_value(self).expect("Converting Feed to serde_json::Value failed!")
    }
}

impl Into<Tags> for Feed {
    fn into(self) -> Tags {
        Tags::from(self.tags).get_or_when_empty(Tags::uncategorized())
    }
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

#[cfg(test)]
mod test {

    use crate::model::record::Record;

    use super::Feed;

    #[test]
    fn should_implement_trait_display() {
        let feed = Feed::default();
        assert_eq!(
            r#"{"title":"","date_added":"","url":"","host":"","tags":[]}"#,
            feed.to_string()
        );
    }

    #[test]
    fn feed_from_record() {
        let record = Record::default();
        let feed = Feed::from(record);
        assert_eq!(
            Feed {
                url: "https://example.com".into(),
                host: "example.com".into(),
                ..Default::default()
            },
            feed
        );
    }
}
