use crate::proc_state::ProcState;
use crate::tasks::{DefaultTask, WatchableTask};
use berlin_core::ModuleSpecifier;
use errors::anyhow::Error;

// pub trait Task: fmt::Display {
//     fn run(
//         &self,
//         ps: &ProcState,
//         parser: &CapturingParser,
//         bln_dir: &BerlinDir,
//     ) -> Result<i32, Error>;
// }

// impl fmt::Display for CopyStatic {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "CopyStatic {{ output: {} }}", self.output)
//     }
// }

// impl fmt::Display for Css {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         write!(
//             f,
//             "Css {{ input_pattern: {} output: {} }}",
//             self.input_pattern, self.output
//         )
//     }
// }

// impl fmt::Display for Render {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         write!(
//             f,
//             "Render {{ name: {}, input_pattern: {}, template: {}, output: {} }}",
//             self.name,
//             match &self.input_pattern {
//                 Some(p) => p,
//                 _ => "None",
//             },
//             self.template,
//             self.output
//         )
//     }
// }

pub struct CliMainSiteGenerator<'g>(&'g dyn WatchableTask, &'g ProcState);

impl<'g> CliMainSiteGenerator<'g> {
    pub fn run_tasks(&self) -> Result<i32, Error> {
        self.0.run(&self.1)
    }

    pub fn watch(&self, specifier: ModuleSpecifier) -> Result<i32, Error> {
        self.0.on_change(&self.1, &specifier)
    }
}

pub fn create_main_site_generator(ps: &ProcState) -> Result<CliMainSiteGenerator, Error> {
    Ok(CliMainSiteGenerator(&DefaultTask, &ps))
}
