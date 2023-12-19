use core::fmt;
use core::fmt::Debug;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use crate::tasks::functions::bln_input_feed_aggregate_all;
use crate::tasks::functions::inject_photo_data;
use crate::util::fs::load_files;
use berlin_core::parser::CapturingParser;
use berlin_core::parser::ToCapturingParser;
use files::MediaType;
use files::ModuleSpecifier;
use libs::anyhow::Context;
use libs::anyhow::Error;
use libs::serde_json;
use libs::tera;
use libs::tera::Value;
use page::model::article::Article;
use page::model::feed::Feed;
use page::model::tag::Tag;
use parser::FrontMatter;
use parser::ParsedSource;
use parser::ParsedSourceBuilder;
use parser::Parser;
use templates::template::TemplateName;

use crate::proc_state::ProcState;

use self::copy_static::CopyStatic;
use self::css::Css;
use self::functions::bln_input_aggregate_all;
use self::functions::bln_input_sort_by_date_published;
use self::functions::csv::from_parsed_source;
use self::functions::feed_item_to_parsed_source;
use self::functions::task::Output;
use self::functions::to_article;
use self::functions::util::ComputeTags;
use self::functions::util::EventHandler;
use self::functions::util::GetArticlesByKey;
use self::functions::util::GetFeedByKey;

use self::render::task::do_slugify;
use self::render::task::initialize_context;

pub mod copy_static;
pub mod css;
pub mod functions;
pub mod render;

pub type AggregatedSources = HashMap<String, Vec<ParsedSource>>;

pub type SortFn = Box<dyn Fn(&ParsedSource, &ParsedSource) -> std::cmp::Ordering>;

pub type InputAggregate<'a> =
    &'a dyn Fn(&str, &[ParsedSource], Option<SortFn>) -> AggregatedSources;

pub type Map<T, U> = dyn Fn(&T) -> U;

pub type ParsedSourcesMapperFn<T> = Map<Vec<ParsedSource>, T>;
pub type ScopedParsedSourcesMapperFn<'a> = (&'a str, &'a ParsedSourcesMapperFn<Vec<tera::Value>>);
pub type TemplateVarsAggregate<'a> = &'a ParsedSourcesMapperFn<tera::Context>;

pub type ParsedSourceMapperFn = Map<ParsedSource, (String, ParsedSource, tera::Context)>;
pub type ScopedParsedSourceMapperFn<'a> = (&'a str, &'a ParsedSourceMapperFn);

pub enum Aggregate<'a> {
    Category(&'a str, TemplateVarsAggregate<'a>),
    // Merge(&'a str, &'a [(&'a str, NamedTemplateVarsAggregate<'a>)]),
    Categories(&'a str, &'a [ScopedParsedSourcesMapperFn<'a>]),
}

impl<'a> Debug for Aggregate<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            Aggregate::Category(key, _) => f
                .debug_tuple("Category")
                .field(key)
                .field(&"Fn(&Vec<ParsedSource>) -> tera::Context")
                .finish(),
            Aggregate::Categories(key, _) => f
                .debug_tuple("Categories")
                .field(key)
                .field(&"Vec<(&str, Map<ParsedSource, (String, ParsedSource, tera::Context))>")
                .finish(),
        }
    }
}

pub enum Aggregator<'a> {
    // Create a context per input source
    None(&'a [ScopedParsedSourceMapperFn<'a>]),
    Merge(&'a [Aggregate<'a>]),
    Reduce(&'a Map<AggregatedSources, Vec<(String, tera::Context)>>),
}

impl<'a> Debug for Aggregator<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            Aggregator::None(_) => f
                .debug_tuple("None")
                .field(&"Fn(&Vec<ParsedSource>) -> Vec<tera::Value>")
                .finish(),
            Aggregator::Merge(vec) => f.debug_tuple("Merge").field(vec).finish(),
            Aggregator::Reduce(_) => f
                .debug_tuple("Reduce")
                .field(&"Map<AggregatedSources, Vec<(String, tera::Context)>>")
                .finish(),
        }
    }
}

pub type Aggregators<'a> = &'a [Aggregate<'a>];

pub enum ETask<'a> {
    Render(
        &'a str,
        TemplateName,
        Output,
        Vec<Input<'a>>,
        Vec<Param<'a>>,
    ),
}

impl<'a> fmt::Debug for ETask<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Debug").finish()
    }
}

impl<'a> Watch for ETask<'a> {
    fn on_change(&self, ps: &ProcState, specifier: &ModuleSpecifier) -> Result<i32, Error> {
        //self.execute(&|task| task.on_change(ps, specifier))
        Ok(0)
    }
}
impl<'a> WatchableTask for ETask<'a> {}

