use anyhow::Context as _;
use anyhow::Error;
use anyhow::bail;
use berlin_content::Feed;
use berlin_content::FeedItem;
use berlin_content::FeedItemId;
use serde::Deserialize;
use serde::Deserializer;
use url::Url;

use super::SourceFile;

#[derive(Deserialize)]
struct Record {
    #[serde(rename = "Title")]
    title: String,
    #[serde(rename = "Url")]
    url: String,
    #[serde(rename = "Date Added")]
    date_added: String,
    #[serde(rename = "Manual Tags", deserialize_with = "deserialize_tags")]
    tags: Vec<String>,
}

fn deserialize_tags<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(String::deserialize(deserializer)?
        .split("; ")
        .map(str::to_owned)
        .collect())
}

pub(super) fn parse(sources: &[SourceFile]) -> Result<Feed, Error> {
    let mut items = Vec::new();
    for source in sources {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .delimiter(b',')
            .double_quote(true)
            .from_reader(source.text.as_bytes());
        for (index, record) in reader.deserialize::<Record>().enumerate() {
            let record = record.with_context(|| {
                format!(
                    "Invalid feed record {} in {}",
                    index + 2,
                    source.path.display()
                )
            })?;
            let parsed_url = Url::parse(&record.url).with_context(|| {
                format!(
                    "Invalid feed URL '{}' in {} record {}",
                    record.url,
                    source.path.display(),
                    index + 2
                )
            })?;
            let Some(host) = parsed_url.host().map(|host| host.to_string()) else {
                bail!(
                    "Feed URL '{}' has no host in {} record {}",
                    record.url,
                    source.path.display(),
                    index + 2
                );
            };
            items.push(FeedItem {
                id: FeedItemId(record.url.clone()),
                title: record.title,
                date_added: record.date_added,
                url: record.url,
                host,
                tags: record.tags,
                source: source.uri.clone(),
            });
        }
    }

    let feed = Feed::new(items);
    feed.validate()?;
    Ok(feed)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn csv_projects_directly_to_a_typed_feed() {
        let source = SourceFile {
            path: PathBuf::from("data/feed.csv"),
            uri: "file:///project/data/feed.csv".into(),
            text: concat!(
                "Title,Url,Date Added,Manual Tags\n",
                "Berlin,https://example.com/article,2026-09-05,rust; publishing\n"
            )
            .into(),
        };

        let feed = parse(&[source]).unwrap();

        assert_eq!(feed.len(), 1);
        let item = &feed.as_slice()[0];
        assert_eq!(item.title, "Berlin");
        assert_eq!(item.host, "example.com");
        assert_eq!(item.tags, ["rust", "publishing"]);
        assert_eq!(item.source, "file:///project/data/feed.csv");
    }

    #[test]
    fn malformed_records_are_reported_instead_of_discarded() {
        let source = SourceFile {
            path: PathBuf::from("data/feed.csv"),
            uri: "file:///project/data/feed.csv".into(),
            text: concat!(
                "Title,Url,Date Added,Manual Tags\n",
                "Broken,not a URL,2026-09-05,rust\n"
            )
            .into(),
        };

        let error = parse(&[source]).unwrap_err();

        assert!(error.to_string().contains("Invalid feed URL 'not a URL'"));
        assert!(error.to_string().contains("record 2"));
    }
}
