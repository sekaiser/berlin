//! Deserialization of Ox-Hugo front matter from a Comrak syntax tree.

use comrak::nodes::AstNode;
use comrak::nodes::NodeValue;
use serde::Deserialize;

use crate::Error;
use berlin_document::PublicationDate;

pub(super) const DELIMITER: &str = "---";

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FrontMatter {
    #[serde(default)]
    pub(crate) kind: berlin_document::DocumentKind,
    pub(crate) title: Option<String>,
    pub(crate) slug: Option<String>,
    #[serde(default)]
    pub(crate) previous_slugs: Vec<String>,
    #[serde(rename = "date")]
    pub(crate) published: Option<PublicationDate>,
    pub(crate) author: Option<Vec<String>>,
    pub(crate) description: Option<String>,
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) id: Option<String>,
    #[serde(default)]
    pub(crate) draft: bool,
    #[serde(default, deserialize_with = "deserialize_comments")]
    pub(crate) comments: bool,
    #[serde(rename = "lastmod")]
    pub(crate) modified: Option<PublicationDate>,
}

// Comments are an explicit author opt-in, not a truthy string or null value.
fn deserialize_comments<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<bool, D::Error> {
    struct Boolean;
    impl serde::de::Visitor<'_> for Boolean {
        type Value = bool;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a boolean comments flag")
        }
        fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<bool, E> {
            Ok(value)
        }
    }
    deserializer.deserialize_any(Boolean)
}

pub(crate) fn parse<'a>(
    root: &'a AstNode<'a>,
    source_uri: &str,
) -> Result<Option<FrontMatter>, Error> {
    for node in root.descendants() {
        let data = node.data.borrow();
        let NodeValue::FrontMatter(text) = &data.value else {
            continue;
        };

        return extract_contents(text, source_uri)
            .and_then(|contents| deserialize(contents, source_uri))
            .map(Some);
    }

    Ok(None)
}

fn extract_contents<'a>(node: &'a str, source_uri: &str) -> Result<&'a str, Error> {
    let framed = node.trim_end_matches(['\r', '\n']);
    let contents = framed
        .strip_prefix(DELIMITER)
        .and_then(strip_leading_line_ending)
        .and_then(|contents| contents.strip_suffix(DELIMITER))
        .and_then(strip_trailing_line_ending);

    contents.ok_or_else(|| Error::InvalidFrontMatter {
        source_uri: source_uri.to_owned(),
        message: "front matter delimiters are malformed".into(),
    })
}

fn strip_leading_line_ending(text: &str) -> Option<&str> {
    text.strip_prefix("\r\n")
        .or_else(|| text.strip_prefix('\n'))
}

fn strip_trailing_line_ending(text: &str) -> Option<&str> {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
}

fn deserialize(contents: &str, source_uri: &str) -> Result<FrontMatter, Error> {
    serde_saphyr::from_str(contents).map_err(|error| Error::InvalidFrontMatter {
        source_uri: source_uri.to_owned(),
        message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::deserialize;

    #[test]
    fn rejects_multiple_yaml_documents() {
        let error = deserialize(
            "title: First\n---\ntitle: Second\n",
            "file:///content/article.md",
        )
        .unwrap_err();

        assert!(matches!(
            error,
            crate::Error::InvalidFrontMatter { source_uri, .. }
                if source_uri == "file:///content/article.md"
        ));
    }
}
