//! Finalizes a staged website before installation or release sealing.
//!
//! This is a built-in, offline preparation step, not a command hook. Document
//! revisions still come from the live pipeline artifacts; no metadata is inferred
//! from an independently prepared directory. Only transaction-owned files change.

mod css;
mod html;
mod urls;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Error, bail};
use berlin_core::PipelinePlan;
use url::Url;

use super::release;
use html::Page;

pub(super) fn validate_plan(plan: &PipelinePlan) -> Result<(), Error> {
    let node = release::website_output(plan)?;
    let config = node
        .operation
        .website_config()
        .context("website renderer requires configuration")?;
    release::validate_url(
        config
            .url
            .as_ref()
            .context("production requires a website URL")?,
    )
}

pub(super) fn prepare(plan: &PipelinePlan, staging: &Path) -> Result<(), Error> {
    validate_plan(plan)?;
    let node = release::website_output(plan)?;
    let config = node
        .operation
        .website_config()
        .context("website renderer requires configuration")?;
    let root = staging.join(
        node.output_path
            .as_ref()
            .context("website has no output directory")?,
    );
    finish(&root, config.url.as_ref().context("website has no URL")?)
}

/// The caller owns this disposable directory. Failure never installs its contents.
fn finish(root: &Path, url: &Url) -> Result<(), Error> {
    release::validate_url(url)?;
    // Validate paths (including symlinks) before adding any support files.
    release::inventory(root)?;
    release::add_notices(root)?;
    let files = release::inventory(root)?;
    for file in &files {
        let path = file.path.to_ascii_lowercase();
        if path.ends_with(".org") || path.ends_with(".csv") || path.contains(".ds_store") {
            bail!("Source/private file in release: {}", file.path);
        }
        if path == "static/js/live.js" {
            bail!("Preview-only asset in release: {}", file.path);
        }
    }
    let pages = files
        .iter()
        .filter(|file| file.path.ends_with(".html"))
        .map(|file| Ok((file.path.clone(), Page::read(root, &file.path)?)))
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    if !pages.contains_key("index.html") {
        bail!("production needs index.html");
    }
    let base = Url::parse(&format!("{}/", url.as_str().trim_end_matches('/')))?;
    let mut site = ProductionSite {
        root,
        base,
        pages,
        known: files.into_iter().map(|file| file.path).collect(),
        hashes: HashMap::new(),
        processing: HashSet::new(),
    };
    site.validate_pages()?;
    site.write_discovery_files()?;
    site.known
        .extend(["sitemap.xml", "robots.txt", ".nojekyll"].map(str::to_owned));
    // Process every stylesheet, including unreferenced ones, in a stable order.
    let mut styles = site
        .known
        .iter()
        .filter(|name| name.ends_with(".css"))
        .cloned()
        .collect::<Vec<_>>();
    styles.sort();
    for name in styles {
        site.asset_hash(&name)?;
    }
    for name in site.pages.keys().cloned().collect::<Vec<_>>() {
        let source = fs::read_to_string(root.join(&name))?;
        let output = site.rewrite_html(&name, &source)?;
        fs::write(root.join(name), output)?;
    }
    Ok(())
}

struct ProductionSite<'a> {
    root: &'a Path,
    base: Url,
    known: HashSet<String>,
    pages: BTreeMap<String, Page>,
    hashes: HashMap<String, String>,
    processing: HashSet<String>,
}

impl ProductionSite<'_> {
    fn asset_hash(&mut self, name: &str) -> Result<String, Error> {
        if let Some(hash) = self.hashes.get(name) {
            return Ok(hash.clone());
        }
        if !self.processing.insert(name.to_owned()) {
            bail!("Cyclic asset references: {name}");
        }
        let path = self.root.join(name);
        if name.ends_with(".css") {
            let source = fs::read_to_string(&path)?;
            let output = css::rewrite(&source, |value| self.reference(value, name))
                .with_context(|| format!("Unable to prepare CSS {name}"))?;
            fs::write(&path, output)?;
        }
        let hash = release::digest(&fs::read(path)?)[..16].to_owned();
        self.processing.remove(name);
        self.hashes.insert(name.to_owned(), hash.clone());
        Ok(hash)
    }

    fn write_discovery_files(&self) -> Result<(), Error> {
        let mut sitemap = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
        );
        for (name, page) in &self.pages {
            if page.redirect.is_none() {
                sitemap.push_str(&format!(
                    "  <url><loc>{}</loc></url>\n",
                    xml_escape(self.page_url(name)?.as_str())
                ));
            }
        }
        sitemap.push_str("</urlset>\n");
        fs::write(self.root.join("sitemap.xml"), sitemap)?;
        fs::write(
            self.root.join("robots.txt"),
            format!(
                "User-agent: *\nAllow: /\nSitemap: {}\n",
                self.base.join("sitemap.xml")?
            ),
        )?;
        fs::write(self.root.join(".nojekyll"), "")?;
        Ok(())
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests;
