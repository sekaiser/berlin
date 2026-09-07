use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::Component;
use std::path::Path;

use anyhow::Context as _;
use anyhow::Error;
use berlin_content::FeedItem;
use berlin_content::TagGroup;
use berlin_content::WebsiteAssembly;
use berlin_core::WebsiteConfig;
use berlin_document::Document;
use serde::Serialize;
use slugify::slugify;

use crate::project::Project;
use crate::templates::Templates;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct Tag {
    name: String,
    target: String,
}

impl Tag {
    fn new(name: impl Into<String>) -> Result<Self, Error> {
        let name = name.into();
        Ok(Self {
            target: format!("/{}", tag_route(&name)?),
            name,
        })
    }
}

fn route_component(value: &str, kind: &str) -> Result<String, Error> {
    let slug = slugify!(value);
    if slug.is_empty() {
        anyhow::bail!("Cannot derive a {kind} route from '{value}'");
    }
    Ok(slug)
}

#[derive(Debug, Serialize)]
struct Article {
    title: String,
    description: String,
    author: String,
    date: String,
    target: String,
    tags: Vec<Tag>,
}

impl Article {
    fn from_document(document: &Document) -> Result<Self, Error> {
        let metadata = &document.metadata;
        let required = |name: &str| anyhow::anyhow!("Field {name} is not set!");
        let title = metadata.title.clone().ok_or_else(|| required("title"))?;
        let description = metadata
            .description
            .as_ref()
            .ok_or_else(|| required("description"))?;
        let description_source = markdown::Source::in_memory(description);
        let description = markdown::Renderer::default()
            .render(&description_source)?
            .into_string();
        let author = (!metadata.authors.is_empty())
            .then(|| metadata.authors.join(", "))
            .ok_or_else(|| required("author"))?;
        let date = metadata
            .published
            .as_ref()
            .ok_or_else(|| required("date"))?
            .to_string();
        let tags = metadata
            .tags
            .iter()
            .cloned()
            .map(Tag::new)
            .collect::<Result<Vec<_>, _>>()?;
        let target = format!("/{}", document_route(document)?);

        Ok(Self {
            title,
            description,
            author,
            date,
            target,
            tags,
        })
    }
}

#[derive(Serialize)]
struct FeedView {
    title: String,
    date_added: String,
    url: String,
    host: String,
    tags: Vec<Tag>,
}

impl TryFrom<&FeedItem> for FeedView {
    type Error = Error;

    fn try_from(item: &FeedItem) -> Result<Self, Self::Error> {
        Ok(Self {
            title: item.title.clone(),
            date_added: item.date_added.clone(),
            url: item.url.clone(),
            host: item.host.clone(),
            tags: item
                .tags
                .iter()
                .cloned()
                .map(Tag::new)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

#[derive(Serialize)]
struct Picture<'a> {
    title: &'a str,
    src: &'a str,
    srcset: &'a str,
    target: &'a str,
}

fn base_context(site: &WebsiteConfig) -> tera::Context {
    let mut context = tera::Context::new();
    context.insert("title", &site.title);
    context.insert("author", &site.author);
    context.insert("description", &site.description);
    // Templates append routes with a leading slash; URL serialization retains
    // a trailing slash for origins, so normalize only this presentation value.
    context.insert(
        "config_site_url",
        &site
            .url
            .as_ref()
            .map(|url| url.as_str().trim_end_matches('/')),
    );

    let profiles = &site.profiles;
    context.insert("linkedin", &profiles.linkedin);
    context.insert("github", &profiles.github);
    context.insert("twitter", &profiles.twitter);
    context.insert("og_image_path", "");
    context.insert("me", &profiles.linkedin);
    context
}

fn ordered_documents(website: &WebsiteAssembly) -> impl Iterator<Item = &Document> {
    website
        .documents_by_published()
        .as_slice()
        .iter()
        .filter_map(|reference| website.documents().get(reference))
}

fn articles<'a>(documents: impl Iterator<Item = &'a Document>) -> Result<Vec<Article>, Error> {
    documents
        .map(Article::from_document)
        .take(6)
        .collect::<Result<Vec<_>, _>>()
}

fn feed_views<'a>(items: impl Iterator<Item = &'a FeedItem>) -> Result<Vec<FeedView>, Error> {
    items.map(FeedView::try_from).collect()
}

fn tags_for_index(website: &WebsiteAssembly) -> Result<Vec<Tag>, Error> {
    website.tags().groups().keys().map(Tag::new).collect()
}

fn photos() -> Vec<Picture<'static>> {
    // No example-site photographs are bundled with Berlin.
    Vec::new()
}

