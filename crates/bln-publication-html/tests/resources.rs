use berlin_content::DocumentCollection;
use berlin_document::{ContentId, Document, DocumentKind, Metadata, Provenance, SourceFormat};
use berlin_publication::*;
use berlin_publication_html::*;
use std::collections::BTreeMap;

fn templates() -> BTreeMap<String, String> {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes/notebook/shared/pages");
    let mut templates = BTreeMap::new();
    for name in [
        "partials/engineering-desk.tera",
        "macros/notebook_entry_layout.tera",
        "publications/page.tera",
        "components/berlin/portfolio.v1.tera",
        "components/berlin/resource_card.v1.tera",
        "components/berlin/citation.v1.tera",
        "components/berlin/reading_list.v1.tera",
        "components/berlin/resource_group.v1.tera",
        "components/berlin/featured_work.v1.tera",
        "components/berlin/related_work.v1.tera",
        "components/berlin/introduction.v1.tera",
    ] {
        templates.insert(
            name.into(),
            std::fs::read_to_string(root.join(name)).unwrap(),
        );
    }
    templates
}
fn assembly() -> PublicationAssembly {
    let doc = Document {
        id: ContentId("local".into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some("Local work".into()),
            ..Default::default()
        },
        blocks: vec![],
        relations: vec![],
        provenance: Provenance {
            source: "file:///private/sentinel.md".into(),
            source_hash: "a".repeat(64),
            source_format: SourceFormat::Markdown,
        },
    };
    let external = ExternalResourceCollection::new(vec![(serde_json::from_value(serde_json::json!({
        "id":"repo", "kind":"repository", "url":"https://example.test/repo?a=1&b=2#Part",
        "title":"Repository <script>alert(1)</script>","description":"<img src=x onerror=alert(1)> **literal**",
        "authors":["Author <svg onload=evil>"],"topics":["Systems"],
        "preview":{"url":"https://example.test/preview.png", "alt":"An authored preview"}
    })).unwrap(),ResourceSource{namespace:"reading".into(),logical_name:"resources.yaml".into(),sha256:"b".repeat(64),record:1}),
    (serde_json::from_value(serde_json::json!({"id":"paper","kind":"paper","url":"https://example.test/paper","title":"Paper without optional metadata"})).unwrap(),
    ResourceSource{namespace:"reading".into(),logical_name:"resources.yaml".into(),sha256:"b".repeat(64),record:2})],&ResourceLimits::default(),&PublicationLimits::default()).unwrap();
    let catalog = PublicationCatalog::with_resources(
        DocumentCollection::new(vec![doc]),
        external,
        PublicationOptions {
            source_site_url: Some("https://example.test/notebook".parse().unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    let repo = ResourceRef::external("repo").unwrap();
    let paper = ResourceRef::external("paper").unwrap();
    let root = view! {Portfolio(){
        ReadingList(title:Some("Reading".into()),intro:Some("Authored selection".into())) {
            slot items {
                ResourceCard(resource:repo.clone(),annotation:Some("First <annotation>".into()),key:"first") {
                    Introduction(text:"Card body")
                }
                ResourceCard(resource:repo.clone(),annotation:Some("Second annotation".into()),key:"second")
            }
        }
        ResourceGroup(title:Some("Sources".into()),kind_hint:Some("paper".parse::<ResourceKind>().unwrap())) {
            slot items {
                Citation(resource:paper)
                Citation(resource:repo.clone(),style:Some(CitationStyle::Short))
            }
        }
        FeaturedWork(source:repo.clone(),annotation:Some("External feature".into()))
        FeaturedWork(source:ResourceRef::local("local").unwrap())
        RelatedWork(sources:vec![repo,ResourceRef::local("local").unwrap()])
    }};
    PublicationAssembly::resolve(
        &PublicationSpec::new(
            PublicationMetadata {
                id: PublicationId("mixed".into()),
                title: "Mixed work".into(),
                description: None,
            },
            root,
        )
        .unwrap(),
        catalog,
        &ComponentRegistry::builtins(),
    )
    .unwrap()
}
fn render(
    templates: BTreeMap<String, String>,
    assembly: &PublicationAssembly,
) -> Result<HtmlPublication> {
    PublicationHtmlRenderer::new(templates, HtmlBindings::builtins())?.render(
        assembly,
        &ComponentRegistry::builtins(),
        &PublicationSite {
            url: "https://example.test/portfolio".into(),
            ..Default::default()
        },
    )
}

#[test]
fn e01_e02_e03_e07_e13_e14_e15_mixed_resources_render_in_html_and_direct_text() {
    let assembly = assembly();
    assert_eq!(assembly.catalog().len(), 3);
    let result = render(templates(), &assembly).unwrap();
    for text in [
        "First &lt;annotation&gt;",
        "Second annotation",
        "&lt;script&gt;alert(1)&lt;&#x2F;script&gt;",
        "&lt;svg onload=evil&gt;",
        "&lt;img src=x onerror=alert(1)&gt;",
        "**literal**",
        "Card body",
        "Paper without optional metadata",
        "An authored preview",
    ] {
        assert!(
            result.html.contains(text),
            "missing {text}: {}",
            result.html
        );
    }
    assert!(!result.html.contains("<script>alert"));
    assert!(!result.html.contains("file:///"));
    assert!(
        html_escape::decode_html_entities(&result.html)
            .contains("https://example.test/notebook/notes/local-work.html")
    );
    assert_eq!(result.stylesheets.len(), 1);
    assert_eq!(assembly.sources().len(), 3);
    let repeated = assembly
        .placements()
        .iter()
        .filter(|p| p.occurrence.component.to_string() == "berlin.resource_card@1")
        .collect::<Vec<_>>();
    assert_eq!(repeated.len(), 2);
    assert_ne!(repeated[0].occurrence.id, repeated[1].occurrence.id);
    let plain = berlin_publication_text::render(
        &assembly,
        &ComponentRegistry::builtins(),
        &Default::default(),
    )
    .unwrap();
    for text in [
        "https://example.test/repo?a=1&b=2#Part",
        "First <annotation>",
        "Second annotation",
        "Card body",
        "Paper without optional metadata",
        "**literal**",
    ] {
        assert!(plain.text.contains(text), "missing {text}: {}", plain.text);
    }
    assert!(!plain.text.contains("&lt;"));
    let linkedin = berlin_publication_text::render(
        &assembly,
        &ComponentRegistry::builtins(),
        &TextConfig {
            profile: TextProfile::LinkedInDraft,
        },
    )
    .unwrap();
    assert_eq!(plain.text, linkedin.text);
    assert_eq!(linkedin.manifest.profile, TextProfile::LinkedInDraft);
}

#[test]
fn e06_e10_closed_kind_variants_fallback_and_override_leave_composition_unchanged() {
    let assembly = assembly();
    let first = render(templates(), &assembly).unwrap();
    let mut skin = templates();
    skin.insert("components/berlin/resource_card.repository.v1.tera".into(), "<article class=\"repository-variant\">{{ props.resource.title }}{{ render_slot(owner=view.id,name=\"body\") }}</article>".into());
    let second = render(skin.clone(), &assembly).unwrap();
    assert!(second.html.contains("repository-variant"));
    assert_ne!(first.html, second.html);
    assert_eq!(
        first.manifest.composition_fingerprint,
        second.manifest.composition_fingerprint
    );
    assert_ne!(
        first.manifest.render_input_fingerprint,
        second.manifest.render_input_fingerprint
    );
    skin.insert(
        "components/berlin/resource_card.repository.v1.tera".into(),
        "<script>{{ render_slot(owner=view.id,name=\"body\") }}</script>".into(),
    );
    assert!(render(skin, &assembly).is_err());
}

#[test]
fn e08_e09_resource_slots_are_owned_once_and_concurrent_sessions_are_isolated() {
    let first = std::thread::spawn(|| render(templates(), &assembly()).unwrap().html);
    let assembly = assembly();
    let second = render(templates(), &assembly).unwrap().html;
    assert_eq!(first.join().unwrap(), second);
    let mut omitted = templates();
    omitted.insert(
        "components/berlin/resource_card.v1.tera".into(),
        "<p>{{ props.resource.title }}</p>".into(),
    );
    assert!(render(omitted, &assembly).is_err());
    let mut repeated = templates();
    repeated.insert("components/berlin/resource_card.v1.tera".into(),"{{ render_slot(owner=view.id,name=\"body\") }}{{ render_slot(owner=view.id,name=\"body\") }}".into());
    assert!(render(repeated, &assembly).is_err());
}
