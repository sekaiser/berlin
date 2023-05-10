use std::{
    collections::{BTreeSet, HashMap},
    ops::DerefMut,
};

use berlin_core::{
    anyhow::Error, error::generic_error, resolve_path, FrontMatter, MediaType, ParsedSource,
};
use slugify::slugify;

use super::{
    model::{Article, Feed, Picture, Record, Tag},
    AggregatedSources, SortFn,
};

pub fn bln_input_aggregate_all(
    name: &str,
    sources: &[ParsedSource],
    sort_fn: Option<SortFn>,
) -> AggregatedSources {
    let mut v = sources.to_vec();
    sort_fn.map(|f| v.deref_mut().sort_by(f));

    let mut map = HashMap::new();
    map.insert(name.into(), v);

    map
}

pub fn bln_input_feed_aggregate_all(
    _name: &str,
    sources: &[ParsedSource],
    sort_fn: Option<SortFn>,
) -> AggregatedSources {
    let mut v = sources.to_vec();
    sort_fn.map(|f| v.deref_mut().sort_by(f));

    let mut map = HashMap::new();
    map.insert("feed".into(), v);

    map
}

pub fn extract_tags_from_feed(sources: &Vec<ParsedSource>) -> Vec<tera::Value> {
    let uncategorized = "uncategorized";

    let mut hs: BTreeSet<Tag> = BTreeSet::new();

    for src in sources {
        for feed in parse_csv(Some(src)) {
            if feed.tags.is_empty() {
                hs.insert(Tag::new(uncategorized));
            } else {
                hs.extend(feed.tags);
            }
        }
    }

    to_values(hs)
}

pub fn extract_tags(sources: &Vec<ParsedSource>) -> Vec<tera::Value> {
    let mut hs: BTreeSet<Tag> = BTreeSet::new();

    for s in sources {
        if let Some(fm) = s.front_matter() {
            match fm.tags.as_ref() {
                Some(tags) => {
                    hs.extend(tags.iter().map(|t| Tag::new(t)));
                }
                None => {
                    hs.insert(Tag::new("uncategorized"));
                }
            }
        }
    }

    to_values(hs)
}

fn to_values<T: serde::Serialize>(hs: BTreeSet<T>) -> Vec<tera::Value> {
    Vec::from_iter(hs)
        .iter()
        .flat_map(|v| serde_json::to_value(v))
        .collect()
}

pub fn bln_input_aggregate_by_category(
    _name: &str,
    sources: &[ParsedSource],
    maybe_sort_fn: Option<SortFn>,
) -> AggregatedSources {
    let mut map = HashMap::new();

    for s in sources {
        if let Some(fm) = s.front_matter() {
            match fm.tags.as_ref() {
                Some(tags) => {
                    for t in tags {
                        map.entry(t.to_string())
                            .or_insert(Vec::new())
                            .push(s.clone());
                    }
                }
                None => map
                    .entry("uncategorized".to_string())
                    .or_insert(Vec::new())
                    .push(s.clone()),
            }
        }
    }

    if let Some(f) = maybe_sort_fn.as_ref() {
        let _ = map.values_mut().map(|v| {
            v.sort_by(f.clone());
        });
    }

    map
}

pub fn bln_parse_csv_aggregate_by_category(
    _name: &str,
    srcs: &[ParsedSource],
    _maybe_sort_fn: Option<SortFn>,
) -> AggregatedSources {
    let mut map = HashMap::new();

    for src in srcs {
        let specifier = src.specifier();
        let media_type = MediaType::JsonFeedEntry;

        for feed in parse_csv(Some(src)) {
            if feed.tags.is_empty() {
                map.entry("uncategorized".to_string())
                    .or_insert(Vec::new())
                    .push(feed_item_to_parsed_source(&feed, specifier, &media_type));
            } else {
                for t in feed.tags.iter() {
                    map.entry(t.name.clone())
                        .or_insert(Vec::new())
                        .push(feed_item_to_parsed_source(&feed, specifier, &media_type));
                }
            }
        }
    }

    map
}

fn feed_item_to_parsed_source(
    feed: &Feed,
    specifier: &str,
    media_type: &MediaType,
) -> ParsedSource {
    let maybe_json = feed.to_json_string().ok();

    ParsedSource::new(
        specifier.to_string(),
        media_type.to_owned(),
        maybe_json,
        None,
    )
}

pub fn extract_front_matter(source: &ParsedSource) -> (String, ParsedSource, tera::Context) {
    let mut context = tera::Context::new();
    if let Some(front_matter) = source.front_matter() {
        for x in front_matter.get_fields().into_iter() {
            if let (k, Some(val)) = x {
                let key = format!("page_{k}");
                match k {
                    "tags" => {
                        if let Some(tags) = val.downcast_ref::<Vec<String>>() {
                            context.insert(
                                key,
                                &tags.iter().map(|t| Tag::new(t)).collect::<Vec<Tag>>(),
                            );
                        }
                    }
                    _ => context.insert(key, &val.downcast_ref::<String>()),
                }
            }
        }
    }

    let path = resolve_path(source.specifier()).expect("Path is invalid!");
    (
        path.to_file_path()
            .expect("Path is invalid!")
            .file_stem()
            .expect("Not a file!")
            .to_string_lossy()
            .to_string(),
        source.to_owned(),
        context,
    )
}

