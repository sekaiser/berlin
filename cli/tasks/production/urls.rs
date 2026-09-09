//! Site-relative URL resolution and versions of final, recursively prepared assets.
use anyhow::{Context, Error, bail};
use percent_encoding::percent_decode_str;
use url::Url;

use super::ProductionSite;

pub(super) struct LocalReference {
    pub path: String,
    pub url: Url,
}

impl ProductionSite<'_> {
    pub(super) fn page_url(&self, name: &str) -> Result<Url, Error> {
        let mut url = self.base.clone();
        if name != "index.html" {
            url.path_segments_mut()
                .map_err(|_| anyhow::anyhow!("invalid site base"))?
                .pop_if_empty()
                .extend(name.split('/'));
        }
        Ok(url)
    }

    pub(super) fn local_reference(
        &self,
        value: &str,
        owner: &str,
    ) -> Result<Option<LocalReference>, Error> {
        let value = value.trim();
        let page = self.page_url(owner)?;
        // An authored /path is site-root relative. Already-prefixed paths must
        // not gain the repository prefix a second time.
        let mut url = if value.starts_with('/')
            && !value.starts_with("//")
            && !value.starts_with(self.base.path())
            && value.split(['?', '#']).next() != Some(self.base.path().trim_end_matches('/'))
        {
            self.base.join(value.trim_start_matches('/'))?
        } else {
            page.join(value)?
        };
        match url.scheme() {
            "data" | "mailto" | "tel" => return Ok(None),
            "http" | "https" => {}
            _ => bail!("Unsafe URL in {owner}: {value}"),
        }
        if url.host_str().is_some_and(is_preview_host) || url.path().contains("/__berlin/") {
            bail!("Preview URL in {owner}: {value}");
        }
        if url.origin() != self.base.origin() {
            return Ok(None);
        }
        if url.path() == self.base.path().trim_end_matches('/') {
            url.set_path(self.base.path());
        }
        let path = url
            .path()
            .strip_prefix(self.base.path())
            .with_context(|| format!("URL escapes site base in {owner}: {value}"))?;
        let mut path = percent_decode_str(path).decode_utf8()?.into_owned();
        if path.is_empty() || path.ends_with('/') {
            path.push_str("index.html");
        }
        if !self.known.contains(&path) {
            bail!("Missing local destination in {owner}: {value} ({path})");
        }
        Ok(Some(LocalReference { path, url }))
    }

    pub(super) fn reference(&mut self, value: &str, owner: &str) -> Result<String, Error> {
        let Some(mut reference) = self.local_reference(value, owner)? else {
            return Ok(value.to_owned());
        };
        if let Some(fragment) = reference
            .url
            .fragment()
            .filter(|fragment| !fragment.is_empty())
        {
            let page = self.pages.get(&reference.path).map(|page| {
                page.redirect
                    .as_ref()
                    .and_then(|target| self.pages.get(target))
                    .unwrap_or(page)
            });
            let fragment = percent_decode_str(fragment).decode_utf8()?;
            if let Some(page) = page
                && !page.ids.contains(fragment.as_ref())
            {
                bail!("Missing fragment in {owner}: {}#{fragment}", reference.path);
            }
        }
        if value.trim().starts_with('#') {
            return Ok(value.to_owned());
        }
        if !self.pages.contains_key(&reference.path) {
            let hash = self.asset_hash(&reference.path)?;
            let query = reference
                .url
                .query_pairs()
                .filter(|(name, _)| name != "v")
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect::<Vec<_>>();
            reference.url.set_query(None);
            reference
                .url
                .query_pairs_mut()
                .extend_pairs(query)
                .append_pair("v", &hash);
        }
        Ok(reference.url.into())
    }
}

fn is_preview_host(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host.starts_with("127.")
        || host.starts_with("0.")
        || host == "[::1]"
        || host == "[::]"
}
