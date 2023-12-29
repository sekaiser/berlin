use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Output(String);

impl Output {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn to_string(self) -> String {
        self.0
    }

    pub fn replace(&self, from: &str, to: &str) -> String {
        self.0.replace(from, to)
    }

    pub fn contains(&self, s: &str) -> bool {
        self.0.contains(s)
    }
}

impl AsRef<str> for Output {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for Output {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for Output {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl std::fmt::Display for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