fn document_context(base: &tera::Context, document: &Document) -> Result<tera::Context, Error> {
    let mut context = base.clone();
    let metadata = &document.metadata;
    insert_if_some(&mut context, "page_title", metadata.title.as_ref());
    insert_if_some(
        &mut context,
        "page_description",
        metadata.description.as_ref(),
    );
    insert_if_some(&mut context, "page_published", metadata.published.as_ref());
    insert_if_some(&mut context, "page_modified", metadata.modified.as_ref());
    if !metadata.authors.is_empty() {
        context.insert("page_author", &metadata.authors);
    }
    let tags = metadata
        .tags
        .iter()
        .cloned()
        .map(Tag::new)
        .collect::<Result<Vec<_>, _>>()?;
    context.insert("page_tags", &tags);
    // Preserve the existing shared-template aliases independently of page_* fallbacks.
    context.insert("description", &metadata.tags.join(","));
    context.insert("title", metadata.title.as_deref().unwrap_or(""));
    if document.id.0 != document.provenance.source {
        context.insert("page_id", &document.id.0);
    }
    Ok(context)
}

/// Inserts present metadata; absent values leave inherited context unchanged.
fn insert_if_some<T: Serialize>(context: &mut tera::Context, key: &str, value: Option<&T>) {
    if let Some(value) = value {
        context.insert(key, value);
    }
}

fn write_page(output_root: &Path, path: &Path, contents: String) -> Result<(), Error> {
    if path.as_os_str().is_empty()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        anyhow::bail!("Rendered page path is not confined: {}", path.display());
    }
    let output = output_root.join(path);
    std::fs::create_dir_all(
        output
            .parent()
            .context("Rendered page has no parent directory")?,
    )?;
    std::fs::write(output, contents)?;
    Ok(())
}

fn validate_routes(website: &WebsiteAssembly) -> Result<(), Error> {
    let mut routes = HashMap::new();

    for document in website.documents().as_slice() {
        register_route(
            &mut routes,
            document_route(document)?,
            format!("document '{}'", document.id.0),
        )?;
    }
    for tag in website.tags().groups().keys() {
        register_route(&mut routes, tag_route(tag)?, format!("tag '{tag}'"))?;
    }
    Ok(())
}

/// Registers routes case-insensitively for portability across filesystems.
fn register_route(
    routes: &mut HashMap<String, String>,
    path: String,
    owner: String,
) -> Result<(), Error> {
    match routes.entry(path.to_lowercase()) {
        Entry::Vacant(entry) => {
            entry.insert(owner);
            Ok(())
        }
        Entry::Occupied(entry) => {
            let previous = entry.get();
            anyhow::bail!("Published route '{path}' is produced by both {previous} and {owner}");
        }
    }
}

/// Returns the document's output-relative route, also used for public links.
fn document_route(document: &Document) -> Result<String, Error> {
    let title = document
        .metadata
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .with_context(|| format!("Document '{}' has no title", document.id.0))?;
    let slug = route_component(title, "document")?;
    Ok(format!("notes/{slug}.html"))
}

/// Returns the tag's output-relative route, also used for public links.
fn tag_route(tag: &str) -> Result<String, Error> {
    let slug = route_component(tag, "tag")?;
    Ok(format!("tags/{slug}.html"))
}

