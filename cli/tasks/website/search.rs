//! Publication-only search projection. Never serialize document provenance or
//! component properties; index visible semantic text, not source files or HTML.

use anyhow::Error;
use berlin_content::WebsiteAssembly;
use berlin_document::{Block, Document, DocumentKind, plain_text};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub(super) struct Index {
    schema_version: u8,
    documents: Vec<SearchDocument>,
    tag_paths: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct SearchDocument {
    title: String,
    kind: DocumentKind,
    tags: Vec<String>,
    path: String,
    sections: Vec<Section>,
}

#[derive(Default, Serialize)]
struct Section {
    heading: String,
    fragment: Option<String>,
    text: String,
}

impl Index {
    pub(super) fn from_website(website: &WebsiteAssembly) -> Result<Self, Error> {
        let documents = super::ordered_documents(website)
            .map(SearchDocument::from_document)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            schema_version: 1,
            documents,
            tag_paths: website
                .tags()
                .groups()
                .keys()
                .map(|tag| Ok((tag.clone(), super::tag_route(tag)?)))
                .collect::<Result<_, Error>>()?,
        })
    }
}

impl SearchDocument {
    fn from_document(document: &Document) -> Result<Self, Error> {
        let mut sections = vec![Section::default()];
        collect_blocks(&document.blocks, 0, &mut sections);
        for section in &mut sections {
            section.text = section
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
        }
        // Keep an entry even for a heading-only document: titles and tags are
        // searchable independently of whether it has introductory prose.
        Ok(Self {
            path: super::document_route(document)?,
            title: document.metadata.title.clone().unwrap_or_default(),
            kind: document.kind.clone(),
            tags: document.metadata.tags.clone(),
            sections,
        })
    }
}

fn append(sections: &mut [Section], current: usize, text: &str) {
    sections[current].text.push_str(text);
    sections[current].text.push(' ');
}

fn collect_blocks(blocks: &[Block], current: usize, sections: &mut Vec<Section>) {
    for block in blocks {
        match block {
            Block::Section {
                id, title, blocks, ..
            } => {
                let section = sections.len();
                let mut url = url::Url::parse("https://search.invalid/").expect("static URL");
                url.set_fragment(Some(&id.0));
                sections.push(Section {
                    heading: plain_text(title),
                    fragment: url.fragment().map(str::to_owned),
                    text: String::new(),
                });
                collect_blocks(blocks, section, sections);
            }
            Block::Paragraph { content } | Block::Heading { content, .. } => {
                append(sections, current, &plain_text(content))
            }
            Block::BlockQuote { blocks }
            | Block::Component { blocks, .. }
            | Block::FootnoteDefinition { blocks, .. } => collect_blocks(blocks, current, sections),
            Block::List(list) => {
                for item in &list.items {
                    collect_blocks(&item.blocks, current, sections);
                }
            }
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        append(sections, current, &plain_text(cell));
                    }
                }
            }
            Block::Code(code) => {
                if let Some(caption) = &code.caption {
                    append(sections, current, &plain_text(caption));
                }
                append(sections, current, &code.value);
            }
            Block::Figure(figure) => {
                if let Some(caption) = &figure.caption {
                    append(sections, current, caption);
                }
            }
            Block::Html { .. } | Block::ThematicBreak => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use berlin_content::{DocumentCollection, Feed};

    fn document(id: &str, extra: &str, body: &str) -> Document {
        markdown::Parser::new()
            .parse(
                &markdown::Source::new(
                    &format!("---\nid: {id}\ntitle: {id}\n{extra}---\n{body}"),
                    format!("file:///private/{id}.md"),
                )
                .unwrap(),
            )
            .unwrap()
    }

    #[test]
    fn only_indexes_published_semantic_content_and_preserves_nested_sections() {
        let public = document(
            "Public",
            "tags: [rust]\n",
            "Introduction.\n\n## Parent {#parent}\nParent prose.\n\n### Child {#child}\nNested **explanation**.\n\n```rust\nserde_json::from_str(input)\n```\n\n<div>raw-html-secret</div>",
        );
        let private = document(
            "private-title",
            "draft: true\ntags: [private-tag]\n",
            "private-body",
        );
        let website = WebsiteAssembly::new(
            DocumentCollection::new(vec![public, private]),
            Feed::default(),
        )
        .unwrap();
        let index = Index::from_website(&website).unwrap();
        assert_eq!(index.documents.len(), 1);
        assert_eq!(
            index.tag_paths.get("rust").map(String::as_str),
            Some("tags/rust.html")
        );
        assert!(!index.tag_paths.contains_key("private-tag"));
        let doc = &index.documents[0];
        assert_eq!(doc.path, "notes/public.html");
        assert_eq!(doc.sections[1].fragment.as_deref(), Some("parent"));
        assert_eq!(doc.sections[1].text, "Parent prose.");
        assert_eq!(doc.sections[2].heading, "Child");
        assert!(doc.sections[2].text.contains("Nested explanation."));
        assert!(doc.sections[2].text.contains("serde_json::from_str"));
        let json = serde_json::to_string(&index).unwrap();
        for absent in ["private", "file:", "provenance", "raw-html-secret"] {
            assert!(!json.contains(absent));
        }
    }

    #[test]
    fn uses_mapped_titles_and_encodes_actual_section_fragments() {
        let mut doc = document("Original", "", "## Heading {#first}\nText.");
        doc.metadata.title = Some("Mapped title".into());
        if let Block::Section { id, .. } = &mut doc.blocks[0] {
            id.0 = "a b".into();
        }
        let projected = SearchDocument::from_document(&doc).unwrap();
        assert_eq!(projected.path, "notes/mapped-title.html");
        assert_eq!(projected.title, "Mapped title");
        assert_eq!(projected.sections[1].fragment.as_deref(), Some("a%20b"));
    }
}
