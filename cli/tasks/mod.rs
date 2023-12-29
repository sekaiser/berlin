use core::fmt;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use crate::tasks::functions::bln_input_feed_aggregate_all;
use crate::tasks::functions::inject_photo_data;
use crate::util;
use crate::util::fs::consume_files;
use crate::util::fs::load_files;
use berlin_core::parser::CapturingParser;
use berlin_core::parser::ToCapturingParser;
use berlin_core::task::Task;
use content::model::tag::Tag;
use files::resolve_url_or_path;
use files::MediaType;
use files::ModuleSpecifier;
use libs::anyhow::Context;
use libs::anyhow::Error;
use libs::serde_json;
use libs::tera;
use libs::tera::Value;
use parser::ParsedSource;
use parser::ParsedSourceBuilder;
use parser::Parser;
use templates::template::TemplateName;

use crate::proc_state::ProcState;

use self::functions::bln_input_aggregate_all;
use self::functions::bln_input_sort_by_date_published;
use self::functions::tags::compute_tagged_sources;
use self::functions::tags::Content;
use self::functions::task::Output;
use self::functions::util::ComputeTags;
use self::functions::util::EventHandler;
use self::functions::util::GetArticlesByKey;
use self::functions::util::GetFeedByKey;

use self::render::task::initialize_context;

pub mod copy_static;
pub mod css;
pub mod functions;
pub mod render;

impl<'a> fmt::Debug for Task<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Debug").finish()
    }
}

impl<'a> Watch for Task<'a> {
    fn on_change(&self, ps: &ProcState, specifier: &ModuleSpecifier) -> Result<i32, Error> {
        //self.execute(&|task| task.on_change(ps, specifier))
        Ok(0)
    }
}

impl<'a> WatchableTask for Task<'a> {}

