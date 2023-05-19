use crate::tasks::render::render_builder::RenderBuilder;
use crate::tasks::Aggregate;
use crate::tasks::AggregatedSources;
use crate::tasks::Aggregator;
use crate::tasks::Input;
use crate::tasks::InputLoader;
use crate::tasks::Inputs;
use crate::tasks::Task;
use crate::tasks::Watch;
use crate::tasks::WatchableTask;
use std::fmt;
use std::{path::PathBuf, process::exit};

use berlin_core::{resolve_path, ModuleSpecifier, ParsedSource};
use errors::anyhow::Error;

use crate::{args::ConfigFile, proc_state::ProcState};

mod render {
    use berlin_core::ParsedSource;

    use super::RenderStruct;

    pub(crate) type All<'a> = RenderStruct<'a, Vec<(String, ParsedSource, tera::Context)>>;
    pub(crate) type Single<'a> = RenderStruct<'a, tera::Context>;
    pub(crate) type Category<'a> = RenderStruct<'a, Vec<(String, tera::Context)>>;
}

mod reducer {
    use crate::tasks::{AggregatedSources, Aggregators, ScopedParsedSourceMapperFn};

    use super::RenderData;

    pub(crate) type SingleContext<'a> = RenderData<&'a Aggregators<'a>>;
    pub(crate) type PerScope<'a> = RenderData<&'a [ScopedParsedSourceMapperFn<'a>]>;
    pub(crate) type ReducerFn<'a> =
        RenderData<&'a dyn Fn(&AggregatedSources) -> Vec<(String, tera::Context)>>;
}

fn initialize_context(
    maybe_module_specifier: Option<ModuleSpecifier>,
) -> Result<tera::Context, Error> {
    let mut context = tera::Context::new();
    if let Some(config_specifier) = maybe_module_specifier {
        if let Ok(config_file) = ConfigFile::read(config_specifier.path()) {
            let site_config = config_file.to_site_config()?;
            context.insert("title", &site_config.title);
            context.insert("author", &site_config.author);
            context.insert("description", &site_config.description);
            context.insert("config_site_url", &site_config.url);

            let profiles_config = config_file.to_profiles_config()?;
            context.insert("linkedin", &profiles_config.linkedin);
            context.insert("github", &profiles_config.github);
            context.insert("twitter", &profiles_config.twitter);
            context.insert("og_image_path", "");
            context.insert("me", &profiles_config.linkedin);
        }
    }
    Ok(context)
}

pub(crate) struct OutputStruct {
    pub(crate) root_path: PathBuf,
    pub(crate) output: String,
}

impl OutputStruct {
    fn target_path(&self, with_slug: Option<(&str, &str)>) -> PathBuf {
        let output = match with_slug {
            Some((pattern, value)) => self.output.replace(pattern, value),
            None => self.output.to_string(),
        };

        self.root_path.join(output)
    }
}

trait RenderFn {
    fn render(&self) -> Vec<(PathBuf, String)>;
}

pub(crate) struct RenderData<T> {
    pub(crate) data: T,
    pub(crate) aggregated_sources: AggregatedSources,
    pub(crate) parent_context: tera::Context,
}

fn do_slugify(source: &ParsedSource) -> String {
    use slugify::slugify;

    let maybe_title = source
        .front_matter()
        .map(|fm| fm.title.as_ref().expect("Title not set in front_matter"));

    let value = match maybe_title {
        Some(title) => title.to_owned(),
        None => PathBuf::from(source.specifier().to_owned())
            .file_stem()
            .map(|n| n.to_string_lossy())
            .map(|n| n.to_string())
            .unwrap(),
    };

    slugify!(&value)
}

impl<'a> From<reducer::PerScope<'a>> for Vec<(String, ParsedSource, tera::Context)> {
    fn from(value: reducer::PerScope) -> Self {
        let reducer::PerScope {
            aggregated_sources,
            parent_context,
            data,
        } = value;
        let mut vec = Vec::new();
        for (category, processor_fn) in data.into_iter() {
            let key: &str = category.as_ref();
            if let Some(sources) = aggregated_sources.get(key) {
                for src in sources {
                    let (_, parsed_source, context) = processor_fn(src);
                    let mut ctx = parent_context.clone();
                    ctx.extend(context);
                    vec.push((do_slugify(&parsed_source), parsed_source, ctx));
                }
            }
        }
        vec
    }
}

impl<'a> From<reducer::ReducerFn<'a>> for Vec<(String, tera::Context)> {
    fn from(value: reducer::ReducerFn<'a>) -> Self {
        let reducer::ReducerFn {
            aggregated_sources,
            parent_context,
            data: reducer_fn,
        } = value;

        let to_pair = |v: &(String, tera::Context)| {
            let mut ctx = parent_context.clone();
            ctx.extend(v.1.to_owned());
            (v.0.to_owned(), ctx)
        };
        reducer_fn(&aggregated_sources)
            .iter()
            .map(to_pair)
            .collect::<Vec<(String, tera::Context)>>()
    }
}

impl<'a> From<reducer::SingleContext<'a>> for tera::Context {
    fn from(value: reducer::SingleContext<'a>) -> Self {
        let reducer::SingleContext {
            aggregated_sources,
            parent_context,
            data,
        } = value;

        let mut context = parent_context.clone();
        for processor in data.into_iter() {
            match processor {
                Aggregate::Category(key, process) => {
                    if let Some(input) = aggregated_sources.get(&key.to_string()) {
                        context.extend(process(input));
                    }
                }
                // Aggregate::Merge(new_key, processors) => {
                //     let mut vec: Vec<Value> = Vec::new();
                //     for p in processors.into_iter() {
                //         if let Some(input) = self.0.get(p.1 .0.as_str()) {
                //             if let Some(value) = p.1 .1(input).get(&p.0) {
                //                 if value.is_array() {
                //                     let mut value = value.to_owned();
                //                     let v = value.as_array_mut().expect("Not an array of Value!");
                //                     vec.append(v);
                //                 }
                //             }
                //         }
                //     }
                //     context.insert(new_key.to_string(), &vec);
                // }
                Aggregate::Categories(new_key, processors) => {
                    // TODO Use BTreeSet instead of Vec to remove duplicates
                    let mut values = Vec::new();
                    for p in processors.iter() {
                        let (key, processor_fn) = p;
                        if let Some(input) = aggregated_sources.get(&key.to_string()) {
                            values.append(&mut processor_fn(input));
                        }
                    }
                    context.insert(new_key.to_string(), &values);
                }
            }
        }
        context
    }
}
pub(crate) struct RenderStruct<'a, T> {
    pub(crate) ps: &'a ProcState,
    pub(crate) template_name: &'a str,
    pub(crate) output: OutputStruct,
    pub(crate) data: T,
}

