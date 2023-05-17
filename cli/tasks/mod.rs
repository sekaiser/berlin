use core::fmt;
use core::fmt::Debug;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use crate::tasks::functions::bln_input_aggregate_by_category;
use crate::tasks::functions::bln_input_feed_aggregate_all;
use crate::tasks::functions::bln_parse_csv_aggregate_by_category;
use crate::tasks::functions::collect_articles;
use crate::tasks::functions::extract_front_matter;
use crate::tasks::functions::extract_tags;
use crate::tasks::functions::extract_tags_from_feed;
use crate::tasks::functions::inject_photo_data;
use crate::tasks::functions::parse_feed;
use crate::tasks::render::render_builder::RenderBuilder;
use crate::util::fs::load_files;
use berlin_core::anyhow::Context;
use berlin_core::MediaType;
use berlin_core::ParsedSource;
use berlin_core::{anyhow::Error, ModuleSpecifier};
use parser::CapturingParser;
use parser::Parser;

use crate::proc_state::ProcState;

use self::copy_static::CopyStatic;
use self::css::Css;
use self::functions::bln_input_aggregate_all;
use self::functions::collect_by_tag;

pub mod copy_static;
pub mod css;
pub mod functions;
pub mod model;
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

#[derive(Clone)]
pub enum Input<'a> {
    Files(&'a Vec<PathBuf>),
    Pattern(&'a str),
    PatternWithAggregate(&'a str, InputAggregate<'a>),
}

impl<'a, 'b> Input<'a> {
    pub fn load(
        &self,
        name: &str,
        base_path: &PathBuf,
        parser: &'a CapturingParser<'a>,
    ) -> Result<AggregatedSources, Error> {
        match self {
            Input::Files(vec) => {
                let sources = self.parse(vec, parser)?;
                Ok(Self::resolve_input_aggregate(None)(&name, &sources, None))
            }
            Input::Pattern(ref input_pattern) => {
                let sources = self.parse(&load_files(base_path, input_pattern), parser)?;
                Ok(Self::resolve_input_aggregate(None)(&name, &sources, None))
            }
            Input::PatternWithAggregate(ref input_pattern, aggregate_fn) => {
                let sources = self.parse(&load_files(base_path, input_pattern), parser)?;
                Ok(aggregate_fn(&name, &sources, None))
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

    fn resolve_input_aggregate(maybe_input_aggregate: Option<InputAggregate>) -> InputAggregate {
        match maybe_input_aggregate {
            Some(input_aggregate) => input_aggregate,
            None => &bln_input_aggregate_all,
        }
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
        let tasks: &[&dyn WatchableTask] = &[
            &RenderBuilder::new("index", "index.tera", "index.html")
                .input(&[
                    Input::Pattern("content/notes/*.md"),
                    Input::PatternWithAggregate("data/feed.csv", &bln_input_feed_aggregate_all),
                ])
                .template_vars(Aggregator::Merge(&[
                    Aggregate::Category("index", &collect_articles),
                    Aggregate::Category("feed", &parse_feed),
                    Aggregate::Categories(
                        "tags",
                        &[("index", &extract_tags), ("feed", &extract_tags_from_feed)],
                    ),
                ]))
                .add_to_context(&inject_photo_data)
                .add_to_context(&|| -> tera::Context {
                    let mut context = tera::Context::new();
                    let vec: Vec<String> = vec![];
                    context.insert("slides", &vec);
                    context
                })
                .build(),
            &RenderBuilder::new("tags", "tags/base.tera", "tags/[slug].html")
                .input(&[
                    Input::PatternWithAggregate(
                        "content/notes/*.md",
                        &bln_input_aggregate_by_category,
                    ),
                    Input::PatternWithAggregate(
                        "data/feed.csv",
                        &bln_parse_csv_aggregate_by_category,
                    ),
                ])
                .template_vars(Aggregator::Reduce(&collect_by_tag))
                .build(),
            &RenderBuilder::new("notes", "notes/[slug].tera", "notes/[slug].html")
                .input(&[Input::Pattern("content/notes/*.md")])
                .template_vars(Aggregator::None(&[("notes", &extract_front_matter)]))
                .build(),
            &RenderBuilder::new("notes_index", "notes.tera", "notes.html")
                .input(&[Input::Pattern("content/notes/*.md")])
                .template_vars(Aggregator::Merge(&[Aggregate::Category(
                    "notes_index",
                    &collect_articles,
                )]))
                .build(),
            &RenderBuilder::new("about", "about.tera", "about.html").build(),
            &RenderBuilder::new("garage", "garage.tera", "garage.html").build(),
            &RenderBuilder::new("feed", "feed.tera", "feed.html")
                .input(&[Input::Pattern("data/feed.csv")])
                .template_vars(Aggregator::Merge(&[Aggregate::Category(
                    "feed",
                    &parse_feed,
                )]))
                .build(),
            &RenderBuilder::new("photostream", "photostream.tera", "photostream.html")
                .add_to_context(&inject_photo_data)
                .build(),
            &Css {
                input_pattern: "styles.css".into(),
                output: "styles.css".into(),
            },
            &CopyStatic {
                output: "static/{file}".into(),
            },
        ];

        for task in tasks.iter() {
            let res = consumer(*task);

            if let Err(e) = res {
                eprintln!("Error while running task: {e}");
            }
        }
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
