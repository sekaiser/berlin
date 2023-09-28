use std::sync::Arc;

use errors::error::generic_error;
use libs::anyhow::Error;
use libs::pandoc;
use libs::pandoc::{InputFormat, InputKind, OutputFormat, OutputKind, PandocOption, PandocOutput};

pub fn parse(source: Arc<str>) -> Result<String, Error> {
    let mut pandoc = pandoc::new();
    pandoc.set_input(InputKind::Pipe(source.as_ref().to_owned()));
    pandoc.add_option(PandocOption::Standalone);
    let filter = std::env::current_dir()?
        .parent()
        .unwrap()
        .join("filters")
        .join("test.lua");
    pandoc.add_option(PandocOption::LuaFilter(filter));
    pandoc.set_input_format(InputFormat::Org, vec![]);
    pandoc.set_output_format(OutputFormat::Other("gfm".to_string()), vec![]);
    pandoc.set_output(OutputKind::Pipe);

    match pandoc.execute() {
        Ok(PandocOutput::ToBuffer(data)) => Ok(data),
        Err(e) => Err(generic_error(e.to_string())),
        _ => Err(generic_error(
            "Only PandocOutput::ToBuffer(..) is supported!",
        )),
    }
}
