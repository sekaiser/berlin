use std::{collections::HashMap, path::PathBuf};

use berlin_config::ConfigFile;
use files::{fs::consume_files, resolve_url_or_path, ModuleSpecifier};
use libs::anyhow::Error;
use libs::tera;
use parser::ParsedSource;

use self::{
    input::Input, output::Output, param::Param, template_name::TemplateName,
    types::AggregatedSources,
};

pub mod input;
pub mod output;
pub mod param;
pub mod template_name;
pub mod types;

pub trait EventHandler {
    fn on_data_loaded(&self, srcs: &AggregatedSources) -> tera::Value {
        tera::Value::Null
    }
}

pub enum Task<'a> {
    Render(
        &'a str,
        TemplateName,
        Output,
        Vec<Input<'a>>,
        Vec<Param<'a>>,
    ),
    Mount(Output),
    Css(&'a str, Output),
}

pub trait Executer {
    fn run(&self, ps: &ProcState) -> Result<i32, Error>;
}

pub trait Watch {
    fn on_change(&self, ps: &ProcState, specifier: &ModuleSpecifier) -> Result<i32, Error>;
}

impl<'a> Executer for Task<'a> {
    fn run(&self, ps: &ProcState) -> Result<i32, Error> {
        match self {
            Task::Render(task_name, template_name, ref output, inputs, params) => {
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
            Task::Mount(ref output) => {
                consume_files(ps.dir.static_file_path(), "**/*.*", |specifiers| {
                    let static_file_path = ps.dir.static_file_path();
                    let target_file_path = ps.dir.target_file_path();
                    let prefix = static_file_path.to_string_lossy();
                    for specifier in specifiers {
                        let relative_path = specifier
                            .path()
                            .strip_prefix(&format!("{}/", prefix))
                            .unwrap();

                        let path = target_file_path.join(output.replace("{file}", relative_path));

                        std::fs::create_dir_all(&path.parent().unwrap()).unwrap();
                        std::fs::copy(&specifier.path(), &path).unwrap();
                    }
                });
            }
            Task::Css(input_pattern, ref output) => {
                let input = Input::Files(input_pattern);
                let aggregated_sources = input.load(
                    "css",
                    &ps.dir.css_file_path(),
                    &ps.parsed_source_cache.as_capturing_parser(),
                )?;
                for parsed_source in aggregated_sources.values().flatten() {
                    let specifier = resolve_url_or_path(parsed_source.specifier())?;
                    let path_buf = files::to_file_path(&specifier)?;
                    let output = ps.dir.target_file_path().join("css").join(
                        output.replace("{file}", path_buf.file_name().unwrap().to_str().unwrap()),
                    );
                    std::fs::create_dir_all(&output.parent().unwrap())?;
                    std::fs::write(output, parsed_source.data())?;
                }
            }
        };

        Ok(0)
    }
}

pub fn do_slugify(source: &ParsedSource) -> String {
    use libs::slugify::slugify;

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

pub fn initialize_context(
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
