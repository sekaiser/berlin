//! Rewrite CSS through the existing CSS parser, including imports and custom values.
use anyhow::{Error, anyhow};
use lightningcss::{
    rules::CssRule,
    stylesheet::{ParserOptions, PrinterOptions, StyleSheet},
    values::url::Url,
    visitor::{Visit, VisitTypes, Visitor},
};

pub(super) fn rewrite(
    source: &str,
    reference: impl FnMut(&str) -> Result<String, Error>,
) -> Result<String, Error> {
    let mut sheet = StyleSheet::parse(source, ParserOptions::default())
        .map_err(|error| anyhow!(error.to_string()))?;
    sheet.visit(&mut References(reference))?;
    Ok(sheet
        .to_css(PrinterOptions {
            minify: true,
            ..PrinterOptions::default()
        })?
        .code)
}

struct References<F>(F);

impl<'i, F: FnMut(&str) -> Result<String, Error>> Visitor<'i> for References<F> {
    type Error = Error;

    fn visit_types(&self) -> VisitTypes {
        lightningcss::visit_types!(URLS | RULES)
    }

    fn visit_url(&mut self, url: &mut Url<'i>) -> Result<(), Error> {
        url.url = (self.0)(&url.url)?.into();
        Ok(())
    }

    fn visit_rule(&mut self, rule: &mut CssRule<'i>) -> Result<(), Error> {
        if let CssRule::Import(import) = rule {
            import.url = (self.0)(&import.url)?.into();
        }
        rule.visit_children(self)
    }
}
