use std::{
    collections::{BTreeSet, HashMap},
    ops::DerefMut,
};

use errors::error::generic_error;
use files::{resolve_path, MediaType};
use libs::anyhow::Error;
use libs::serde_json;
use libs::slugify::slugify;
use libs::tera;
use page::model::{article::Article, feed::Feed, picture::Picture, record::Record, tag::Tag};
use parser::{FrontMatter, ParsedSource, ParsedSourceBuilder};
use serde::Serialize;

use super::{AggregatedSources, SortFn};

////////////////////

pub mod task {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
    pub struct Output(String);

    impl Output {
        pub fn new<S: Into<String>>(name: S) -> Self {
            Self(name.into())
        }

        pub fn as_str(&self) -> &str {
            self.0.as_str()
        }

        pub fn to_string(self) -> String {
            self.0
        }

        pub fn replace(&self, from: &str, to: &str) -> String {
            self.0.replace(from, to)
        }

        pub fn contains(&self, s: &str) -> bool {
            self.0.contains(s)
        }
    }

    impl AsRef<str> for Output {
        fn as_ref(&self) -> &str {
            self.0.as_str()
        }
    }

    impl From<String> for Output {
        fn from(s: String) -> Self {
            Self::new(s)
        }
    }

    impl From<&str> for Output {
        fn from(s: &str) -> Self {
            Self::new(s)
        }
    }

    impl std::fmt::Display for Output {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }
}

pub mod tags {
    use std::{cell::RefCell, collections::HashMap};

    use page::model::tag::Tag;
    use parser::ParsedSource;

    pub struct TaggedParsedSourceStore {
        store: RefCell<HashMap<Tag, ParsedSource>>,
    }

    impl std::default::Default for TaggedParsedSourceStore {
        fn default() -> Self {
            Self {
                store: RefCell::new(HashMap::new()),
            }
        }
    }

    impl TaggedParsedSourceStore {
        pub fn set_parsed_source(
            &self,
            tag: Tag,
            parsed_source: ParsedSource,
        ) -> Option<ParsedSource> {
            self.store.borrow_mut().insert(tag, parsed_source)
        }

        pub fn get_parsed_source(&self, tag: &Tag) -> Option<ParsedSource> {
            self.store.borrow_mut().get(tag).cloned()
        }

        pub fn values(&self) -> Vec<ParsedSource> {
            self.store
                .borrow()
                .values()
                .map(|value| value.clone())
                .collect::<Vec<_>>()
        }

        // Return all parsed sources
        // pub fn values(&self) -> Values<Tag, ParsedSource> {
        //     let store = self.store.borrow();
        //     Values {
        //         inner: store.iter(),
        //     }
        // }
    }
}

pub mod util {
    use std::collections::BTreeSet;

    use libs::{serde_json, tera};
    use page::model::{article::Article, feed::Feed, tag::Tag};
    use parser::ParsedSource;

    use crate::tasks::AggregatedSources;

    use super::{csv::from_parsed_source, FromParsedSource};

    pub fn to_article(parsed_source: &ParsedSource) -> Article {
        Article::from_parsed_source(parsed_source.to_owned())
            .expect("Could not parse parsed_source!")
    }

    pub trait EventHandler {
        fn on_data_loaded(&self, srcs: &AggregatedSources) -> tera::Value;
    }

    pub struct GetArticlesByKey<'a, Key>(pub &'a Key)
    where
        Key: AsRef<str> + ?Sized;

    impl<'a> EventHandler for GetArticlesByKey<'a, str> {
        fn on_data_loaded(&self, srcs: &AggregatedSources) -> tera::Value {
            srcs.get(self.0)
                .into_iter()
                .flatten()
                .map(to_article)
                .flat_map(serde_json::to_value)
                .collect()
        }
    }

    pub struct GetFeedByKey<'a, Key>(pub &'a Key)
    where
        Key: AsRef<str> + ?Sized;

    impl<'a> EventHandler for GetFeedByKey<'a, str> {
        fn on_data_loaded(&self, srcs: &AggregatedSources) -> tera::Value {
            srcs.get(self.0)
                .into_iter()
                .flatten()
                .flat_map(from_parsed_source::<Feed>)
                .flat_map(serde_json::to_value)
                .collect()
        }
    }

    pub struct ComputeTags;

    impl EventHandler for ComputeTags {
        fn on_data_loaded(&self, srcs: &AggregatedSources) -> tera::Value {
            let mut hs: BTreeSet<Tag> = BTreeSet::new();
            for source in srcs.get("index").into_iter().flatten() {
                if let Some(fm) = source.front_matter() {
                    match fm.tags.as_ref() {
                        Some(tags) => {
                            hs.extend(tags.iter().map(|t| Tag::new(t)));
                        }
                        None => {
                            hs.insert(Tag::uncategorized());
                        }
                    }
                }
            }

            for src in srcs.get("feed").into_iter().flatten() {
                for feed in from_parsed_source::<Feed>(src) {
                    if feed.tags.is_empty() {
                        hs.insert(Tag::uncategorized());
                    } else {
                        hs.extend(feed.tags);
                    }
                }
            }

            serde_json::to_value(hs).unwrap()
        }
    }
}

