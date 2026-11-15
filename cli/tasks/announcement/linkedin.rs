//! Explicit member or organization publishing; all inputs are bound to an approval.
pub(crate) mod connection;
mod http;
mod organization;
mod publish;
#[cfg(test)]
mod tests;
use super::*;

pub(crate) const API_VERSION: &str = "202609";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Target {
    pub member_urn: String,
    pub name: String,
    pub api_version: String,
    pub visibility: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization: Option<Organization>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Organization {
    pub urn: String,
    pub name: String,
}
impl Target {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        urn(&self.member_urn, "urn:li:person:")?;
        name(&self.name)?;
        if self.visibility != "PUBLIC" {
            bail!("invalid LinkedIn publishing target");
        }
        if let Some(organization) = &self.organization {
            organization::id(&organization.urn)?;
            name(&organization.name)?;
        }
        version(&self.api_version)
    }
    pub(crate) fn author_urn(&self) -> &str {
        self.organization
            .as_ref()
            .map_or(&self.member_urn, |organization| &organization.urn)
    }
    fn same_binding(&self, other: &Self) -> bool {
        self.member_urn == other.member_urn && self.author_urn() == other.author_urn()
    }
}
fn name(value: &str) -> Result<(), Error> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        bail!("invalid LinkedIn display name");
    }
    Ok(())
}
fn version(v: &str) -> Result<(), Error> {
    if v.len() != 6
        || !v.bytes().all(|b| b.is_ascii_digit())
        || !(1..=12).contains(&v[4..].parse::<u8>()?)
    {
        bail!("LinkedIn API version must be YYYYMM");
    }
    Ok(())
}
pub(super) fn urn(value: &str, prefix: &str) -> Result<(), Error> {
    let id = value
        .strip_prefix(prefix)
        .context("unexpected LinkedIn URN type")?;
    if id.is_empty()
        || id.len() > 256
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        bail!("invalid LinkedIn URN");
    }
    Ok(())
}
pub(super) fn post_url(urn: &str) -> Result<Url, Error> {
    let prefix = if urn.starts_with("urn:li:share:") {
        "urn:li:share:"
    } else {
        "urn:li:ugcPost:"
    };
    self::urn(urn, prefix)?;
    if !urn[prefix.len()..].bytes().all(|b| b.is_ascii_digit()) {
        bail!("invalid LinkedIn post ID");
    }
    Ok(Url::parse(&format!(
        "https://www.linkedin.com/feed/update/{urn}"
    ))?)
}
pub(crate) use publish::{deliver, reconcile};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Attempt {
    pub target: Target,
    pub image_urn: Option<String>,
    pub post_urn: Option<String>,
}
