//! Destination declarations are data. Evaluating a pipeline never deploys it.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeploymentTarget {
    /// An existing GitHub.com Pages site sourced from gh-pages at its root.
    #[serde(rename = "github_pages")]
    GitHubPages { repository: String },
}

impl DeploymentTarget {
    pub fn validate(&self) -> Result<(), &'static str> {
        let Self::GitHubPages { repository } = self;
        let parts = repository.split('/').collect::<Vec<_>>();
        if parts.len() != 2
            || parts.iter().any(|part| {
                part.is_empty()
                    || *part == "."
                    || *part == ".."
                    || part.ends_with(".git")
                    || !part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            })
        {
            return Err("GitHub Pages requires an owner/repository name without .git");
        }
        Ok(())
    }
}