pub mod csv {
    use files::MediaType;
    use page::model::record::Record;
    use parser::ParsedSource;

    mod reader {
        use libs::csv::Reader;
        use parser::ParsedSource;

        pub fn from_parsed_source(source: &ParsedSource) -> Reader<&[u8]> {
            from_string(source.data())
        }

        pub fn from_string(source: &str) -> Reader<&[u8]> {
            from_bytes(source.as_bytes())
        }

        pub fn from_bytes(source: &[u8]) -> Reader<&[u8]> {
            libs::csv::ReaderBuilder::new()
                .has_headers(true)
                .delimiter(b',')
                .double_quote(true)
                .from_reader(source)
        }
    }

    pub fn from_parsed_source<T>(source: &ParsedSource) -> Vec<T>
    where
        T: From<Record>,
    {
        let mut feed: Vec<T> = Vec::new();
        if source.media_type() == MediaType::Csv {
            let mut rdr = reader::from_parsed_source(source);

            for result in rdr.deserialize::<Record>() {
                if let Ok(record) = result {
                    feed.push(record.into());
                }
            }
        }

        feed
    }
}

////////////////////

pub fn bln_input_sort_by_date_published(
    name: &str,
    sources: &[ParsedSource],
    _sort_fn: Option<SortFn>,
) -> AggregatedSources {
    let sort_fn: SortFn = Box::new(|a, b| {
        let published_a = a
            .front_matter()
            .map(|f| f.published.as_ref().unwrap())
            .unwrap();
        let published_b = b
            .front_matter()
            .map(|f| f.published.as_ref().unwrap())
            .unwrap();

        published_b.cmp(&published_a)
    });

    bln_input_aggregate_all(name, sources, Some(sort_fn))
}

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
    bln_input_aggregate_all("feed", sources, sort_fn)
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

pub fn feed_item_to_parsed_source(
    feed: &Feed,
    specifier: &str,
    media_type: &MediaType,
) -> ParsedSource {
    ParsedSourceBuilder::new(specifier.to_string(), media_type.to_owned())
        .content(feed.to_string())
        .build()
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
        context.insert(
            "description",
            &front_matter.tags.as_ref().unwrap_or(&vec![]).join(","),
        );
        context.insert(
            "title",
            &front_matter.title.as_ref().unwrap_or(&"".to_string()),
        );
        if let Some(id) = front_matter.id.as_ref() {
            context.insert(
                "og_image_path",
                &format!("/static/pics/notes/{id}/article_image.png"),
            );
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
            let mut rdr = libs::csv::ReaderBuilder::new()
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
    let articles = srcs
        .iter()
        .map(to_article)
        .take(6)
        .collect::<Vec<Article>>();

    wrap_into_context("articles", &articles)
}

pub fn to_article(parsed_source: &ParsedSource) -> Article {
    Article::from_parsed_source(parsed_source.to_owned()).expect("Could not parse parsed_source!")
}

fn wrap_into_context<T: Serialize + ?Sized, S: Into<String>>(key: S, value: &T) -> tera::Context {
    let mut context = tera::Context::new();
    context.insert(key, value);
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
        Ok(self.iter().map(Feed::from).collect::<Vec<Feed>>())
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
                ..
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
            let description =
                markdown::string_to_html(&description, &markdown::MarkdownOptions::default());
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
