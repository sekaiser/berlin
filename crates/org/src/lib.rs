mod org;

use std::sync::Arc;

use errors::error::generic_error;
use files::{MediaType, ModuleSpecifier};
use libs::anyhow::Error;
use markdown::DefaultMarkdownParser;
pub use org::parse;
use parser::{ParsedSource, Parser};

#[derive(Default, Clone)]
pub struct DefaultOrgParser;

impl Parser for DefaultOrgParser {
    fn parse(
        &self,
        specifier: &ModuleSpecifier,
        source: Arc<str>,
        media_type: MediaType,
    ) -> Result<ParsedSource, Error> {
        match org::parse(source) {
            Ok(data) => DefaultMarkdownParser {}.parse(specifier, Arc::from(data), media_type),
            Err(e) => Err(generic_error(format!(
                "Cannot convert file {} to {}\nReason: {}",
                specifier,
                MediaType::Markdown,
                e.to_string()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let specifier = ModuleSpecifier::parse("file:///a/test.org").expect("bad specifier");
        let source = r#"
        #+TITLE: Test

        This is the example org-mode file used for the Denver Emacs org-mode
        introduction talk. Everything in this talk should work without any custom
        settings or installation with a reasonably recent Emacs version.

        Let's start with a headline, this is kind of like Markdown's # character:

        * This is an example headline

          Text can be put into the headline. You can create another headline at the same
          level with another * character
        "#;
        let parsed_source = DefaultOrgParser::default()
            .parse(&specifier, source.into(), MediaType::Org)
            .unwrap();
        print!("{:?}", parsed_source.data());
    }
}
