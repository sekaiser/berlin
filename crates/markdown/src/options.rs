//! Berlin's canonical Ox-Hugo-compatible Comrak configuration.

use comrak::Options;

use crate::front_matter::DELIMITER as FRONT_MATTER_DELIMITER;

pub(super) fn berlin_markdown_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.front_matter_delimiter = Some(FRONT_MATTER_DELIMITER.to_owned());
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tagfilter = true;
    options.render.r#unsafe = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.header_id_prefix = Some(String::new());
    options.extension.footnotes = true;
    options.extension.description_lists = true;
    options
}