impl<'a> RenderFn for render::Single<'a> {
    fn render(&self) -> Vec<(PathBuf, String)> {
        vec![(
            self.output.target_path(None),
            self.ps.render_with_context(self.template_name, &self.data),
        )]
    }
}

impl<'a> RenderFn for render::Category<'a> {
    fn render(&self) -> Vec<(PathBuf, String)> {
        let to_rendered_pair = |f: &(String, tera::Context)| {
            let (slug, context) = f;
            (
                self.output.target_path(Some(("[slug]", slug))),
                self.ps.render_with_context(&self.template_name, context),
            )
        };
        self.data.iter().map(to_rendered_pair).collect()
    }
}

impl<'a> RenderFn for render::All<'a> {
    fn render(&self) -> Vec<(PathBuf, String)> {
        let to_rendered_pair = |f: &(String, ParsedSource, tera::Context)| {
            let (slug, parsed_source, context) = f;
            (
                self.output.target_path(Some(("[slug]", slug))),
                self.ps.render_parsed_source_with_context(
                    &self.template_name,
                    parsed_source,
                    context,
                ),
            )
        };
        self.data.iter().map(to_rendered_pair).collect()
    }
}

pub struct Render<'a> {
    pub name: &'a str,
    pub inputs: Inputs<'a>,
    pub template: &'a str,
    pub maybe_aggregator: Option<Aggregator<'a>>,
    pub add_to_context: Vec<&'a dyn Fn() -> tera::Context>,
    pub output: &'a str,
}

impl<'a> Render<'a> {
    #[allow(dead_code)]
    pub fn builder() -> RenderBuilder {
        RenderBuilder::default()
    }

    fn to_render_all(
        &'a self,
        reducer: reducer::PerScope,
        output: OutputStruct,
        ps: &'a ProcState,
    ) -> render::All {
        let template_name = &self.template;
        let data = Vec::<(String, ParsedSource, tera::Context)>::from(reducer);
        render::All {
            ps,
            template_name,
            output,
            data,
        }
    }

    fn to_render_single(
        &'a self,
        reducer: reducer::SingleContext,
        output: OutputStruct,
        ps: &'a ProcState,
    ) -> render::Single {
        let template_name = &self.template;
        let data = tera::Context::from(reducer);
        render::Single {
            ps,
            template_name,
            output,
            data,
        }
    }

    fn to_render_category(
        &'a self,
        reducer: reducer::ReducerFn,
        output: OutputStruct,
        ps: &'a ProcState,
    ) -> render::Category {
        let template_name = &self.template;
        let data = Vec::<(String, tera::Context)>::from(reducer);
        render::Category {
            ps,
            template_name,
            output,
            data,
        }
    }

