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
use berlin_document::DocumentKind;
use serde::Serialize;
use slugify::slugify;

use crate::project::Project;
use crate::templates::Templates;

mod search;

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
    preview: Option<berlin_document::Preview>,
    kind: DocumentKind,
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
            kind: document.kind.clone(),
            preview: metadata.preview.clone(),
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
    annotation: Option<String>,
}

impl TryFrom<&FeedItem> for FeedView {
    type Error = Error;

    fn try_from(item: &FeedItem) -> Result<Self, Self::Error> {
        Ok(Self {
            title: item.title.clone(),
            date_added: item.date_added.clone(),
            url: item.url.clone(),
            host: item.host.clone(),
            annotation: item.annotation.clone(),
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
    context.insert("config_giscus", &site.giscus);
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
        .collect::<Result<Vec<_>, _>>()
}

/// Authored entry points, independent of the recent-writing cutoff.
fn guides(website: &WebsiteAssembly) -> Result<Vec<Article>, Error> {
    articles(ordered_documents(website).filter(|document| document.kind == DocumentKind::Guide))
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
    context.insert("page_kind", &document.kind);
    let metadata = &document.metadata;
    insert_discussion_context(&mut context, document)?;
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

/// Binds an opted-in article to its stable identity, independently of its route.
fn insert_discussion_context(
    context: &mut tera::Context,
    document: &Document,
) -> Result<(), Error> {
    context.insert("page_comments", &document.metadata.comments);
    if !document.metadata.comments {
        return Ok(());
    }
    if document.id.0 == document.provenance.source {
        anyhow::bail!("Comments require an explicit stable document ID");
    }
    let settings = context
        .get("config_giscus")
        .and_then(|value| value.as_object())
        .context("Article enables comments but website_config.giscus is not configured")?;
    let repo = settings
        .get("repo")
        .and_then(|value| value.as_str())
        .context("Missing giscus repository")?;
    let term = format!("berlin:{}", document.id.0);
    let mut discussion = url::Url::parse(&format!("https://github.com/{repo}/discussions"))?;
    discussion
        .query_pairs_mut()
        .append_pair("discussions_q", &format!("\"{term}\""));
    context.insert("page_discussion_term", &term);
    context.insert("page_discussion_url", discussion.as_str());
    Ok(())
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
        for path in previous_routes(document)? {
            register_route(
                &mut routes,
                path,
                format!("redirect for document '{}'", document.id.0),
            )?;
        }
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
    let slug = match document.metadata.slug.as_deref() {
        Some(slug) => validated_slug(document, slug)?.to_owned(),
        None => route_component(title, "document")?,
    };
    Ok(format!("notes/{slug}.html"))
}

/// Authored slugs are exact identifiers, not text to be silently rewritten.
fn validated_slug<'a>(document: &Document, slug: &'a str) -> Result<&'a str, Error> {
    if !slug.split('-').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    }) {
        anyhow::bail!(
            "Document '{}' has invalid slug '{slug}'; use lowercase ASCII letters, digits, and single hyphens",
            document.id.0
        );
    }
    Ok(slug)
}

