use std::{fmt, path::PathBuf};

use berlin_core::{anyhow::Error, resolve_path, resolve_url_or_path, ModuleSpecifier};

use crate::{proc_state::ProcState, util::path::specifier_to_file_path};

use super::{Input, InputLoader, Task, Watch, WatchableTask};

pub struct Css {
    pub input_pattern: String,
    pub output: String,
}

impl Css {
    fn run_internal(&self, ps: &ProcState, files_provider: InputLoader) -> Result<i32, Error> {
        if let Some(input) = files_provider.load_input()?.get(files_provider.name) {
            for parsed_source in input.iter() {
                let specifier = resolve_url_or_path(parsed_source.specifier())?;
                let path_buf = specifier_to_file_path(&specifier)?;
                let output = ps.dir.target_file_path().join("css").join(
                    self.output
                        .replace("{file}", path_buf.file_name().unwrap().to_str().unwrap()),
                );
                std::fs::create_dir_all(&output.parent().unwrap())?;
                std::fs::write(output, parsed_source.data())?;
            }
        }

        Ok(0)
    }
}

impl WatchableTask for Css {}

impl Task for Css {
    fn run(&self, ps: &ProcState) -> Result<i32, Error> {
        self.run_internal(
            ps,
            InputLoader {
                name: "css",
                base_path: &ps.dir.css_file_path(),
                inputs: &Input::Pattern(&self.input_pattern).into(),
                parser: &ps.parsed_source_cache.as_capturing_parser(),
            },
        )
    }
}

impl Watch for Css {
    fn on_change(&self, ps: &ProcState, specifier: &ModuleSpecifier) -> Result<i32, Error> {
        let prefix = format!("{}/", ps.dir.css_file_path().to_string_lossy());
        if let Some(changed_file) = specifier.path().strip_prefix(&prefix) {
            let re = fnmatch_regex::glob_to_regex(&self.input_pattern)?;

            if re.is_match(&changed_file) {
                let paths: Vec<PathBuf> = ps
                    .maybe_css_resolutions
                    .as_ref()
                    .map_or(Vec::new(), |files| {
                        files.get_root(PathBuf::from(specifier.path()))
                    });

                for p in paths.iter() {
                    ps.parsed_source_cache
                        .free(&resolve_path(&p.to_string_lossy())?);
                }

                let files_provider = InputLoader {
                    name: "css",
                    base_path: &ps.dir.css_file_path(),
                    inputs: &Input::Files(&paths).into(),
                    parser: &ps.parsed_source_cache.as_capturing_parser(),
                };

                self.run_internal(ps, files_provider)?;
            }
        }

        Ok(0)
    }
}

impl<'a> fmt::Debug for Css {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Css")
            .field("input_pattern", &self.input_pattern)
            .field("output", &self.output)
            .finish()
    }
}
