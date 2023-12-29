use std::{
    collections::HashMap,
    ops::DerefMut,
    path::{Path, PathBuf},
    sync::Arc,
};

use files::{fs::load_files, MediaType, ModuleSpecifier};
use libs::anyhow::{Context, Error};
use parser::{ParsedSource, Parser};

use crate::parser::CapturingParser;

use super::types::{AggregatedSources, InputAggregate, SortFn};

pub enum Input<'a> {
    Vec(&'a Vec<PathBuf>),
    Files(&'a str),
    Aggregation(Vec<Box<Self>>, InputAggregate<'a>),
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
            Input::Vec(ref paths) => Ok((self.parse(paths, parser)?, &bln_input_aggregate_all)),
            Input::Files(ref pattern) => Ok((
                self.parse(&load_files(base_path, pattern), parser)?,
                &bln_input_aggregate_all,
            )),
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