impl<'a> Task for ETask<'a> {
    fn run(&self, ps: &ProcState) -> Result<i32, Error> {
        match self {
            ETask::Render(task_name, template_name, ref output, inputs, params) => {
                let base_path = &ps.dir.target_file_path();
                let parser = &ps.parsed_source_cache.as_capturing_parser();
                let mut context = initialize_context(ps.options.maybe_config_file_specifier())?;

                if inputs.is_empty() {
                    for param in params.iter() {
                        if let Param::Static(provider) = param {
                            context.extend(provider());
                        }
                    }

                    let out = base_path.join(output.as_str());
                    let data = &ps.render_with_context(template_name.as_ref(), &context);
                    std::fs::create_dir_all(&out.parent().unwrap())?;
                    std::fs::write(&out, &data)?;

                    return Ok(0);
                }

                let mut aggregate = HashMap::new();
                for input in inputs.iter() {
                    let aggregated_sources =
                        input.load(task_name, &ps.dir.root_file_path(), parser)?;
                    for (key, ref mut parsed_sources_mut) in aggregated_sources {
                        aggregate
                            .entry(key)
                            .or_insert(Vec::new())
                            .append(parsed_sources_mut);
                    }
                }

                let mut inject_to_source = Vec::new();
                for param in params.iter() {
                    match param {
                        Param::Static(context_provider) => context.extend(context_provider()),
                        Param::Single(key, provider) => {
                            context.insert(*key, &provider(&aggregate));
                        }
                        Param::Multiple(mappings) => {
                            for (key, provider) in mappings.iter() {
                                context.insert(*key, &provider(&aggregate));
                            }
                        }
                        Param::Custom(provider) => {
                            context.extend(provider(&aggregate));
                        }
                        Param::Bind(provider) => inject_to_source.push(provider),
                        Param::Single2(key, handler) => {
                            context.insert(*key, &handler.on_data_loaded(&aggregate))
                        }
                        _ => {}
                    }
                }

                if output.contains("[slug]") {
                    let (mut outs, mut data, mut ctxs): (
                        Vec<PathBuf>,
                        Vec<ParsedSource>,
                        Vec<tera::Context>,
                    ) = (Vec::new(), Vec::new(), Vec::new());

                    if let Some(sources) = aggregate.get(&task_name.to_string()) {
                        for source in sources {
                            outs.push(
                                base_path.join(output.replace("[slug]", &do_slugify(&source))),
                            );
                            data.push(source.to_owned());

                            let mut child_context = context.clone();
                            for provider in inject_to_source.iter() {
                                child_context.extend(provider(source));
                            }
                            ctxs.push(child_context);
                        }
                    }

                    for n in 0..outs.len() {
                        let data = &ps.render_parsed_source_with_context(
                            template_name.as_ref(),
                            data.get(n).unwrap(),
                            &ctxs.get(n).unwrap(),
                        );
                        std::fs::create_dir_all(&outs.get(n).unwrap().parent().unwrap())?;
                        std::fs::write(&outs.get(n).unwrap(), &data)?;
                    }
                } else {
                    let out = base_path.join(output.as_str());
                    let data = &ps.render_with_context(template_name.as_ref(), &context);

                    std::fs::create_dir_all(&out.parent().unwrap())?;
                    std::fs::write(&out, &data)?;
                };
            }
            _ => {}
        };

        Ok(0)
    }
}