impl<'a> From<Input<'a>> for Inputs<'a> {
    fn from(value: Input<'a>) -> Self {
        vec![value]
    }
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
        let tasks: &[Task] = &[
            Task::Render(
                "notes_index",
                TemplateName::new("notes.tera"),
                Output::new("notes.html"),
                vec![Input::Aggregation(
                    vec![Input::Files("content/notes/*.md").into()],
                    &bln_input_sort_by_date_published,
                )],
                vec![Param::Single2("articles", &GetArticlesByKey("notes_index"))],
            ),
            Task::Render(
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
            Task::Render(
                "tags",
                TemplateName::new("tags/base.tera"),
                Output::new("tags/[slug].html"),
                vec![Input::Aggregation(
                    vec![
                        Input::Files("content/notes/*.md").into(),
                        Input::Files("data/feed.csv").into(),
                    ],
                    &|name, srcs, _| {
                        let sources = compute_tagged_sources(srcs)
                            .drain()
                            .map(|mut sources_grouped_by_tag| {
                                let mut articles: Vec<serde_json::Value> = Vec::new();
                                let mut feed: Vec<serde_json::Value> = Vec::new();
                                for s in sources_grouped_by_tag.1.drain(..) {
                                    match s {
                                        Content::Article(a) => {
                                            articles.push(a);
                                        }
                                        Content::Feed(f) => {
                                            feed.push(f);
                                        }
                                    };
                                }

                                let tag = sources_grouped_by_tag.0.borrow();
                                let mut custom: HashMap<String, Value> = HashMap::new();
                                custom.insert(
                                    "articles".to_string(),
                                    serde_json::to_value(
                                        articles.iter().take(6).collect::<Vec<_>>(),
                                    )
                                    .unwrap(),
                                );
                                custom.insert("tag_name".to_string(), tag.to_string().into());
                                custom.insert(
                                    "feed".to_string(),
                                    serde_json::to_value(feed).unwrap(),
                                );
                                ParsedSourceBuilder::new(
                                    format!("file:///tags/{}.txt", tag),
                                    MediaType::JsonFeedEntry,
                                )
                                .front_matter(tag.into())
                                .custom(custom)
                                .content("".to_string())
                                .build()
                            })
                            .collect();

                        let mut aggregated_sources = HashMap::new();
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
            Task::Render(
                "notes",
                TemplateName::new("notes/[slug].tera"),
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
            Task::Render(
                "about",
                "about.tera".into(),
                "about.html".into(),
                vec![],
                vec![],
            ),
            Task::Render(
                "garage",
                "garage.tera".into(),
                "garage.html".into(),
                vec![],
                vec![],
            ),
            Task::Render(
                "photostream",
                "photostream.tera".into(),
                "photostream.html".into(),
                vec![],
                vec![Param::Static(&inject_photo_data)],
            ),
            Task::Css("styles.css".into(), "styles.css".into()),
            Task::Mount("static/{file}".into()),
        ];

        for task in tasks.iter() {
            let res = consumer(task as &dyn WatchableTask);

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

#[cfg(test)]
mod test {
    #![allow(warnings, unused)]
    use std::any::Any;

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

    #[test]
    fn test() {
        // pub trait IntoPickSource<T> {
        //     type Pick: Into<T>;

        //     fn pick(&self) -> Self::Pick;
        // }

        // pub trait AsIntoPickSource<T> {
        //     type PickSource: IntoPickSource<T>;

        //     fn as_into_pick_source(&self) -> &Self::PickSource;
        // }

        // impl<T, PS> AsIntoPickSource<T> for PS
        // where
        //     PS: IntoPickSource<T>,
        // {
        //     type PickSource = Self;

        //     fn as_into_pick_source(&self) -> &Self::PickSource {
        //         self
        //     }
        // }

        // pub trait PickSource {
        //     type Pick;

        //     fn pick(&self) -> Self::Pick;
        // }

        // pub trait AsPickSource {
        //     type PickSource: PickSource;

        //     fn as_pick_source(&self) -> &Self::PickSource;
        // }

        // impl<PS> AsPickSource for PS
        // where
        //     PS: PickSource,
        // {
        //     type PickSource = Self;

        //     fn as_pick_source(&self) -> &Self::PickSource {
        //         self
        //     }
        // }

        // impl<PS> PickSource for PS
        // where
        //     PS: Into<Tags> + Clone,
        // {
        //     type Pick = Tags;

        //     fn pick(&self) -> Self::Pick {
        //         <PS as Into<Tags>>::into(self.clone())
        //     }
        // }

        // #[derive(Clone)]
        // struct Wrapper<T>(T);

        // impl Into<Tags> for Wrapper<ParsedSource> {
        //     fn into(self) -> Tags {
        //         self.0
        //             .front_matter()
        //             .map(<&FrontMatter as Into<Tags>>::into)
        //             .filter(|t| !t.is_empty())
        //             .unwrap_or(Tags::uncategorized())
        //     }
        // }

        // // impl PickSource for Feed {
        // //     type Pick = Tags;

        // //     fn pick(&self) -> Self::Pick {
        // //         Tags::from(&self.tags).get_or_when_empty(Tags::uncategorized())
        // //     }
        // // }

        // let source = ParsedSourceBuilder::new("file:///abc.txt".into(), MediaType::Html)
        //     .front_matter(FrontMatter {
        //         title: Some("no title".into()),
        //         author: None,
        //         description: None,
        //         published: None,
        //         tags: Some(vec!["abc".into(), "def".into()]),
        //         id: None,
        //     })
        //     .build();

        // let vec: Tags = <&ParsedSource as Into<Tags>>::into(&source);

        // println!("{:?}", vec);

        // pub trait Table: QuerySource + AsQuery + Sized {
        //     type PrimaryKey: SelectableExpression<Self> + ValidGrouping<()>;
        //     type AllColumns: SelectableExpression<Self> + ValidGrouping<()>;

        //     /// Returns the primary key of this table.
        //     ///
        //     /// If the table has a composite primary key, this will be a tuple.
        //     fn primary_key(&self) -> Self::PrimaryKey;
        //     /// Returns a tuple of all columns belonging to this table.
        //     fn all_columns() -> Self::AllColumns;
        // }

        // impl<T, Selection> Collector<Selection> for T
        // where
        //     Selection: Selector,
        //     T: Table,
        //     T::Query: SelectDsl<Selection>,
        // {
        //     type Output = <T::Query as SelectDsl<Selection>>::Output;

        //     fn select(self, selection: Selection) -> Self::Output {
        //         self.as_query().select(selection)
        //     }
        // }
    }
}
