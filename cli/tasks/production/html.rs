//! Inspects and rewrites real HTML attributes, leaving prose, scripts and code alone.
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Error, bail};
use lol_html::{RewriteStrSettings, element, html_content::Element, rewrite_str};

use super::{ProductionSite, xml_escape};

#[derive(Default)]
pub(super) struct Page {
    pub ids: HashSet<String>,
    pub redirect: Option<String>,
    canonical: Option<String>,
    refresh: Option<String>,
    noindex: bool,
}

impl Page {
    pub fn read(root: &Path, name: &str) -> Result<Self, Error> {
        let source = fs::read_to_string(root.join(name))?;
        let mut page = Self::default();
        rewrite_str(
            &source,
            RewriteStrSettings::new().append_element_content_handler(element!("*", |element| {
                page.inspect(element)?;
                Ok(())
            })),
        )
        .with_context(|| format!("Unable to inspect HTML {name}"))?;
        Ok(page)
    }

    fn inspect(&mut self, element: &Element<'_, '_>) -> Result<(), Error> {
        if let Some(id) = attribute(element, "id") {
            self.ids.insert(id);
        }
        if let Some(id) = attribute(element, "name")
            && element.tag_name() == "a"
        {
            self.ids.insert(id);
        }
        match element.tag_name().as_str() {
            "base" if element.has_attribute("href") => {
                bail!("HTML base overrides are not supported by production preparation")
            }
            "link"
                if attribute(element, "rel").is_some_and(|rel| {
                    rel.split_ascii_whitespace()
                        .any(|value| value.eq_ignore_ascii_case("canonical"))
                }) =>
            {
                if self.canonical.is_some() {
                    bail!("Duplicate canonical URL");
                }
                self.canonical = attribute(element, "href");
            }
            "meta" => {
                let content = attribute(element, "content");
                match attribute(element, "name").as_deref() {
                    Some("berlin-redirect") => {
                        if self.redirect.is_some() {
                            bail!("Duplicate redirect marker");
                        }
                        self.redirect = content.clone();
                    }
                    Some("robots") => {
                        self.noindex |= content.as_ref().is_some_and(|value| {
                            value
                                .to_ascii_lowercase()
                                .split([',', ' '])
                                .any(|word| word == "noindex")
                        })
                    }
                    _ => {}
                }
                if attribute(element, "http-equiv")
                    .is_some_and(|value| value.eq_ignore_ascii_case("refresh"))
                {
                    if self.refresh.is_some() {
                        bail!("Duplicate refresh destination");
                    }
                    self.refresh = content;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl ProductionSite<'_> {
    pub(super) fn validate_pages(&self) -> Result<(), Error> {
        for (name, page) in &self.pages {
            if let Some(target) = &page.redirect {
                if !generated_page_path(name)
                    || !generated_page_path(target)
                    || name == target
                    || !self.pages.contains_key(target)
                {
                    bail!("Invalid redirect in {name}: {target}");
                }
                if self.pages[target].redirect.is_some() {
                    bail!("Redirect chain or cycle in {name}");
                }
                let refresh = page
                    .refresh
                    .as_deref()
                    .and_then(|value| value.strip_prefix("0;url="))
                    .with_context(|| format!("Wrong redirect destination in {name}"))?;
                if self.page_url(name)?.join(refresh)? != self.page_url(target)? {
                    bail!("Wrong redirect destination in {name}");
                }
            } else if page.noindex || page.refresh.is_some() {
                bail!("Preview-only markup or unmarked redirect in {name}");
            }
            let canonical = page
                .canonical
                .as_deref()
                .with_context(|| format!("Missing canonical URL in {name}"))?;
            let expected = self.page_url(page.redirect.as_deref().unwrap_or(name))?;
            // Canonicals must be absolute and exact; authored body links may be relative.
            if canonical != expected.as_str() {
                bail!("Wrong canonical URL in {name}: {canonical}");
            }
        }
        Ok(())
    }

    pub(super) fn rewrite_html(&mut self, name: &str, source: &str) -> Result<String, Error> {
        rewrite_str(
            source,
            RewriteStrSettings::new().append_element_content_handler(element!("*", |element| {
                for key in ["href", "src", "poster"] {
                    if let Some(value) = attribute(element, key) {
                        let value = self.reference(&value, name)?;
                        element.set_attribute(key, &xml_escape(&value))?;
                    }
                }
                if let Some(value) = attribute(element, "srcset") {
                    let value = srcset(&value, |value| self.reference(value, name))?;
                    element.set_attribute("srcset", &xml_escape(&value))?;
                }
                if element.tag_name() == "meta" {
                    let key = attribute(element, "property").or_else(|| attribute(element, "name"));
                    if matches!(key.as_deref(), Some("og:image" | "twitter:image"))
                        && let Some(value) = attribute(element, "content")
                    {
                        let value = self.reference(&value, name)?;
                        element.set_attribute("content", &xml_escape(&value))?;
                    }
                }
                Ok(())
            })),
        )
        .with_context(|| format!("Unable to prepare HTML {name}"))
    }
}

fn attribute(element: &Element<'_, '_>, name: &str) -> Option<String> {
    element
        .get_attribute(name)
        .map(|value| html_escape::decode_html_entities(&value).into_owned())
}

fn generated_page_path(path: &str) -> bool {
    path.ends_with(".html") && berlin_publication::validate_site_route(path).is_ok()
}

/// URLs end at whitespace, not at embedded commas (notably in data URLs).
/// Trailing commas separate candidates without descriptors.
fn srcset(
    source: &str,
    mut reference: impl FnMut(&str) -> Result<String, Error>,
) -> Result<String, Error> {
    let mut remaining = source;
    let mut candidates = Vec::new();
    while !remaining
        .trim_matches(|c: char| c.is_ascii_whitespace() || c == ',')
        .is_empty()
    {
        remaining = remaining.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        let end = remaining
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(remaining.len());
        let url = &remaining[..end];
        remaining = &remaining[end..];
        if url.ends_with(',') {
            candidates.push(reference(url.trim_end_matches(','))?);
        } else {
            let end = remaining.find(',').unwrap_or(remaining.len());
            let descriptor = remaining[..end].trim();
            remaining = remaining[end..].trim_start_matches(',');
            let mut candidate = reference(url)?;
            if !descriptor.is_empty() {
                candidate.push(' ');
                candidate.push_str(descriptor);
            }
            candidates.push(candidate);
        }
    }
    Ok(candidates.join(", "))
}
