//! HTML projection for explicit site intentions. Source documents remain borrowed.
use super::*;
use berlin_document::{Block, ContentId, Inline, plain_text};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SiteHtmlOptions {
    pub description: Option<String>,
    pub github: Option<String>,
    pub linkedin: Option<String>,
    pub twitter: Option<String>,
    pub discussion: Option<DiscussionSettings>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscussionSettings {
    pub repo: String,
    pub repo_id: String,
    pub category: String,
    pub category_id: String,
    pub theme: Option<String>,
}
#[derive(Serialize)]
pub struct SiteManifest {
    pub schema_version: u32,
    pub site_id: PublicationId,
    pub catalog_digest: String,
    pub queries: QueryEvidenceTable,
    pub source_scope: QueryEvidence,
    pub catalog_inputs: Vec<ResourceInputEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<OutputEvidence>,
    pub composition_fingerprint: String,
    pub render_input_fingerprint: String,
    pub routes: Vec<String>,
    pub pages: BTreeMap<String, PublicationManifest>,
    pub render_inputs: BTreeMap<String, String>,
}
pub struct HtmlSite {
    pub files: BTreeMap<String, Vec<u8>>,
    pub stylesheets: BTreeSet<String>,
    pub manifest: SiteManifest,
}
#[derive(Clone, Serialize)]
struct Backlink {
    source_id: String,
    title: String,
    path: String,
    excerpt: String,
}
fn failure(code: Code, detail: impl Into<String>) -> Diagnostic {
    error(code, detail)
}

impl PublicationHtmlRenderer {
    pub fn render_site(
        &self,
        assembly: &SiteAssembly,
        registry: &ComponentRegistry,
        site: &PublicationSite,
        options: &SiteHtmlOptions,
    ) -> Result<HtmlSite> {
        let mount = mount_url(
            &site
                .url
                .parse()
                .map_err(|e| failure(Code::UnsafeUrl, format!("destination URL: {e}")))?,
        )?;
        let catalog = assembly.catalog();
        let limits = catalog.limits();
        for value in options
            .github
            .iter()
            .chain(options.linkedin.iter())
            .chain(options.twitter.iter())
        {
            if value.len() > catalog.options().resource_limits.url_bytes {
                return Err(failure(
                    Code::ResourceLimit,
                    "profile URL byte limit exceeded",
                ));
            }
            let url: url::Url = value
                .parse()
                .map_err(|e| failure(Code::UnsafeUrl, format!("profile URL: {e}")))?;
            validate_public_url(&url)?;
        }
        if options
            .description
            .as_ref()
            .is_some_and(|s| s.len() > limits.string_bytes)
        {
            return Err(failure(
                Code::ResourceLimit,
                "site description byte limit exceeded",
            ));
        }
        let mut local_urls = BTreeMap::new();
        for (id, route) in assembly.document_routes() {
            local_urls.insert(
                id.clone(),
                mount
                    .join(route)
                    .map_err(|e| failure(Code::UnsafeUrl, e.to_string()))?
                    .to_string(),
            );
        }
        let semantic_routes = catalog
            .resources()
            .filter(|e| e.descriptor().origin == ResourceOrigin::Local)
            .filter_map(|e| {
                local_urls
                    .get(&e.descriptor().id)
                    .map(String::as_str)
                    .or_else(|| e.original_url())
                    .map(|url| (ContentId(e.descriptor().id.leaf().into()), url.to_owned()))
            })
            .collect::<HashMap<_, _>>();
        let semantic = SemanticRenderer::default().with_document_routes(semantic_routes);
        let mut backlinks = BTreeMap::<ResourceId, Vec<Backlink>>::new();
        for (id, route) in assembly.document_routes() {
            let doc = catalog.get(id)?.handle().try_local_document()?;
            let mut seen = BTreeSet::new();
            for reference in doc.references() {
                let target = ResourceId::local(&reference.link.target.0)?;
                if target == *id || !seen.insert((target.clone(), reference.passage)) {
                    continue;
                }
                let mut url = mount
                    .join(route)
                    .map_err(|e| failure(Code::UnsafeUrl, e.to_string()))?;
                url.set_fragment(Some(&reference.link.anchor.0));
                let path = url
                    .path()
                    .strip_prefix(mount.path().trim_end_matches('/'))
                    .unwrap_or(url.path());
                backlinks.entry(target).or_default().push(Backlink {
                    source_id: id.leaf().into(),
                    title: doc.metadata.title.clone().unwrap_or_default(),
                    path: format!("{path}#{}", url.fragment().unwrap_or_default()),
                    excerpt: reference.excerpt.chars().take(280).collect(),
                });
            }
        }
        let mut files = BTreeMap::new();
        let mut total_bytes = 0usize;
        let mut stylesheets = BTreeSet::from([
            "vendor.css".into(),
            "notebook.css".into(),
            "plain.css".into(),
            "project.css".into(),
        ]);
        let mut pages = BTreeMap::new();
        for (route, intention) in assembly.routes() {
            let mut context = base_context(assembly, site, options, route, &mount)?;
            let bytes = match intention {
                SiteRoute::Composed { assembly: page, .. } => {
                    let result = self.render_mounted(
                        page,
                        registry,
                        site,
                        "publications/site_page.tera",
                        Some(&context),
                        &local_urls,
                    )?;
                    stylesheets.extend(result.stylesheets);
                    pages.insert(route.clone(), result.manifest);
                    result.html.into_bytes()
                }
                SiteRoute::Document { source } => {
                    stylesheets.insert("article.css".into());
                    let entry = catalog.resolve_resource(source)?;
                    let doc = entry.handle().try_local_document()?;
                    context.insert("page_id", source.id.leaf());
                    context.insert("page_title", &entry.descriptor().title);
                    context.insert("title", &entry.descriptor().title);
                    context.insert("page_description", &entry.descriptor().description);
                    context.insert("page_kind", &doc.kind);
                    context.insert("page_author", &doc.metadata.authors);
                    context.insert("page_published", &doc.metadata.published);
                    context.insert("page_modified", &doc.metadata.modified);
                    let tags = entry
                        .descriptor()
                        .topics
                        .iter()
                        .map(|tag| {
                            let route = tag_route(tag)?;
                            let target = assembly
                                .routes()
                                .contains_key(&route)
                                .then(|| format!("/{route}"));
                            Ok(json!({"name":tag,"target":target}))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    context.insert("page_tags", &tags);
                    context.insert("page_comments", &doc.metadata.comments);
                    context.insert(
                        "page_backlinks",
                        backlinks.get(&source.id).unwrap_or(&vec![]),
                    );
                    if doc.metadata.comments {
                        stylesheets.insert("comments.css".into());
                        let settings = options.discussion.as_ref().ok_or_else(|| {
                            failure(
                                Code::InvalidProperty,
                                "comments require discussion settings",
                            )
                        })?;
                        validate_discussion(settings)?;
                        let term = format!("berlin:{}", source.id.leaf());
                        let mut url = url::Url::parse(&format!(
                            "https://github.com/{}/discussions",
                            settings.repo
                        ))
                        .map_err(|e| failure(Code::UnsafeUrl, e.to_string()))?;
                        url.query_pairs_mut()
                            .append_pair("discussions_q", &format!("\"{term}\""));
                        context.insert("page_discussion_term", &term);
                        context.insert("page_discussion_url", url.as_str());
                    }
                    let original = entry
                        .original_url()
                        .map(str::parse)
                        .transpose()
                        .map_err(|e| failure(Code::UnsafeUrl, format!("source URL: {e}")))?
                        .unwrap_or(
                            mount
                                .join(route)
                                .map_err(|e| failure(Code::UnsafeUrl, e.to_string()))?,
                        );
                    let mut fragments = berlin_document_html::FragmentContext::default();
                    let source_mount = catalog
                        .options()
                        .source_site_url
                        .as_ref()
                        .map(mount_url)
                        .transpose()?
                        .unwrap_or_else(|| mount.clone());
                    walk_blocks(&doc.blocks, &mut |block| {
                        if let Block::Figure(figure) = block {
                            fragments.destinations.insert(
                                figure.source.clone(),
                                resolve_source_url(&figure.source, &original, Some(&source_mount))?
                                    .to_string(),
                            );
                        }
                        visit_block_inlines(block, &mut |inline| {
                            let destination = match inline {
                                Inline::Image { source, .. } => Some(source),
                                Inline::Link { destination, .. }
                                    if !destination.starts_with('#')
                                        && !destination.starts_with("mailto:")
                                        && !destination.starts_with("tel:") =>
                                {
                                    Some(destination)
                                }
                                _ => None,
                            };
                            if let Some(value) = destination {
                                fragments.destinations.insert(
                                    value.clone(),
                                    resolve_source_url(value, &original, Some(&source_mount))?
                                        .to_string(),
                                );
                            }
                            Ok(())
                        })
                    })?;
                    let body = semantic
                        .render_fragment(&doc.blocks, &fragments, None, limits.output_bytes)
                        .map_err(|e| failure(Code::TemplateContextFailure, e.to_string()))?
                        .into_string();
                    self.owned_page("publications/document.tera", Some(body), context, limits)?
                        .into_bytes()
                }
                SiteRoute::Tag { label, resources } => {
                    let children = resources
                        .iter()
                        .map(|r| ResourceCard::builder().resource(r.clone()).build())
                        .collect::<Vec<_>>();
                    let page = PublicationSpec::new(
                        PublicationMetadata {
                            id: PublicationId(format!("{}:tag:{label}", assembly.spec().id.0)),
                            title: label.clone(),
                            description: None,
                        },
                        Portfolio::builder().body(ViewFragment(children)).build(),
                    )?;
                    let page = PublicationAssembly::resolve(&page, catalog.clone(), registry)?;
                    context.insert("tag_name", label);
                    let result = self.render_mounted(
                        &page,
                        registry,
                        site,
                        "publications/site_page.tera",
                        Some(&context),
                        &local_urls,
                    )?;
                    stylesheets.extend(result.stylesheets);
                    pages.insert(route.clone(), result.manifest);
                    result.html.into_bytes()
                }
                SiteRoute::Redirect { target, source } => {
                    context.insert("redirect_target", target);
                    context.insert(
                        "destination",
                        mount
                            .join(target)
                            .map_err(|e| failure(Code::UnsafeUrl, e.to_string()))?
                            .as_str(),
                    );
                    context.insert(
                        "page_title",
                        &catalog.resolve_resource(source)?.descriptor().title,
                    );
                    self.owned_page("publications/redirect.tera", None, context, limits)?
                        .into_bytes()
                }
                SiteRoute::SearchPage => {
                    stylesheets.insert("search.css".into());
                    stylesheets.insert("overview.css".into());
                    context.insert("search_index_version", assembly.composition_fingerprint());
                    self.owned_page("search.tera", None, context, limits)?
                        .into_bytes()
                }
                SiteRoute::SearchIndex { resources } => {
                    search_index(assembly, resources)?.into_bytes()
                }
                SiteRoute::SearchClient => include_bytes!("../../support/web/search.js").to_vec(),
            };
            total_bytes = total_bytes
                .checked_add(bytes.len())
                .ok_or_else(|| failure(Code::ResourceLimit, "site output byte overflow"))?;
            if total_bytes > limits.output_bytes {
                return Err(failure(
                    Code::ResourceLimit,
                    "cumulative rendered site byte limit exceeded",
                ));
            }
            files.insert(route.clone(), bytes);
        }
        let render_input_fingerprint = fingerprint(
            "resource-site-render-input/v1",
            &(
                assembly.composition_fingerprint(),
                catalog.input_digest(),
                catalog.original_urls_digest(),
                &self.input_digests,
                site,
                options,
                &stylesheets,
            ),
        )?;
        let manifest = SiteManifest {
            schema_version: 1,
            site_id: assembly.spec().id.clone(),
            catalog_digest: catalog.input_digest().into(),
            queries: assembly.spec().queries.clone(),
            source_scope: assembly.source_scope_evidence().clone(),
            catalog_inputs: catalog.input_sources().to_vec(),
            output: None,
            composition_fingerprint: assembly.composition_fingerprint().into(),
            render_input_fingerprint,
            routes: files.keys().cloned().collect(),
            pages,
            render_inputs: self.input_digests.clone(),
        };
        Ok(HtmlSite {
            files,
            stylesheets,
            manifest,
        })
    }
    fn owned_page(
        &self,
        template: &str,
        body: Option<String>,
        mut context: Context,
        limits: &PublicationLimits,
    ) -> Result<String> {
        crate::audit::audit(&self.tera, template)
            .map_err(|e| template_error(Code::TemplateContextFailure, template, e))?;
        let store = Arc::new(Mutex::new(SlotStore::new(limits.fragment_bytes)));
        if let Some(body) = body {
            store_slot(&store, "page", "body", body)?;
        }
        let mut tera = self.tera.clone();
        tera.register_function("render_slot", SlotBridge(store.clone()));
        context.insert("view", &json!({"id":"page","heading_level":1}));
        render_template(&tera, template, &context, &store, "page", limits)
    }
}
fn tag_route(tag: &str) -> Result<String> {
    use slugify::slugify;
    let slug = slugify!(tag);
    if slug.is_empty() {
        return Err(failure(Code::UnsafeUrl, "tag has no route"));
    }
    Ok(format!("tags/{slug}.html"))
}
fn base_context(
    assembly: &SiteAssembly,
    site: &PublicationSite,
    options: &SiteHtmlOptions,
    route: &str,
    mount: &url::Url,
) -> Result<Context> {
    let mut c = Context::new();
    c.insert("config_site_url", mount.as_str().trim_end_matches('/'));
    c.insert(
        "title",
        &site.title.as_ref().unwrap_or(&assembly.spec().title),
    );
    c.insert("author", &site.author.as_deref().unwrap_or_default());
    c.insert(
        "description",
        &options.description.as_deref().unwrap_or_default(),
    );
    c.insert("github", &options.github);
    c.insert("linkedin", &options.linkedin);
    c.insert("twitter", &options.twitter);
    c.insert("config_giscus", &options.discussion);
    c.insert("has_search", &assembly.routes().contains_key("search.html"));
    c.insert(
        "page_path",
        &if route == "index.html" {
            "/".into()
        } else {
            format!("/{route}")
        },
    );
    c.insert("og_image_path", "");
    let mut navigation = assembly
        .spec()
        .entries
        .iter()
        .filter_map(|entry| match entry {
            SiteEntrySpec::Composed { route, view, .. } if route != "index.html" => {
                Some(json!({"path":format!("/{route}"),"title":view.metadata.title}))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if assembly.routes().contains_key("search.html") {
        navigation.push(json!({"path":"/search.html","title":"Search"}));
    }
    c.insert("site_navigation", &navigation);
    c.insert(
        "notebook_index_path",
        if assembly.routes().contains_key("notes.html") {
            "/notes.html"
        } else {
            "/"
        },
    );
    Ok(c)
}
fn validate_discussion(settings: &DiscussionSettings) -> Result<()> {
    if settings.repo.split('/').count() != 2
        || settings.repo.split('/').any(|p| {
            p.is_empty()
                || p == "."
                || p == ".."
                || !p
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
    {
        return Err(failure(
            Code::InvalidProperty,
            "invalid discussion repository",
        ));
    }
    if let Some(theme) = &settings.theme {
        let url: url::Url = theme
            .parse()
            .map_err(|e| failure(Code::UnsafeUrl, format!("discussion theme: {e}")))?;
        validate_public_url(&url)?;
        if url.scheme() != "https" {
            return Err(failure(Code::UnsafeUrl, "discussion theme must use HTTPS"));
        }
    }
    Ok(())
}
fn search_index(assembly: &SiteAssembly, resources: &[ResourceRef]) -> Result<String> {
    let catalog = assembly.catalog();
    let mut documents = Vec::new();
    for reference in resources {
        let entry = catalog.resolve_resource(reference)?;
        let descriptor = entry.descriptor();
        let path = if let Some(route) = assembly.document_routes().get(&reference.id) {
            route.clone()
        } else {
            entry
                .original_url()
                .ok_or_else(|| failure(Code::UnsafeUrl, "search resource needs a URL"))?
                .into()
        };
        let mut sections = vec![
            json!({"heading":"","fragment":null,"text":descriptor.description.as_deref().unwrap_or_default()}),
        ];
        if let ResourceHandle::Local(local) = entry.handle() {
            // Only an explicitly selected local search source contributes body text.
            walk_blocks(&local.document().blocks, &mut |block| {
                if let Block::Section { id, title, .. } = block {
                    sections.push(json!({"heading":plain_text(title),"fragment":id.0,"text":""}));
                }
                let text = match block {
                    Block::Paragraph { content } | Block::Heading { content, .. } => {
                        Some(plain_text(content))
                    }
                    Block::Code(code) => Some(format!(
                        "{} {}",
                        code.caption
                            .as_ref()
                            .map(|c| plain_text(c))
                            .unwrap_or_default(),
                        code.value
                    )),
                    Block::Math { tex } => Some(tex.clone()),
                    Block::Equation(equation) => {
                        Some(format!("{} {}", equation.tex, equation.number))
                    }
                    Block::Figure(figure) => figure.caption.clone(),
                    Block::Table(table) => Some(
                        table
                            .rows
                            .iter()
                            .flat_map(|row| row.cells.iter())
                            .map(|cell| plain_text(cell))
                            .collect::<Vec<_>>()
                            .join(" "),
                    ),
                    _ => None,
                };
                if let Some(text) = text {
                    let section = sections.last_mut().expect("initial section");
                    section["text"] = json!(
                        section["text"].as_str().unwrap_or_default().to_owned() + " " + &text
                    );
                }
                Ok(())
            })?;
        }
        documents.push(json!({"title":descriptor.title,"kind":descriptor.kind,"tags":descriptor.topics,"path":path,"sections":sections}));
    }
    let mut tag_paths = BTreeMap::new();
    let mut tag_keys = BTreeMap::new();
    for (route, intention) in assembly.routes() {
        if let SiteRoute::Tag { label, .. } = intention {
            tag_paths.insert(label.clone(), route.clone());
            tag_keys.insert(topic_key(label)?, route);
        }
    }
    for reference in resources {
        for tag in &catalog.resolve_resource(reference)?.descriptor().topics {
            let key = topic_key(tag)?;
            if let Some(route) = tag_keys.get(&key) {
                tag_paths.insert(tag.clone(), (*route).clone());
            }
        }
    }
    serde_json::to_string(&json!({"schema_version":1,"documents":documents,"tag_paths":tag_paths}))
        .map_err(|e| failure(Code::InvalidProperty, e.to_string()))
}
