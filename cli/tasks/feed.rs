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
    #[serde(rename = "Annotation", default)]
    annotation: Option<String>,
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
                annotation: record
                    .annotation
                    .map(|note| note.trim().to_owned())
                    .filter(|note| !note.is_empty()),
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
        assert_eq!(item.annotation, None);
    }

    #[test]
    fn annotations_are_optional_plain_text_not_imported_abstracts() {
        let source = SourceFile {
            path: PathBuf::from("data/feed.csv"),
            uri: "file:///project/data/feed.csv".into(),
            text: concat!(
                "Title,Url,Date Added,Manual Tags,Abstract Note,Annotation\n",
                "One,https://example.com/one,2026-09-08,rust,Publisher summary,\"  A useful distinction, worth revisiting.  \"\n",
                "Two,https://example.com/two,2026-09-08,rust,Not my opinion,\"   \"\n",
                "Three,https://example.com/three,2026-09-08,rust,,\"Literal <em>text</em> & punctuation\"\n"
            )
            .into(),
        };
        let feed = parse(&[source]).unwrap();
        let items = feed.as_slice();
        assert_eq!(
            items[0].annotation.as_deref(),
            Some("A useful distinction, worth revisiting.")
        );
        assert_eq!(items[1].annotation, None);
        assert_eq!(
            items[2].annotation.as_deref(),
            Some("Literal <em>text</em> & punctuation")
        );
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
