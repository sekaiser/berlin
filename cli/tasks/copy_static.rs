use files::ModuleSpecifier;
use libs::anyhow::Error;
use std::fmt;

use crate::{proc_state::ProcState, util::fs::consume_files};

use super::{Task, Watch, WatchableTask};

pub struct CopyStatic {
    pub output: String,
}

impl WatchableTask for CopyStatic {}

impl Task for CopyStatic {
    fn run(&self, ps: &ProcState) -> Result<i32, Error> {
        consume_files(ps.dir.static_file_path(), "**/*.*", |specifiers| {
            let static_file_path = ps.dir.static_file_path();
            let target_file_path = ps.dir.target_file_path();
            let prefix = static_file_path.to_string_lossy();
            for specifier in specifiers {
                let relative_path = specifier
                    .path()
                    .strip_prefix(&format!("{}/", prefix))
                    .unwrap();

                let output = target_file_path.join(self.output.replace("{file}", relative_path));
                std::fs::create_dir_all(&output.parent().unwrap()).unwrap();
                std::fs::copy(&specifier.path(), &output).unwrap();
            }
        });

        Ok(0)
    }
}

impl Watch for CopyStatic {
    fn on_change(&self, ps: &ProcState, specifier: &ModuleSpecifier) -> Result<i32, Error> {
        let static_file_path = ps.dir.static_file_path();
        let target_file_path = ps.dir.target_file_path();
        let prefix = static_file_path.to_string_lossy();
        if specifier.path().starts_with(prefix.as_ref()) {
            let relative_path = specifier
                .path()
                .strip_prefix(&format!("{}/", prefix))
                .unwrap();

            let output = target_file_path.join(self.output.replace("{file}", relative_path));
            std::fs::create_dir_all(&output.parent().unwrap()).unwrap();
            std::fs::copy(&specifier.path(), &output).unwrap();
        }

        Ok(0)
    }
}

impl<'a> fmt::Debug for CopyStatic {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("CopyStatic")
            .field("output", &self.output)
            .finish()
    }
}