pub fn render(
    project: &Project,
    website: &WebsiteAssembly,
    config: &WebsiteConfig,
    output_root: &Path,
) -> Result<(), Error> {
    validate_routes(website)?;
    let mut session = RenderSession::new(project, website, config, output_root)?;

    session.render_index()?;
    session.render_notes_index()?;
    session.render_feed()?;
    session.render_documents()?;
    session.render_tag_pages()?;
    session.render_static_pages()?;
    session.render_photostream()
}

/// Shares templates and base context for one website render; each method owns
/// the context construction and output of one page family.
struct RenderSession<'a> {
    templates: Templates,
    base: tera::Context,
    website: &'a WebsiteAssembly,
    output_root: &'a Path,
}

impl<'a> RenderSession<'a> {
    fn new(
        project: &Project,
        website: &'a WebsiteAssembly,
        config: &WebsiteConfig,
        output_root: &'a Path,
    ) -> Result<Self, Error> {
        Ok(Self {
            templates: Templates::load(project.root().join("pages"))?,
            base: base_context(config),
            website,
            output_root,
        })
    }

    fn render_index(&self) -> Result<(), Error> {
        let mut context = self.base.clone();
        context.insert("articles", &articles(ordered_documents(self.website))?);
        context.insert("feed", &feed_views(self.website.feed().as_slice().iter())?);
        context.insert("tags", &tags_for_index(self.website)?);
        context.insert("photos", &photos());
        context.insert("slides", &Vec::<String>::new());
        self.render_template_page("index.tera", Path::new("index.html"), &context)
    }

    fn render_notes_index(&self) -> Result<(), Error> {
        let mut context = self.base.clone();
        context.insert("articles", &articles(ordered_documents(self.website))?);
        self.render_template_page("notes.tera", Path::new("notes.html"), &context)
    }

    fn render_feed(&self) -> Result<(), Error> {
        let mut context = self.base.clone();
        context.insert("feed", &feed_views(self.website.feed().as_slice().iter())?);
        self.render_template_page("feed.tera", Path::new("feed.html"), &context)
    }

    fn render_documents(&mut self) -> Result<(), Error> {
        for document in self.website.documents().as_slice() {
            let path = document_route(document)?;
            let context = document_context(&self.base, document)?;
            let contents = self
                .templates
                .render("notes/[slug].tera", document, &context)?;
            write_page(self.output_root, Path::new(&path), contents)?;
        }
        Ok(())
    }

    fn render_tag_pages(&self) -> Result<(), Error> {
        for (tag, group) in self.website.tags().groups() {
            let context = tag_context(&self.base, self.website, tag, group)?;
            self.render_template_page("tags/base.tera", Path::new(&tag_route(tag)?), &context)?;
        }
        Ok(())
    }

    fn render_static_pages(&self) -> Result<(), Error> {
        for (template, path) in [("about.tera", "about.html"), ("garage.tera", "garage.html")] {
            self.render_template_page(template, Path::new(path), &self.base)?;
        }
        Ok(())
    }

    fn render_photostream(&self) -> Result<(), Error> {
        let mut context = self.base.clone();
        context.insert("photos", &photos());
        self.render_template_page("photostream.tera", Path::new("photostream.html"), &context)
    }

    fn render_template_page(
        &self,
        template: &str,
        path: &Path,
        context: &tera::Context,
    ) -> Result<(), Error> {
        let contents = self.templates.render_template(template, context)?;
        write_page(self.output_root, path, contents)
    }
}

