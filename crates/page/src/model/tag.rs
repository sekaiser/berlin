use std::fmt::Display;

use parser::FrontMatter;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Hash, Eq, PartialEq, Debug, PartialOrd, Ord, Clone)]
pub struct Tag {
    pub name: String,
    pub target: String,
}

impl Tag {
    pub fn uncategorized() -> Self {
        Tag::default()
    }

    pub fn new<S: Into<String>>(name: S) -> Self {
        let n = name.into();
        Self {
            target: format!("/tags/{}.html", &n),
            name: n,
        }
    }

    pub fn as_str(&self) -> &str {
        self.name.as_str()
    }

    pub fn to_string(self) -> String {
        self.name
    }
}

impl AsRef<str> for Tag {
    fn as_ref(&self) -> &str {
        self.name.as_str()
    }
}

impl From<String> for Tag {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for Tag {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<&String> for Tag {
    fn from(s: &String) -> Self {
        Self::new(s)
    }
}

impl Default for Tag {
    fn default() -> Self {
        Self::new("uncategorized")
    }
}

impl Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl Into<FrontMatter> for &Tag {
    fn into(self) -> FrontMatter {
        FrontMatter {
            title: Some(self.to_string()),
            author: None,
            description: None,
            published: None,
            tags: None,
            id: None,
        }
    }
}

#[cfg(test)]
mod test {

    use super::Tag;

    #[test]
    fn tag_name_as_str() {
        let name = "test";
        let tag = Tag::new(name);
        assert_eq!(tag.as_str(), name);
    }

    #[test]
    fn tag_name_to_string() {
        let name = "test";
        let tag = Tag::new(name);
        assert_eq!(tag.to_string(), String::from(name));
    }

    #[test]
    fn tag_name_as_ref() {
        let name = "test";
        let tag = Tag::new(name);
        assert_eq!(tag.as_ref(), name);
    }

    #[test]
    fn tag_name_from_str() {
        let name = "test";
        let tag = Tag::from(name);
        assert_eq!(tag.as_ref(), name);
    }

    #[test]
    fn tag_name_from_string() {
        let name = String::from("test");
        let tag = Tag::from(name);
        assert_eq!(tag.as_ref(), "test");
    }

    #[test]
    fn should_implement_trait_display() {
        let tag = Tag::default();
        assert_eq!("uncategorized", tag.to_string());
    }
}