fn previous_routes(document: &Document) -> Result<Vec<String>, Error> {
    if !document.metadata.previous_slugs.is_empty() && document.metadata.slug.is_none() {
        anyhow::bail!(
            "Document '{}' declares previous_slugs without an explicit slug",
            document.id.0
        );
    }
    document
        .metadata
        .previous_slugs
        .iter()
        .map(|slug| validated_slug(document, slug).map(|slug| format!("notes/{slug}.html")))
        .collect()
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
    if let Some(giscus) = &config.giscus {
        giscus.validate().map_err(anyhow::Error::msg)?;
    }
    let templates = match &config.theme {
        Some(selected) => {
            let theme = super::theme::Theme::load(project.root(), selected)?;
            let templates = theme.templates()?;
            theme.write_assets(output_root)?;
            templates
        }
        None => Templates::load(project.root().join("pages"))?,
    };
    let mut session = RenderSession::new(website, config, output_root, templates)?;

    session.render_index()?;
    session.render_notes_index()?;
    session.render_feed()?;
    session.render_documents()?;
    session.render_redirects()?;
    session.render_tag_pages()?;
    session.render_search()?;
    session.render_about()
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
        website: &'a WebsiteAssembly,
        config: &WebsiteConfig,
        output_root: &'a Path,
        mut templates: Templates,
    ) -> Result<Self, Error> {
        let mut base = base_context(config);
        let site_url = base
            .get("config_site_url")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let routes = website
            .documents()
            .as_slice()
            .iter()
            .map(|document| {
                Ok((
                    document.id.clone(),
                    format!("{site_url}/{}", document_route(document)?),
                ))
            })
            .collect::<Result<HashMap<_, _>, Error>>()?;
        base.insert("has_search", &templates.contains("search.tera"));
        templates.set_document_routes(routes);
        Ok(Self {
            templates,
            base,
            website,
            output_root,
        })
    }

    fn render_index(&self) -> Result<(), Error> {
        const RECENT_NOTE_LIMIT: usize = 6;
        let mut context = self.base.clone();
        context.insert("guides", &guides(self.website)?);
        context.insert(
            "articles",
            &articles(ordered_documents(self.website).take(RECENT_NOTE_LIMIT))?,
        );
        context.insert("feed", &feed_views(self.website.feed().as_slice().iter())?);
        context.insert("tags", &tags_for_index(self.website)?);
        context.insert("photos", &photos());
        context.insert("slides", &Vec::<String>::new());
        self.render_template_page("index.tera", Path::new("index.html"), &context)
    }

    fn render_notes_index(&self) -> Result<(), Error> {
        let mut context = self.base.clone();
        context.insert("guides", &guides(self.website)?);
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
            let mut context = document_context(&self.base, document)?;
            context.insert("page_backlinks", &backlink_views(self.website, document)?);
            insert_page_path(&mut context, Path::new(&path))?;
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

    fn render_redirects(&self) -> Result<(), Error> {
        for document in self.website.documents().as_slice() {
            let paths = previous_routes(document)?;
            if paths.is_empty() {
                continue;
            }
            let target = document_route(document)?;
            let site_url = self
                .base
                .get("config_site_url")
                .and_then(|value| value.as_str());
            let destination = match site_url {
                Some(url) => format!("{url}/{target}"),
                // All article redirects live beside their target under notes/.
                None => target
                    .strip_prefix("notes/")
                    .expect("article route")
                    .to_owned(),
            };
            let mut context = tera::Context::new();
            context.insert("target", &target);
            context.insert("destination", &destination);
            context.insert("title", &document.metadata.title);
            let html = tera::Tera::one_off(include_str!("website/redirect.tera"), &context, true)?;
            for path in paths {
                write_page(self.output_root, Path::new(&path), html.clone())?;
            }
        }
        Ok(())
    }

    fn render_about(&self) -> Result<(), Error> {
        self.render_template_page("about.tera", Path::new("about.html"), &self.base)
    }

    fn render_search(&self) -> Result<(), Error> {
        if !self.templates.contains("search.tera") {
            return Ok(());
        }
        let index = search::Index::from_website(self.website)?;
        let json = serde_json::to_string(&index)?;
        let mut context = self.base.clone();
        use sha2::{Digest, Sha256};
        context.insert(
            "search_index_version",
            &Sha256::digest(json.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        );
        write_page(self.output_root, Path::new("search/index.json"), json)?;
        write_page(
            self.output_root,
            Path::new("search/client.js"),
            include_str!("../../support/web/search.js").into(),
        )?;
        self.render_template_page("search.tera", Path::new("search.html"), &context)
    }

    fn render_template_page(
        &self,
        template: &str,
        path: &Path,
        context: &tera::Context,
    ) -> Result<(), Error> {
        let mut context = context.clone();
        insert_page_path(&mut context, path)?;
        let contents = self.templates.render_template(template, &context)?;
        write_page(self.output_root, path, contents)
    }
}

#[derive(Serialize)]
struct BacklinkView {
    title: String,
    path: String,
    excerpt: String,
}

fn backlink_views(
    website: &WebsiteAssembly,
    document: &Document,
) -> Result<Vec<BacklinkView>, Error> {
    website
        .references()
        .incoming(&document.id)
        .iter()
        .map(|backlink| {
            let source = website
                .documents()
                .get(&backlink.source)
                .context("Backlink source is missing from publication")?;
            let mut excerpt = backlink.excerpt.chars().take(280).collect::<String>();
            if excerpt.len() < backlink.excerpt.len() {
                excerpt.push('…');
            }
            Ok(BacklinkView {
                title: source
                    .metadata
                    .title
                    .clone()
                    .context("Backlink source has no title")?,
                path: format!("/{}#{}", document_route(source)?, backlink.anchor.0),
                excerpt,
            })
        })
        .collect()
}

/// Provides an output-relative URL path for navigation and sharing metadata.
/// The home page is addressed by its directory URL, not by `index.html`.
fn insert_page_path(context: &mut tera::Context, path: &Path) -> Result<(), Error> {
    let path = path.to_str().context("Page path is not valid UTF-8")?;
    let page_path = if path == "index.html" {
        "/".to_owned()
    } else {
        format!("/{path}")
    };
    context.insert("page_path", &page_path);
    Ok(())
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
    use berlin_content::DocumentCollection;

    fn document(markdown_source: &str, source: &str) -> Document {
        let source = markdown::Source::new(markdown_source, source).unwrap();
        markdown::Parser::new().parse(&source).unwrap()
    }

    #[test]
    fn backlinks_are_scoped_to_publication_and_follow_identity_across_renames() {
        let source = document(
            "---\nid: source\ntitle: Source\n---\nBuilds on [target](id:target).\n\nAnother [mention](id:target).",
            "file:///source.md",
        );
        let target = document(
            "---\nid: target\ntitle: Target\n---\nBody",
            "file:///target.md",
        );
        let draft = document(
            "---\nid: draft\ntitle: SECRET\ndraft: true\n---\nSECRET [target](id:target) and [missing](id:missing).",
            "file:///private.md",
        );
        let build = |source: Document, target: Document| {
            WebsiteAssembly::new(
                DocumentCollection::new(vec![source, target, draft.clone()]),
                berlin_content::Feed::default(),
            )
            .unwrap()
        };
        let website = build(source.clone(), target.clone());
        assert_eq!(website.documents().len(), 2);
        assert_eq!(website.references().incoming(&target.id).len(), 2);
        let views = backlink_views(&website, &target).unwrap();
        assert_eq!(views[0].excerpt, "Builds on target.");
        assert_eq!(views[0].path, "/notes/source.html#bln-ref-1");
        assert!(!serde_json::to_string(&website).unwrap().contains("SECRET"));
        let mut renamed = source.clone();
        renamed.metadata.title = Some("Renamed source".into());
        renamed.provenance.source = "file:///moved.md".into();
        assert_eq!(
            backlink_views(&build(renamed, target.clone()), &target).unwrap()[0].path,
            "/notes/renamed-source.html#bln-ref-1"
        );
        let mut hidden = target;
        hidden.metadata.draft = true;
        let error = WebsiteAssembly::new(
            DocumentCollection::new(vec![source, hidden]),
            berlin_content::Feed::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("file:///source.md#bln-ref-1"));
        assert!(
            error
                .to_string()
                .contains("not in the published collection")
        );
    }

    #[test]
    fn backlink_index_groups_passages_and_rebuilds_after_mapping() {
        let mut source = document(
            "---\nid: source\ntitle: Source\n---\n[One](id:target) and [two](id:target).\n\n[One](id:target) and [two](id:target).\n\n[Self](id:source).",
            "file:///source.md",
        );
        let target = document(
            "---\nid: target\ntitle: Target\n---\nBody",
            "file:///target.md",
        );
        let build = |source: Document| {
            WebsiteAssembly::new(
                DocumentCollection::new(vec![source, target.clone()]),
                berlin_content::Feed::default(),
            )
            .unwrap()
        };
        let website = build(source.clone());
        // Identical text in two different paragraphs remains two distinct passages.
        assert_eq!(website.references().incoming(&target.id).len(), 2);
        assert!(website.references().incoming(&source.id).is_empty());
        source.blocks.clear();
        assert!(build(source).references().incoming(&target.id).is_empty());
    }

    #[test]
    fn comments_require_configuration_and_keep_the_document_identity() {
        let mut article = document(
            "---\nid: stable-id\ncomments: true\n---\nBody",
            "file:///article.md",
        );
        assert!(document_context(&base_context(&WebsiteConfig::default()), &article).is_err());
        let config = WebsiteConfig {
            giscus: Some(berlin_core::GiscusConfig {
                repo: "owner/site".into(),
                repo_id: "R_123".into(),
                category: "Comments".into(),
                category_id: "DIC_123".into(),
                theme: None,
            }),
            ..Default::default()
        };
        let first = document_context(&base_context(&config), &article).unwrap();
        article.metadata.title = Some("Renamed title".into());
        article.provenance.source = "file:///renamed.md".into();
        let renamed = document_context(&base_context(&config), &article).unwrap();
        assert_eq!(
            first.get("page_discussion_term"),
            renamed.get("page_discussion_term")
        );
        assert_eq!(
            first.get("page_discussion_url"),
            renamed.get("page_discussion_url")
        );
        assert_eq!(first.get("page_comments"), Some(&serde_json::json!(true)));
        article.id.0 = article.provenance.source.clone();
        assert!(document_context(&base_context(&config), &article).is_err());
        article.metadata.comments = false;
        let disabled =
            document_context(&base_context(&WebsiteConfig::default()), &article).unwrap();
        assert_eq!(
            disabled.get("page_comments"),
            Some(&serde_json::json!(false))
        );
        assert!(disabled.get("page_discussion_term").is_none());
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
                annotation: None,
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
            ("search.tera", "Search {{ title }}"),
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
        session.render_about().unwrap();
        session.render_search().unwrap();
        let search: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(output.path().join("search/index.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(search["schema_version"], 1);
        assert_eq!(search["documents"].as_array().unwrap().len(), 2);
        assert!(output.path().join("search/client.js").exists());

        for (path, expected) in [
            ("index.html", "Site|2|0|2|0|0"),
            ("notes.html", "2"),
            ("feed.html", "0"),
            ("notes/first.html", "First|<p>First body.</p>\n"),
            ("notes/second.html", "Second|<p>Second body.</p>\n"),
            ("tags/rust.html", "rust|First|0"),
            ("tags/other.html", "other|Second|0"),
            ("about.html", "Site"),
            ("search.html", "Search Site"),
        ] {
            assert_eq!(
                std::fs::read_to_string(output.path().join(path)).unwrap(),
                expected,
                "{path}"
            );
        }
        assert!(session.base.get("page_title").is_none());
        assert!(session.base.get("tag_name").is_none());
        assert!(session.base.get("page_path").is_none());
        assert!(!output.path().join("garage.html").exists());
        assert!(!output.path().join("photostream.html").exists());
        assert!(project.path().join("garage.tera").exists());
        assert!(project.path().join("photostream.tera").exists());
    }

    #[test]
    fn guides_are_authored_published_entries_not_recent_notes_or_tags() {
        let documents: Vec<_> = (1..=9)
            .map(|number| {
                let kind = if number <= 2 { "guide" } else { "article" };
                document(
                    &format!("---\nid: note-{number}\ntitle: Note {number}\nauthor: [Ada]\ndescription: Summary\ndate: 2026-09-0{number}\nkind: {kind}\ndraft: {}\ntags: [guide]\n---\nBody", number == 2),
                    &format!("file:///note-{number}.md"),
                )
            })
            .collect();
        let website = WebsiteAssembly::new(
            berlin_content::DocumentCollection::new(documents),
            berlin_content::Feed::default(),
        )
        .unwrap();
        let guides = guides(&website).unwrap();
        assert_eq!(guides.len(), 1);
        assert_eq!(guides[0].title, "Note 1");
        assert_eq!(guides[0].kind, DocumentKind::Guide);
        assert!(
            !ordered_documents(&website)
                .take(6)
                .any(|document| document.kind == DocumentKind::Guide)
        );
        let context = document_context(
            &tera::Context::new(),
            website.documents().as_slice().first().unwrap(),
        )
        .unwrap();
        assert_eq!(context.get("page_kind"), Some(&serde_json::json!("guide")));
    }

    #[test]
    fn only_the_homepage_limits_recent_notes() {
        let project = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join("tags")).unwrap();
        for name in ["index.tera", "notes.tera", "tags/base.tera"] {
            std::fs::write(
                project.path().join(name),
                "{{ page_path | safe }}|{{ articles | length }}|{% for a in articles %}{{ a.title }},{% endfor %}",
            )
            .unwrap();
        }
        let documents = (1..=8)
            .map(|number| {
                document(
                    &format!(
                        "---\nid: note-{number}\ntitle: Note {number}\nauthor: [Ada]\ndescription: Summary\ndate: 2026-09-0{number}\ntags: [rust]\n---\nBody"
                    ),
                    &format!("file:///note-{number}.md"),
                )
            })
            .collect::<Vec<_>>();
        let website = WebsiteAssembly::new(
            berlin_content::DocumentCollection::new(documents),
            berlin_content::Feed::default(),
        )
        .unwrap();
        let session = RenderSession {
            templates: Templates::load(project.path()).unwrap(),
            base: tera::Context::new(),
            website: &website,
            output_root: output.path(),
        };
        session.render_index().unwrap();
        session.render_notes_index().unwrap();
        session.render_tag_pages().unwrap();

        for (path, expected) in [
            (
                "index.html",
                "/|6|Note 8,Note 7,Note 6,Note 5,Note 4,Note 3,",
            ),
            (
                "notes.html",
                "/notes.html|8|Note 8,Note 7,Note 6,Note 5,Note 4,Note 3,Note 2,Note 1,",
            ),
            (
                "tags/rust.html",
                "/tags/rust.html|8|Note 8,Note 7,Note 6,Note 5,Note 4,Note 3,Note 2,Note 1,",
            ),
        ] {
            assert_eq!(
                std::fs::read_to_string(output.path().join(path)).unwrap(),
                expected
            );
        }
        assert!(session.base.get("page_path").is_none());
    }

    #[test]
    fn page_paths_are_output_relative_and_keep_nested_routes() {
        let mut context = tera::Context::new();
        for (path, expected) in [
            ("index.html", "/"),
            ("notes/some-note.html", "/notes/some-note.html"),
            ("about.html", "/about.html"),
        ] {
            insert_page_path(&mut context, Path::new(path)).unwrap();
            assert_eq!(context.get("page_path"), Some(&serde_json::json!(expected)));
        }
    }

    #[test]
    fn article_projection_uses_semantic_metadata() {
        let document = document(
            r#"---
title: "Typed publishing"
preview: {source: /attachments/diagram.svg, alt: A diagram, width: 320, height: 224}
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
        assert_eq!(article.preview, document.metadata.preview);
        assert_eq!(article.preview.as_ref().unwrap().width.get(), 320);
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
