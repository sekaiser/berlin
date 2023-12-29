use std::fmt::Display;

use errors::error::generic_error;
use libs::serde_json;
use serde::Serialize;

#[derive(Serialize, Default)]
pub struct Picture<'a> {
    pub title: &'a str,
    pub src: &'a str,
    pub srcset: &'a str,
    pub target: &'a str,
}

impl<'a> Display for Picture<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = serde_json::to_string(self).map_err(|e| {
            generic_error(format!(
                "Serializing feed item into JSON string failed: {}",
                e.to_string()
            ))
        });

        write!(f, "{}", result.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use crate::model::picture::Picture;

    #[test]
    fn should_implement_trait_display() {
        assert_eq!(
            r#"{"title":"","src":"","srcset":"","target":""}"#,
            Picture::default().to_string()
        );
    }
}
