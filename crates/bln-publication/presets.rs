//! Reusable compositions over the same catalog, component contracts and assemblies
//! as custom Rust/Rhai composers. No filesystem reads or presentation logic.
use crate::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "preset",
    content = "options",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum StandardComposition {
    Notebook(Box<NotebookConfig>),
    ArticleDraft(Box<ArticleDraftConfig>),
}
impl StandardComposition {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Notebook(_) => "notebook",
            Self::ArticleDraft(_) => "article_draft",
        }
    }
    pub fn resolve(
        &self,
        catalog: PublicationCatalog,
        registry: &ComponentRegistry,
    ) -> Result<ComposedArtifact> {
        encoded_size(self, catalog.limits().property_bytes)?;
        match self {
            Self::Notebook(config) => Ok(ComposedArtifact::Site(Arc::new(SiteAssembly::resolve(
                &config.compose(&catalog)?,
                catalog,
                registry,
            )?))),
            Self::ArticleDraft(config) => Ok(ComposedArtifact::Single(Arc::new(
                PublicationAssembly::resolve(&config.compose(&catalog)?, catalog, registry)?,
            ))),
        }
    }
}

/// Defaults cover a notebook's ordinary routes. Optional pages hold short
/// introductions; substantial custom page content belongs in source/template files.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NotebookConfig {
    pub id: String,
    pub title: String,
    pub home_title: String,
    pub documents: ResourceQuery,
    pub reading: ResourceQuery,
    pub document_prefix: String,
    pub notes_route: String,
    pub reading_route: String,
    pub pages: Vec<NotebookPage>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotebookPage {
    pub id: String,
    pub route: String,
    pub title: String,
    #[serde(default)]
    pub introduction: String,
}
fn local_documents() -> ResourceQuery {
    ResourceQuery {
        origins_any: Some(vec![ResourceOrigin::Local]),
        order: ResourceOrder::PublishedDesc,
        ..Default::default()
    }
}
impl Default for NotebookConfig {
    fn default() -> Self {
        Self {
            id: "notebook".into(),
            title: "Notebook".into(),
            home_title: "Notebook".into(),
            documents: local_documents(),
            reading: ResourceQuery {
                origins_any: Some(vec![ResourceOrigin::External]),
                order: ResourceOrder::AddedDesc,
                ..Default::default()
            },
            document_prefix: "notes".into(),
            notes_route: "notes.html".into(),
            reading_route: "feed.html".into(),
            pages: vec![],
        }
    }
}
fn invalid(detail: &str) -> Diagnostic {
    Diagnostic::new(Code::InvalidProperty, Stage::Construction, detail)
}
fn introduction(text: impl Into<String>) -> ComponentCall {
    Introduction { text: text.into() }.into_call()
}
fn publication(id: &str, title: &str, body: Vec<ComponentCall>) -> Result<PublicationSpec> {
    PublicationSpec::new(
        PublicationMetadata {
            id: PublicationId(id.into()),
            title: title.into(),
            description: None,
        },
        Portfolio {}.into_call().with_body(ViewFragment(body)),
    )
}
fn section(title: &str, body: Vec<ComponentCall>) -> ComponentCall {
    Section {
        title: title.into(),
    }
    .into_call()
    .with_body(ViewFragment(body))
}
fn featured(selection: &ResourceSelection) -> Vec<ComponentCall> {
    selection
        .iter()
        .map(|entry| {
            FeaturedWork {
                source: entry.reference(),
                annotation: None,
            }
            .into_call()
        })
        .collect()
}
fn reading_list(title: &str, selection: &ResourceSelection) -> ComponentCall {
    let mut result = ReadingList {
        title: Some(title.into()),
        intro: None,
    }
    .into_call();
    result.slots.insert(
        "items".into(),
        ViewFragment(
            selection
                .iter()
                .map(|entry| {
                    ResourceCard {
                        resource: entry.reference(),
                        annotation: None,
                        presentation: None,
                    }
                    .into_call()
                })
                .collect(),
        ),
    );
    result
}
impl NotebookConfig {
    pub fn compose(&self, catalog: &PublicationCatalog) -> Result<SiteSpec> {
        let limits = catalog.limits();
        encoded_size(self, limits.property_bytes)?;
        let strings = [
            &self.id,
            &self.title,
            &self.home_title,
            &self.document_prefix,
            &self.notes_route,
            &self.reading_route,
        ]
        .into_iter()
        .chain(
            self.pages
                .iter()
                .flat_map(|page| [&page.id, &page.route, &page.title, &page.introduction]),
        );
        if strings
            .into_iter()
            .any(|value| value.len() > limits.string_bytes)
        {
            return Err(Diagnostic::new(
                Code::ResourceLimit,
                Stage::Construction,
                "notebook configuration string budget exceeded",
            ));
        }
        if self.pages.len().saturating_add(5) > limits.occurrences
            || catalog
                .len()
                .max(1)
                .checked_mul(3)
                .is_none_or(|n| n > catalog.options().resource_limits.query_work)
        {
            return Err(Diagnostic::new(
                Code::ResourceLimit,
                Stage::Construction,
                "notebook page or query work budget exceeded",
            ));
        }
        for page in &self.pages {
            validate_site_route(&page.route)?;
        }
        validate_site_route(&self.notes_route)?;
        validate_site_route(&self.reading_route)?;
        validate_site_route(&format!("{}/example.html", self.document_prefix))?;
        let authored = catalog.select(&self.documents)?;
        authored.local_documents()?;
        let reading = catalog.select(&self.reading)?;
        let all = catalog.select(&ResourceQuery {
            order: ResourceOrder::IdAsc,
            ..Default::default()
        })?;
        let guides = authored
            .iter()
            .filter(|entry| entry.descriptor().kind.as_str() == "guide")
            .map(ResourceEntry::reference)
            .collect::<Vec<_>>();
        // Bound expansion before constructing repeated card trees. Assemblies
        // subsequently enforce exact cumulative nodes, properties and routes.
        let nodes = authored
            .len()
            .saturating_mul(2)
            .saturating_add(reading.len().saturating_mul(2))
            .saturating_add(self.pages.len().saturating_mul(2))
            .saturating_add(16)
            .saturating_add(usize::from(!guides.is_empty()));
        if nodes > limits.occurrences {
            return Err(Diagnostic::new(
                Code::ResourceLimit,
                Stage::Construction,
                "notebook component expansion exceeds occurrence budget",
            ));
        }
        let mut result = SiteSpec::new(PublicationId(self.id.clone()), self.title.clone());
        for selected in [&authored, &reading, &all] {
            result.record_selection(selected, limits)?;
        }
        let mut home = vec![];
        if !guides.is_empty() {
            home.push(
                RelatedWork {
                    sources: guides.clone(),
                    title: Some("Start here".into()),
                }
                .into_call(),
            );
        }
        if !authored.is_empty() {
            home.push(section("From the notebook", featured(&authored)));
        }
        home.push(
            RelatedWork {
                sources: all.iter().map(ResourceEntry::reference).collect(),
                title: Some("Follow a thread".into()),
            }
            .into_call(),
        );
        if !reading.is_empty() {
            home.push(reading_list("Worth keeping.", &reading));
        }
        result.add_page(
            "home",
            "index.html",
            publication("home", &self.home_title, home)?,
        );
        let mut notes = vec![];
        if !guides.is_empty() {
            notes.push(
                RelatedWork {
                    sources: guides,
                    title: Some("Start here".into()),
                }
                .into_call(),
            );
        }
        notes.push(if authored.is_empty() {
            introduction("The first notes are taking shape.")
        } else {
            section("Notes", featured(&authored))
        });
        result.add_page(
            "notes",
            &self.notes_route,
            publication("notes", "Notes", notes)?,
        );
        let saved = if reading.is_empty() {
            introduction("No saved references yet.")
        } else {
            reading_list("Reading", &reading)
        };
        result.add_page(
            "reading",
            &self.reading_route,
            publication("reading", "Reading", vec![saved])?,
        );
        for page in &self.pages {
            result.add_page(
                &page.id,
                &page.route,
                publication(
                    &page.id,
                    &page.title,
                    vec![introduction(&page.introduction)],
                )?,
            );
        }
        result.add_document_pages(authored.query().clone(), &self.document_prefix);
        result.add_tag_pages(all.query().clone());
        result.add_search(all.query().clone());
        result.add_redirects(authored.query().clone());
        Ok(result)
    }
}

/// A summary/link placement, not a conversion of an entire article to text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ArticleDraftConfig {
    pub selection: ResourceQuery,
}
impl Default for ArticleDraftConfig {
    fn default() -> Self {
        Self {
            selection: ResourceQuery {
                limit: Some(1),
                ..local_documents()
            },
        }
    }
}
impl ArticleDraftConfig {
    pub fn compose(&self, catalog: &PublicationCatalog) -> Result<PublicationSpec> {
        encoded_size(self, catalog.limits().property_bytes)?;
        let selected = catalog.select(&self.selection)?;
        selected.local_documents()?;
        if selected.len() != 1 {
            return Err(invalid("article_draft requires exactly one local document"));
        }
        let entry = selected.iter().next().expect("one checked result");
        let mut result = publication(
            "linkedin-draft",
            &entry.descriptor().title,
            featured(&selected),
        )?;
        record_selection(&mut result.queries, &selected, catalog.limits())?;
        Ok(result)
    }
}