    fn run_internal(&self, ps: &ProcState) -> Result<i32, Error> {
        let Render {
            ref name,
            ref inputs,
            add_to_context: inject_to_context,
            maybe_aggregator,
            template: template_name,
            output,
        } = self;

        let base_path = &ps.dir.root_file_path();
        let parser = &ps.parsed_source_cache.as_capturing_parser();

        let mut context = initialize_context(ps.options.maybe_config_file_specifier())?;

        for enricher in inject_to_context {
            context.extend(enricher());
        }

        let output = OutputStruct {
            root_path: ps.dir.target_file_path(),
            output: output.to_string(),
        };

        let files = if inputs.is_empty() {
            render::Single {
                ps,
                template_name,
                data: context,
                output,
            }
            .render()
        } else {
            let files_provider = InputLoader {
                name,
                inputs,
                base_path,
                parser,
            };

            let input_aggregate = files_provider.load_input()?; // HashMap<String, Vec<ParsedSource>>

            if input_aggregate.is_empty() {
                eprintln!("Error! No input found");
                exit(1);
            } else {
                if let Some(aggregator) = maybe_aggregator {
                    match aggregator {
                        Aggregator::None(processors) => {
                            let reducer = reducer::PerScope {
                                aggregated_sources: input_aggregate,
                                parent_context: context,
                                data: processors,
                            };
                            self.to_render_all(reducer, output, ps).render()
                        }
                        Aggregator::Merge(processors) => {
                            let reducer = reducer::SingleContext {
                                data: processors,
                                aggregated_sources: input_aggregate,
                                parent_context: context,
                            };

                            self.to_render_single(reducer, output, ps).render()
                        }
                        Aggregator::Reduce(processor) => {
                            let reducer = reducer::ReducerFn {
                                aggregated_sources: input_aggregate,
                                parent_context: context,
                                data: processor,
                            };
                            self.to_render_category(reducer, output, ps).render()
                        }
                    }
                } else {
                    eprintln!("Input found, but it is not clear what to do with it!");
                    eprintln!("Make sure you use RenderBuilder.template_vars(...)!");
                    exit(1);
                }
            }
        };

        for f in files {
            std::fs::create_dir_all(&f.0.parent().unwrap())?;
            std::fs::write(&f.0, &f.1)?;
        }

        Ok(0)
    }
}

impl<'a> fmt::Debug for Render<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Render")
            .field("name", &self.name)
            .field("template", &self.template)
            .field("output", &self.output)
            .field("maybe_aggregator", &self.maybe_aggregator)
            .field("inject_to_context", &"Vec<&Fn() -> tera::Context>")
            .finish()
    }
}

impl<'a> WatchableTask for Render<'a> {}

impl<'a> Task for Render<'a> {
    fn run(&self, ps: &ProcState) -> Result<i32, Error> {
        self.run_internal(ps)
    }
}

impl<'a> Watch for Render<'a> {
    fn on_change(&self, ps: &ProcState, specifier: &ModuleSpecifier) -> Result<i32, Error> {
        let Render { inputs, .. } = self;

        let prefix = format!("{}/", ps.dir.root_file_path().to_string_lossy());
        if let Some(changed_file) = specifier.path().strip_prefix(&prefix) {
            for input in inputs.iter() {
                if match input {
                    Input::Pattern(ref input_pattern)
                    | Input::PatternWithAggregate(ref input_pattern, _) => {
                        let re = fnmatch_regex::glob_to_regex(input_pattern)?;
                        re.is_match(&changed_file)
                    }
                    Input::Files(paths) => paths.contains(&PathBuf::from(changed_file)),
                } {
                    if let Ok(file_path) = specifier.to_file_path() {
                        ps.parsed_source_cache
                            .free(&resolve_path(&file_path.to_string_lossy())?);
                        return self.run_internal(ps);
                    } else {
                        eprintln!("Invalid path!");
                        return Ok(1);
                    }
                }
            }
        }

        let prefix = format!("{}/", ps.dir.templates_file_path().to_string_lossy());
        if let Some(changed_file) = specifier.path().strip_prefix(&prefix) {
            let expr = self.template.clone();
            let re = fnmatch_regex::glob_to_regex(&expr)?;

            ps.hera.lock().full_reload()?;
            if re.is_match(&changed_file) {
                let path = specifier.to_file_path().ok().expect("Invalid path");
                ps.parsed_source_cache
                    .free(&resolve_path(&path.to_string_lossy())?);

                return self.run_internal(ps);
            }
        }
        Ok(0)
    }
}
