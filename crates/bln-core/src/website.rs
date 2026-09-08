//! Presentation settings carried by a website render operation, not content assembly.

use serde::Deserialize;
use serde::Serialize;
use url::Url;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebsiteConfig {
    /// Local theme directory, absolute or relative to the publishing project.
    /// No implicit registry lookup or download is performed.
    pub theme: Option<std::path::PathBuf>,
    pub url: Option<Url>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub profiles: WebsiteProfiles,
    pub giscus: Option<GiscusConfig>,
}

/// Public identifiers from the giscus setup page; never credentials or tokens.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GiscusConfig {
    pub repo: String,
    pub repo_id: String,
    pub category: String,
    pub category_id: String,
    /// Optional HTTPS stylesheet for the embedded widget. Defaults to giscus light.
    pub theme: Option<Url>,
}

impl GiscusConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        let parts = self.repo.split('/').collect::<Vec<_>>();
        if parts.len() != 2
            || parts.iter().any(|part| {
                part.is_empty()
                    || *part == "."
                    || *part == ".."
                    || !part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            })
        {
            return Err("giscus repo must be an owner/repository name");
        }
        if [&self.repo_id, &self.category_id].iter().any(|id| {
            id.is_empty()
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_=/+".contains(&byte))
        }) {
            return Err("giscus repository and category IDs must be nonempty GitHub node IDs");
        }
        if self.category.trim().is_empty() || self.category.chars().any(char::is_control) {
            return Err("giscus category must be nonempty and contain no control characters");
        }
        if let Some(theme) = &self.theme
            && (theme.scheme() != "https"
                || theme.host_str().is_none()
                || !theme.username().is_empty()
                || theme.password().is_some())
        {
            return Err("giscus theme must be an HTTPS URL without credentials");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebsiteProfiles {
    pub linkedin: Option<Url>,
    pub github: Option<Url>,
    pub twitter: Option<Url>,
    pub mastodon: Option<Url>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_public_giscus_identifiers_and_theme() {
        let mut config = GiscusConfig {
            repo: "owner/notebook".into(),
            repo_id: "R_123".into(),
            category: "Article comments".into(),
            category_id: "DIC_123".into(),
            theme: None,
        };
        assert_eq!(config.validate(), Ok(()));
        for repo in [
            "",
            "../notebook",
            "owner/repo/extra",
            "https://github.com/owner/repo",
        ] {
            config.repo = repo.into();
            assert!(config.validate().is_err());
        }
        config.repo = "owner/notebook".into();
        config.repo_id.clear();
        assert!(config.validate().is_err());
        config.repo_id = "R_123".into();
        config.theme = Some("javascript:alert(1)".parse().unwrap());
        assert!(config.validate().is_err());
        config.theme = Some("https://example.com/theme.css".parse().unwrap());
        assert_eq!(config.validate(), Ok(()));
    }
}