pub fn parse_feed(sources: &Vec<ParsedSource>) -> tera::Context {
    let mut context = tera::Context::new();
    let mut feed = Vec::new();
    for source in sources {
        feed.append(&mut parse_csv(Some(&source)));
    }
    context.insert("feed", &feed);
    context
}

pub fn parse_csv(maybe_source: Option<&ParsedSource>) -> Vec<Feed> {
    let mut feed: Vec<Feed> = Vec::new();
    if let Some(source) = maybe_source {
        if source.media_type() == MediaType::Csv {
            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(true)
                .delimiter(b',')
                .double_quote(true)
                .from_reader(source.data().as_bytes());

            for result in rdr.deserialize::<Record>() {
                if let Ok(record) = result {
                    feed.push(record.into());
                }
            }
        }
    }

    feed
}

pub fn inject_photo_data() -> tera::Context {
    let mut context = tera::Context::new();
    context.insert("photos", &vec![Picture {
                title: "Gazelli Art House at Art Dubai 2023: Persian Dreams",
                src: "https://d7hftxdivxxvm.cloudfront.net?height=490&amp;quality=80&amp;resize_to=fill&amp;src=https%3A%2F%2Fd32dm0rphc51dk.cloudfront.net%2FM12Gc-3Et8RdEa1E8MFIXQ%2Fnormalized.jpg&amp;width=490",
                srcset: "https://d7hftxdivxxvm.cloudfront.net?height=490&amp;quality=80&amp;resize_to=fill&amp;src=https%3A%2F%2Fd32dm0rphc51dk.cloudfront.net%2FM12Gc-3Et8RAdEa1E8MFIXQ%2Fnormalized.jpg&amp;width=490 1x, https://d7hftxdivxxvm.cloudfront.net?height=980&amp;quality=80&amp;resize_to=fill&amp;src=https%3A%2F%2Fd32dm0rphc51dk.cloudfront.net%2FM12Gc-3Et8RdEa1E8MFIXQ%2Fnormalized.jpg&amp;width=980 2x",
                target: "https://news.artnet.com/art-world/fake-instagram-photography-ai-generated-joe-avery-2260674",
            }]);

    context
}

pub fn collect_by_tag(srcs: &AggregatedSources) -> Vec<(String, tera::Context)> {
    srcs.iter()
        .map(|srcs| {
            let mut articles: Vec<ParsedSource> = Vec::new();
            let mut feed: Vec<ParsedSource> = Vec::new();
            for s in srcs.1 {
                match s.media_type() {
                    MediaType::Html => {
                        articles.push(s.to_owned());
                    }
                    MediaType::JsonFeedEntry => {
                        feed.push(s.to_owned());
                    }
                    _ => {}
                };
            }

            let mut context = tera::Context::new();
            context.extend(collect_articles(&articles));
            context.insert("tag_name", srcs.0);
            context.insert("feed", &feed.to_feed_vec().expect("Cannot parse data!"));
            (srcs.0.to_string(), context)
        })
        .collect()
}

pub fn collect_articles(srcs: &Vec<ParsedSource>) -> tera::Context {
    let mut context = tera::Context::new();

    let articles = srcs
        .iter()
        .map(|src| {
            Article::from_parsed_source(src.to_owned()).expect("Could not parse parsed_source!")
        })
        .take(6)
        .collect::<Vec<Article>>();

    context.insert("articles", &articles);

    context
}

pub trait ToFeed: Sized {
    fn to_feed_vec(&self) -> Result<Vec<Feed>, Error>;
}

pub trait FromParsedSource<T>: Sized {
    fn from_parsed_source(parsed_source: ParsedSource) -> Result<T, Error>;
}

impl ToFeed for Vec<ParsedSource> {
    fn to_feed_vec(&self) -> Result<Vec<Feed>, Error> {
        let mut feed: Vec<Feed> = Vec::new();
        for src in self {
            let feed_item: Feed = serde_json::from_str(src.data())?;
            feed.push(feed_item);
        }
        Ok(feed)
    }
}

impl FromParsedSource<Article> for Article {
    fn from_parsed_source(parsed_source: ParsedSource) -> Result<Article, Error> {
        if let Some(front_matter) = parsed_source.front_matter() {
            let err_msg = |f: &str| format!("Field {} is not set!", f);

            let FrontMatter {
                author,
                tags,
                title,
                description,
                published,
            } = front_matter;

            let mut parsed_tags: Vec<Tag> = Vec::new();
            if let Some(tags) = tags.as_ref() {
                for tag in tags {
                    parsed_tags.push(Tag::new(tag.clone()));
                }
            }

            let author = author
                .as_ref()
                .map(|v| v.join(", "))
                .expect(&err_msg("author"));
            let title = title.as_ref().expect(&err_msg("title")).clone();
            let description = description.as_ref().expect(&err_msg("description")).clone();
            let date = published.as_ref().expect(&err_msg("date")).clone();
            let target = format!("/notes/{}.html", slugify!(&title));
            return Ok(Article {
                title,
                description,
                author,
                date,
                tags: parsed_tags,
                target,
            });
        }

        return Err(generic_error("front matter is not set!"));
    }
}
