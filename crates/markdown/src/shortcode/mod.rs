//! Source-aware access to Berlin's supported Ox-Hugo shortcodes.

mod parser;

use parser::Shortcode;

use crate::Error;
use crate::Source;

pub(crate) fn parse(source: &Source<'_>) -> Result<Vec<Shortcode>, Error> {
    parser::parse(source.uri(), source.text()).map_err(|error| Error::InvalidShortcode {
        source_uri: source.uri().to_string(),
        message: error.to_string(),
    })
}
