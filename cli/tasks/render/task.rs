use std::path::PathBuf;

use files::ModuleSpecifier;
use libs::anyhow::Error;
use parser::ParsedSource;

use crate::args::ConfigFile;
use libs::tera;



// impl<'a> WatchableTask for Render<'a> {}

// impl<'a> Task for Render<'a> {
//     fn run(&self, ps: &ProcState) -> Result<i32, Error> {
//         self.run_internal(ps)
//     }
// }

// impl<'a> Watch for Render<'a> {
//     fn on_change(&self, ps: &ProcState, specifier: &ModuleSpecifier) -> Result<i32, Error> {
//         let Render { inputs, .. } = self;

//         let prefix = format!("{}/", ps.dir.root_file_path().to_string_lossy());
//         if let Some(changed_file) = specifier.path().strip_prefix(&prefix) {
//             // for input in inputs.iter() {
//             //     if match input {
//             //         Input::Pattern(ref input_pattern)
//             //         | Input::PatternWithAggregate(ref input_pattern, _) => {
//             //             let re = libs::fnmatch_regex::glob_to_regex(input_pattern)?;
//             //             re.is_match(&changed_file)
//             //         }
//             //         Input::Files(paths) => paths.contains(&PathBuf::from(changed_file)),
//             //     } {
//             //         if let Ok(file_path) = specifier.to_file_path() {
//             //             ps.parsed_source_cache
//             //                 .free(&resolve_path(&file_path.to_string_lossy())?);
//             //             return self.run_internal(ps);
//             //         } else {
//             //             eprintln!("Invalid path!");
//             //             return Ok(1);
//             //         }
//             //     }
//             // }
//         }

//         let prefix = format!("{}/", ps.dir.templates_file_path().to_string_lossy());
//         if let Some(changed_file) = specifier.path().strip_prefix(&prefix) {
//             let expr = self.template;
//             let re = libs::fnmatch_regex::glob_to_regex(&expr)?;

//             ps.hera.lock().full_reload()?;
//             if re.is_match(&changed_file) {
//                 let path = specifier.to_file_path().ok().expect("Invalid path");
//                 ps.parsed_source_cache
//                     .free(&resolve_path(&path.to_string_lossy())?);

//                 return self.run_internal(ps);
//             }
//         }
//         Ok(0)
//     }
// }
