//! Presentation settings carried by a website render operation, not content assembly.

use serde::Deserialize;
use serde::Serialize;
use url::Url;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebsiteConfig {
    pub url: Option<Url>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub profiles: WebsiteProfiles,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebsiteProfiles {
    pub linkedin: Option<Url>,
    pub github: Option<Url>,
    pub twitter: Option<Url>,
    pub mastodon: Option<Url>,
}