pub enum Input<'a> {
    Vec(&'a Vec<PathBuf>),
    Files(&'a str),
    Aggregation(Vec<Box<Self>>, InputAggregate<'a>),
}

#[allow(unused_variables)]
pub enum Param<'a> {
    Single2(&'a str, &'a dyn EventHandler),
    Static(&'static dyn Fn() -> tera::Context),
    Single(&'a str, &'static dyn Fn(&AggregatedSources) -> tera::Value),
    Multiple(Vec<(&'a str, &'static dyn Fn(&AggregatedSources) -> tera::Value)>),
    Custom(&'static dyn Fn(&AggregatedSources) -> tera::Context),
    Bind(&'static dyn Fn(&ParsedSource) -> tera::Context),
}

impl<'a> Input<'a> {
    pub fn load(
        &self,
        name: &str,
        base_path: &PathBuf,
        parser: &'a CapturingParser<'a>,
    ) -> Result<AggregatedSources, Error> {
        let (sources, fun) = self.process(name, self, base_path, parser)?;
        let aggregated_sources = fun(name, &sources, None);
        Ok(aggregated_sources)
    }

    fn process(
        &self,
        name: &'a str,
        input: &'a Input,
        base_path: &PathBuf,
        parser: &'a CapturingParser,
    ) -> Result<(Vec<ParsedSource>, InputAggregate<'a>), Error> {
        match input {
            Input::Vec(ref paths) => {
                let sources = self.parse(paths, parser)?;
                Ok((sources, &bln_input_aggregate_all))
            }
            Input::Files(ref pattern) => {
                let sources = self.parse(&load_files(base_path, pattern), parser)?;
                Ok((sources, &bln_input_aggregate_all))
            }
            Input::Aggregation(inputs, fun) => {
                if inputs.len() == 1 {
                    let (sources, _) =
                        self.process(name, inputs.first().unwrap().as_ref(), base_path, parser)?;
                    Ok((sources, fun))
                } else {
                    let mut parsed_sources = Vec::new();
                    for input in inputs.iter() {
                        let (ref sources, input_fun) =
                            self.process(name, input.as_ref(), base_path, parser)?;
                        for (_, ref mut srcs) in input_fun(name, sources, None) {
                            parsed_sources.append(srcs);
                        }
                    }

                    Ok((parsed_sources, fun))
                }
            }
        }
    }

    fn parse(
        &self,
        paths: &Vec<PathBuf>,
        parser: &CapturingParser,
    ) -> Result<Vec<ParsedSource>, Error> {
        let mut sources = Vec::new();

        for path in paths {
            let specifier = ModuleSpecifier::from_file_path(path).expect("Invalid path.");
            let content = std::fs::read_to_string(specifier.path())
                .context(format!("Unable to read file {:?}", &specifier))?;
            let media_type = MediaType::from(Path::new(specifier.path()));
            sources.push(parser.parse(&specifier, Arc::from(content), media_type)?);
        }

        Ok(sources)
    }
}

pub type Inputs<'a> = Vec<Input<'a>>;

impl<'a> From<Input<'a>> for Inputs<'a> {
    fn from(value: Input<'a>) -> Self {
        vec![value]
    }
}

pub struct InputLoader<'a> {
    pub name: &'a str,
    pub base_path: &'a PathBuf,
    pub inputs: &'a Inputs<'a>,
    pub parser: &'a CapturingParser<'a>,
}

impl<'a> InputLoader<'a> {
    pub fn load_input(&self) -> Result<AggregatedSources, Error> {
        let InputLoader {
            inputs,
            name,
            base_path,
            parser,
        } = self;
        let mut aggregate = HashMap::new();
        for input in inputs.iter() {
            let aggregated_sources = input.load(name, base_path, parser)?;
            for (key, ref mut parsed_sources_mut) in aggregated_sources {
                aggregate
                    .entry(key)
                    .or_insert(Vec::new())
                    .append(parsed_sources_mut);
            }
        }

        Ok(aggregate)
    }
}

pub trait Task {
    fn run(&self, ps: &ProcState) -> Result<i32, Error>;
}

pub trait Watch {
    fn on_change(&self, ps: &ProcState, specifier: &ModuleSpecifier) -> Result<i32, Error>;
}

pub trait WatchableTask: Task + Watch + fmt::Debug {}

pub trait Writer {}

impl WatchableTask for DefaultTask {}

pub struct DefaultTask;

impl DefaultTask {
    fn execute<'a>(
        &self,
        consumer: &dyn Fn(&dyn WatchableTask) -> Result<i32, Error>,
    ) -> Result<i32, Error> {
        let tasks: &[ETask] = &[
            ETask::Render(
                "notes_index",
                TemplateName::new("notes.tera"),
                Output::new("notes.html"),
                vec![Input::Aggregation(
                    vec![Input::Files("content/notes/*.md").into()],
                    &bln_input_sort_by_date_published,
                )],
                vec![Param::Single2("articles", &GetArticlesByKey("notes_index"))],
            ),
            ETask::Render(
                "index",
                TemplateName::new("index.tera"),
                Output::new("index.html"),
                vec![
                    Input::Aggregation(
                        vec![Input::Files("content/notes/*.md").into()],
                        &bln_input_sort_by_date_published,
                    ),
                    Input::Aggregation(
                        vec![Input::Files("data/feed.csv").into()],
                        &bln_input_feed_aggregate_all,
                    ),
                ],
                vec![
                    Param::Single2("articles", &GetArticlesByKey("index")),
                    Param::Single2("feed", &GetFeedByKey("feed")),
                    Param::Single2("tags", &ComputeTags),
                    Param::Static(&inject_photo_data),
                    Param::Static(&|| -> tera::Context {
                        let mut context = tera::Context::new();
                        let vec: Vec<String> = vec![];
                        context.insert("slides", &vec);
                        context
                    }),
                ],
            ),
            ETask::Render(
                "tags",
                TemplateName::new("tags/base.tera"),
                Output::new("tags/[slug].html"),
                vec![Input::Aggregation(
                    vec![
                        Input::Files("content/notes/*.md").into(),
                        Input::Files("data/feed.csv").into(),
                    ],
                    &|name, srcs, _| {
                        let mut tags = HashMap::new();

                        for src in srcs.iter() {
                            let specifier = src.specifier();
                            let media_type = MediaType::JsonFeedEntry;

                            match src.media_type() {
                                MediaType::Html => {
                                    if let Some(fm) = src.front_matter() {
                                        match fm.tags.as_ref() {
                                            Some(fm_tags) => {
                                                fm_tags.iter().for_each(|t| {
                                                    tags.entry(t.to_string())
                                                        .or_insert(Vec::new())
                                                        .push(src.clone())
                                                });
                                            }
                                            None => tags
                                                .entry("uncategorized".to_string())
                                                .or_insert(Vec::new())
                                                .push(src.clone()),
                                        }
                                    }
                                }
                                MediaType::Csv => {
                                    for feed in from_parsed_source::<Feed>(src) {
                                        if feed.tags.is_empty() {
                                            tags.entry("uncategorized".to_string())
                                                .or_insert(Vec::new())
                                                .push(feed_item_to_parsed_source(
                                                    &feed,
                                                    specifier,
                                                    &media_type,
                                                ));
                                        } else {
                                            feed.tags.iter().for_each(|t| {
                                                tags.entry(t.name.clone())
                                                    .or_insert(Vec::new())
                                                    .push(feed_item_to_parsed_source(
                                                        &feed,
                                                        specifier,
                                                        &media_type,
                                                    ))
                                            });
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }

                        let mut aggregated_sources = HashMap::new();

                        let sources = tags
                            .iter()
                            .map(|sources_grouped_by_tag| {
                                let mut articles: Vec<ParsedSource> = Vec::new();
                                let mut feed: Vec<ParsedSource> = Vec::new();

                                for s in sources_grouped_by_tag.1 {
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

                                let tag = sources_grouped_by_tag.0;
                                let mut custom: HashMap<String, Value> = HashMap::new();
                                custom.insert(
                                    "articles".to_string(),
                                    serde_json::to_value(
                                        articles
                                            .iter()
                                            .map(to_article)
                                            .take(6)
                                            .collect::<Vec<Article>>(),
                                    )
                                    .unwrap(),
                                );
                                custom.insert(
                                    "tag_name".to_string(),
                                    serde_json::to_value(tag).unwrap(),
                                );
                                custom.insert(
                                    "feed".to_string(),
                                    serde_json::to_value(
                                        feed.iter().map(Feed::from).collect::<Vec<Feed>>(),
                                    )
                                    .unwrap(),
                                );
                                let fm = FrontMatter {
                                    title: Some(tag.clone()),
                                    author: None,
                                    description: None,
                                    published: None,
                                    tags: None,
                                    id: None,
                                };
                                ParsedSourceBuilder::new(
                                    format!("file:///tags/{}.txt", tag),
                                    MediaType::JsonFeedEntry,
                                )
                                .front_matter(fm)
                                .custom(custom)
                                .content("".to_string())
                                .build()
                            })
                            .collect();

                        aggregated_sources.insert(name.to_string(), sources);

                        aggregated_sources
                    },
                )],
                vec![Param::Bind(&|src| {
                    let mut ctx = tera::Context::new();

                    if let Some(data) = src.custom() {
                        ctx.insert("tag_name", data.get("tag_name").unwrap());
                        ctx.insert("feed", data.get("feed").unwrap());
                        ctx.insert("articles", data.get("articles").unwrap());
                    }
                    ctx
                })],
            ),
            ETask::Render(
                "notes",
                TemplateName::new("notes/[slug].tera"), // should be notes/base.tera
                Output::new("notes/[slug].html"),
                vec![Input::Files("content/notes/*.md")],
                vec![Param::Bind(&|src| {
                    let mut context = tera::Context::new();
                    if let Some(front_matter) = src.front_matter() {
                        for x in front_matter.get_fields().into_iter() {
                            if let (k, Some(val)) = x {
                                let key = format!("page_{k}");
                                match k {
                                    "tags" => {
                                        if let Some(tags) = val.downcast_ref::<Vec<String>>() {
                                            context.insert(
                                                key,
                                                &tags
                                                    .iter()
                                                    .map(|t| Tag::new(t))
                                                    .collect::<Vec<_>>(),
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
                    context
                })],
            ),
            ETask::Render(
                "about",
                "about.tera".into(),
                "about.html".into(),
                vec![],
                vec![],
            ),
            ETask::Render(
                "garage",
                "garage.tera".into(),
                "garage.html".into(),
                vec![],
                vec![],
            ),
        ];

        for task in tasks.iter() {
            let res = consumer(task as &dyn WatchableTask);

            if let Err(e) = res {
                eprintln!("Error while running task: {e}");
            }
        }

        //let tasks: &[&dyn WatchableTask] = &[
        // &RenderBuilder::new("index", "index.tera", "index.html")
        //     .input(&[
        //         Input::Aggregation(
        //             Input::Files("content/notes/*.md").into(),
        //             &bln_input_sort_by_date_published,
        //         ),
        //         Input::Aggregation(
        //             Box::new(Input::Files("data/feed.csv")),
        //             &bln_input_feed_aggregate_all,
        //         ),
        //     ])
        //     .template_vars(Aggregator::Merge(&[
        //         Aggregate::Category("index", &collect_articles),
        //         Aggregate::Category("feed", &parse_feed),
        //         Aggregate::Categories(
        //             "tags",
        //             &[("index", &extract_tags), ("feed", &extract_tags_from_feed)],
        //         ),
        //     ]))
        //     .add_to_context(&inject_photo_data)
        //     .add_to_context(&|| -> tera::Context {
        //         let mut context = tera::Context::new();
        //         let vec: Vec<String> = vec![];
        //         context.insert("slides", &vec);
        //         context
        //     })
        //     .build(),
        // &RenderBuilder::new("tags", "tags/base.tera", "tags/[slug].html")
        //     .input(&[
        //         Input::Aggregation(
        //             Box::new(Input::Files("content/notes/*.md")),
        //             &bln_input_aggregate_by_category,
        //         ),
        //         Input::Aggregation(
        //             Box::new(Input::Files("data/feed.csv")),
        //             &bln_parse_csv_aggregate_by_category,
        //         ),
        //     ])
        //     .template_vars(Aggregator::Reduce(&collect_by_tag))
        //     .build(),
        // &RenderBuilder::new("notes", "notes/[slug].tera", "notes/[slug].html")
        //     .input(&[Input::Files("content/notes/*.md")])
        //     .template_vars(Aggregator::None(&[("notes", &extract_front_matter)]))
        //     .build(),
        //&task_notes,
        // &RenderBuilder::new("about", "about.tera", "about.html").build(),
        // &RenderBuilder::new("garage", "garage.tera", "garage.html").build(),
        // &RenderBuilder::new("feed", "feed.tera", "feed.html")
        //     .input(&[Input::Files("data/feed.csv")])
        //     .template_vars(Aggregator::Merge(&[Aggregate::Category(
        //         "feed",
        //         &parse_feed,
        //     )]))
        //     .build(),
        // &RenderBuilder::new("photostream", "photostream.tera", "photostream.html")
        //     .add_to_context(&inject_photo_data)
        //     .build(),
        // &Css {
        //     input_pattern: "styles.css".into(),
        //     output: "styles.css".into(),
        // },
        // &CopyStatic {
        //     output: "static/{file}".into(),
        // },
        //];

        // for task in tasks.iter() {
        //     let res = consumer(*task);

        //     if let Err(e) = res {
        //         eprintln!("Error while running task: {e}");
        //     }
        // }
        Ok(0)
    }
}

impl fmt::Debug for DefaultTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DefaultTask").finish()
    }
}

impl Watch for DefaultTask {
    fn on_change(&self, ps: &ProcState, specifier: &ModuleSpecifier) -> Result<i32, Error> {
        self.execute(&|task| task.on_change(ps, specifier))
    }
}

impl Task for DefaultTask {
    fn run(&self, ps: &ProcState) -> Result<i32, Error> {
        self.execute(&|task| task.run(ps))
    }
}

#[cfg(test)]
mod test {
    #![allow(warnings, unused)]
    use super::*;

    #[test]
    fn should_render_notes_index() {
        // let task = ETask::Render(
        //     "notes_index",
        //     TemplateName::new("notes.tera"),
        //     "notes.html",
        //     vec![Input::Aggregation(
        //         Input::Files("content/notes/*.md").into(),
        //         &bln_input_sort_by_date_published,
        //     )],
        //     vec![], // vec![Param::Single(
        //             //     "articles",
        //             //     vec![("notes_index", &to_articles)],
        //             // )],
        // );

        //task.run(ps);
    }
}