fn tag_context(
    base: &tera::Context,
    website: &WebsiteAssembly,
    tag: &str,
    group: &TagGroup,
) -> Result<tera::Context, Error> {
    let mut context = base.clone();
    context.insert("tag_name", tag);
    context.insert(
        "articles",
        &articles(ordered_documents(website).filter(|document| {
            group
                .documents
                .as_slice()
                .iter()
                .any(|reference| reference.0 == document.id)
        }))?,
    );
    context.insert(
        "feed",
        &feed_views(
            group
                .feed_items
                .as_slice()
                .iter()
                .filter_map(|reference| website.feed().get(reference)),
        )?,
    );
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(markdown_source: &str, source: &str) -> Document {
        let source = markdown::Source::new(markdown_source, source).unwrap();
        markdown::Parser::new().parse(&source).unwrap()
    }

    #[test]
    fn base_context_uses_render_configuration() {
        let config = WebsiteConfig {
            title: Some("My website".into()),
            description: Some("My description".into()),
            url: Some("https://example.com/".parse().unwrap()),
            profiles: berlin_core::WebsiteProfiles {
                github: Some("https://github.com/example".parse().unwrap()),
                ..Default::default()
            },
            ..Default::default()
        };
        let context = base_context(&config);
        assert_eq!(context.get("title"), Some(&serde_json::json!("My website")));
        assert_eq!(
            context.get("description"),
            Some(&serde_json::json!("My description"))
        );
        assert_eq!(
            context.get("config_site_url"),
            Some(&serde_json::json!("https://example.com"))
        );
        assert_eq!(
            context.get("github"),
            Some(&serde_json::json!("https://github.com/example"))
        );
    }

    #[test]
    fn index_tags_are_unique_across_documents_and_feed() {
        let documents = [
            document(
                "---\nid: tagged\ntags: [rust, rust]\n---\nBody",
                "file:///tagged.md",
            ),
            document("---\nid: untagged\n---\nBody", "file:///untagged.md"),
        ];
        let feed: Vec<_> = [vec!["rust".into(), "workflow".into()], vec![]]
            .into_iter()
            .enumerate()
            .map(|(index, tags)| FeedItem {
                id: berlin_content::FeedItemId(index.to_string()),
                title: "Reading".into(),
                date_added: "2026-09-07".into(),
                url: "https://example.com".into(),
                host: "example.com".into(),
                tags,
                source: "feed.csv".into(),
            })
            .collect();
        let website = WebsiteAssembly::new(
            berlin_content::DocumentCollection::new(documents.to_vec()),
            berlin_content::Feed::new(feed),
        )
        .unwrap();

        let tags = tags_for_index(&website).unwrap();
        assert_eq!(
            tags.iter().map(|tag| tag.name.as_str()).collect::<Vec<_>>(),
            ["rust", "uncategorized", "workflow"]
        );
        assert_eq!(tags[0].target, "/tags/rust.html");
    }

    #[test]
    fn render_session_writes_all_page_families_without_leaking_context() {
        let project = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        for (path, template) in [
            (
                "index.tera",
                "{{ title }}|{{ articles | length }}|{{ feed | length }}|{{ tags | length }}|{{ photos | length }}|{{ slides | length }}",
            ),
            ("notes.tera", "{{ articles | length }}"),
            ("feed.tera", "{{ feed | length }}"),
            ("notes/[slug].tera", "{{ page_title }}|{{ render() }}"),
            (
                "tags/base.tera",
                "{{ tag_name }}|{% for article in articles %}{{ article.title }}{% endfor %}|{{ feed | length }}",
            ),
            ("about.tera", "{{ title }}"),
            ("garage.tera", "{{ title }}"),
            ("photostream.tera", "{{ title }}|{{ photos | length }}"),
        ] {
            let path = project.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, template).unwrap();
        }
        let documents = [("First", "rust"), ("Second", "other")].map(|(title, tag)| {
            document(
                &format!("---\nid: {title}\ntitle: {title}\nauthor: [Ada]\ndescription: Summary\ndate: 2026-09-05\ntags: [{tag}]\n---\n{title} body.\n"),
                &format!("file:///content/{title}.md"),
            )
        });
        let website = WebsiteAssembly::new(
            berlin_content::DocumentCollection::new(documents.to_vec()),
            berlin_content::Feed::default(),
        )
        .unwrap();
        let mut base = tera::Context::new();
        base.insert("title", "Site");
        let mut session = RenderSession {
            templates: Templates::load(project.path()).unwrap(),
            base,
            website: &website,
            output_root: output.path(),
        };

        session.render_index().unwrap();
        session.render_notes_index().unwrap();
        session.render_feed().unwrap();
        session.render_documents().unwrap();
        session.render_tag_pages().unwrap();
        session.render_static_pages().unwrap();
        session.render_photostream().unwrap();

        for (path, expected) in [
            ("index.html", "Site|2|0|2|0|0"),
            ("notes.html", "2"),
            ("feed.html", "0"),
            ("notes/first.html", "First|<p>First body.</p>\n"),
            ("notes/second.html", "Second|<p>Second body.</p>\n"),
            ("tags/rust.html", "rust|First|0"),
            ("tags/other.html", "other|Second|0"),
            ("about.html", "Site"),
            ("garage.html", "Site"),
            ("photostream.html", "Site|0"),
        ] {
            assert_eq!(
                std::fs::read_to_string(output.path().join(path)).unwrap(),
                expected,
                "{path}"
            );
        }
        assert!(session.base.get("page_title").is_none());
        assert!(session.base.get("tag_name").is_none());
    }

    #[test]
    fn article_projection_uses_semantic_metadata() {
        let document = document(
            r#"---
title: "Typed publishing"
author: ["Ada", "Grace"]
description: "A **semantic** description."
date: 2026-09-05
tags: ["berlin", "publishing"]
---

The article body.
"#,
            "file:///content/typed-publishing.md",
        );

        let article = Article::from_document(&document).expect("projection should succeed");

        assert_eq!(article.title, "Typed publishing");
        assert_eq!(article.author, "Ada, Grace");
        assert_eq!(article.date, "2026-09-05");
        assert_eq!(article.target, "/notes/typed-publishing.html");
        assert_eq!(
            article.tags,
            vec![Tag::new("berlin").unwrap(), Tag::new("publishing").unwrap()]
        );
        assert!(article.description.contains("<strong>semantic</strong>"));
    }

    #[test]
    fn article_projection_rejects_incomplete_metadata() {
        let document = document(
            "Body without front matter.",
            "file:///content/incomplete.md",
        );

        let error = Article::from_document(&document).expect_err("title is required");

        assert_eq!(error.to_string(), "Field title is not set!");
    }

    #[test]
    fn page_context_uses_semantic_metadata() {
        let document = document(
            r#"---
title: "Typed publishing"
author: ["Ada", "Grace"]
description: "A semantic description."
date: 2026-09-05
lastmod: 2026-09-06
tags: ["berlin", "publishing"]
id: semantic-publishing
---

The article body.
"#,
            "file:///content/typed-publishing.md",
        );

        let mut base = tera::Context::new();
        for key in [
            "page_title",
            "page_description",
            "page_published",
            "page_modified",
        ] {
            base.insert(key, "inherited");
        }
        let context = document_context(&base, &document).unwrap();

        assert_eq!(
            context.get("page_title"),
            Some(&serde_json::json!("Typed publishing"))
        );
        assert_eq!(
            context.get("page_description"),
            Some(&serde_json::json!("A semantic description."))
        );
        assert_eq!(
            context.get("title"),
            Some(&serde_json::json!("Typed publishing"))
        );
        assert_eq!(
            context.get("description"),
            Some(&serde_json::json!("berlin,publishing"))
        );
        assert_eq!(
            context.get("page_author"),
            Some(&serde_json::json!(["Ada", "Grace"]))
        );
        assert_eq!(
            context.get("page_published"),
            Some(&serde_json::json!("2026-09-05"))
        );
        assert_eq!(
            context.get("page_modified"),
            Some(&serde_json::json!("2026-09-06"))
        );
        assert_eq!(
            context.get("page_id"),
            Some(&serde_json::json!("semantic-publishing"))
        );
    }

    #[test]
    fn absent_metadata_preserves_inherited_page_values() {
        let document = document("Body", "file:///content/untitled.md");
        let mut base = tera::Context::new();
        let inherited_keys = [
            "page_title",
            "page_description",
            "page_published",
            "page_modified",
            "page_author",
            "page_id",
        ];
        for key in inherited_keys {
            base.insert(key, "inherited");
        }
        base.insert("title", "Site title");
        base.insert("description", "Site description");
        base.insert("page_tags", &vec!["inherited"]);
        let original = base.clone().into_json();

        let context = document_context(&base, &document).unwrap();

        for key in inherited_keys {
            assert_eq!(context.get(key), base.get(key), "{key}");
        }
        assert_eq!(context.get("title"), Some(&serde_json::json!("")));
        assert_eq!(context.get("description"), Some(&serde_json::json!("")));
        assert_eq!(context.get("page_tags"), Some(&serde_json::json!([])));
        assert_eq!(base.into_json(), original);
    }

    #[test]
    fn absent_metadata_does_not_insert_null_page_values() {
        let document = document("Body", "file:///content/untitled.md");
        let context = document_context(&tera::Context::new(), &document).unwrap();

        for key in [
            "page_title",
            "page_description",
            "page_published",
            "page_modified",
            "page_author",
            "page_id",
        ] {
            assert!(context.get(key).is_none(), "{key}");
        }
    }

    #[test]
    fn route_validation_rejects_colliding_document_slugs() {
        let first = document(
            "---\ntitle: Same title\nid: first\n---\n",
            "file:///content/first.md",
        );
        let second = document(
            "---\ntitle: Same title\nid: second\n---\n",
            "file:///content/second.md",
        );
        let website = WebsiteAssembly::new(
            berlin_content::DocumentCollection::new(vec![first, second]),
            berlin_content::Feed::default(),
        )
        .unwrap();

        let error = validate_routes(&website).unwrap_err();

        assert!(error.to_string().contains("notes/same-title.html"));
        assert!(error.to_string().contains("first"));
        assert!(error.to_string().contains("second"));
    }

    #[test]
    fn route_registration_preserves_original_owner_on_case_insensitive_collision() {
        let mut routes = HashMap::new();
        register_route(&mut routes, "notes/Rust.html".into(), "first".into()).unwrap();

        let error =
            register_route(&mut routes, "notes/rust.html".into(), "second".into()).unwrap_err();

        assert_eq!(
            error.to_string(),
            "Published route 'notes/rust.html' is produced by both first and second"
        );
        assert_eq!(routes.len(), 1);
        assert_eq!(routes["notes/rust.html"], "first");
    }

    #[test]
    fn document_routes_require_nonblank_titles() {
        let mut document = document("Body", "file:///content/untitled.md");
        document.id.0 = "untitled".into();
        for title in [None, Some("".into()), Some(" \t ".into())] {
            document.metadata.title = title;
            assert_eq!(
                document_route(&document).unwrap_err().to_string(),
                "Document 'untitled' has no title"
            );
        }
    }

    #[test]
    fn route_validation_rejects_colliding_tag_slugs() {
        let document = document(
            "---\ntitle: Article\nid: article\ntags: [\"Rust / Async\", \"Rust Async\"]\n---\n",
            "file:///content/article.md",
        );
        let website = WebsiteAssembly::new(
            berlin_content::DocumentCollection::new(vec![document]),
            berlin_content::Feed::default(),
        )
        .unwrap();

        let error = validate_routes(&website).unwrap_err().to_string();

        assert!(error.contains("tags/rust-async.html"));
        assert!(error.contains("tag 'Rust / Async'"));
        assert!(error.contains("tag 'Rust Async'"));
    }

    #[test]
    fn tags_are_slugged_before_becoming_routes() {
        let tag = Tag::new("Rust / Async").unwrap();

        assert_eq!(tag.target, "/tags/rust-async.html");
        assert_eq!(tag.target, format!("/{}", tag_route(&tag.name).unwrap()));
    }

    #[test]
    fn page_writer_rejects_paths_that_escape_the_output_root() {
        let directory = tempfile::tempdir().unwrap();

        let error =
            write_page(directory.path(), Path::new("../outside.html"), "bad".into()).unwrap_err();

        assert!(error.to_string().contains("not confined"));
        assert!(!directory.path().join("../outside.html").exists());
    }
}
