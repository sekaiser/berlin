use berlin_content::DocumentCollection;
use berlin_document::{ContentId, Document, DocumentKind, Metadata, Provenance, SourceFormat};
use berlin_publication::*;
use berlin_publication_html::*;
use std::collections::BTreeMap;

fn templates() -> BTreeMap<String, String> {
    [
        (
            "partials/engineering-desk.tera",
            include_str!("../../../themes/notebook/shared/pages/partials/engineering-desk.tera"),
        ),
        (
            "macros/notebook_entry_layout.tera",
            include_str!("../../../themes/notebook/shared/pages/macros/notebook_entry_layout.tera"),
        ),
        (
            "publications/page.tera",
            include_str!("../../../themes/notebook/shared/pages/publications/page.tera"),
        ),
        (
            "components/berlin/portfolio.v1.tera",
            include_str!(
                "../../../themes/notebook/shared/pages/components/berlin/portfolio.v1.tera"
            ),
        ),
        (
            "components/berlin/section.v1.tera",
            include_str!("../../../themes/notebook/shared/pages/components/berlin/section.v1.tera"),
        ),
        (
            "components/berlin/introduction.v1.tera",
            include_str!(
                "../../../themes/notebook/shared/pages/components/berlin/introduction.v1.tera"
            ),
        ),
        (
            "components/berlin/featured_work.v1.tera",
            include_str!(
                "../../../themes/notebook/shared/pages/components/berlin/featured_work.v1.tera"
            ),
        ),
        (
            "components/berlin/excerpt.v1.tera",
            include_str!("../../../themes/notebook/shared/pages/components/berlin/excerpt.v1.tera"),
        ),
        (
            "components/berlin/related_work.v1.tera",
            include_str!(
                "../../../themes/notebook/shared/pages/components/berlin/related_work.v1.tera"
            ),
        ),
        (
            "components/berlin/callout.v1.tera",
            include_str!("../../../themes/notebook/shared/pages/components/berlin/callout.v1.tera"),
        ),
    ]
    .into_iter()
    .map(|(name, source)| (name.into(), source.into()))
    .collect()
}
fn assembly(opening: &str) -> PublicationAssembly {
    assembly_with_limits(opening, PublicationLimits::default())
}
fn assembly_with_limits(opening: &str, limits: PublicationLimits) -> PublicationAssembly {
    let documents = DocumentCollection::new(vec![Document {
        id: ContentId("source".into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some("Source <title> & work".into()),
            slug: Some("source-work".into()),
            description: Some("Source **description**.".into()),
            tags: vec!["systems".into()],
            ..Default::default()
        },
        blocks: vec![],
        relations: vec![],
        provenance: Provenance {
            source: "file:///private/source.md".into(),
            source_format: SourceFormat::Markdown,
            source_hash: "a".repeat(64),
        },
    }]);
    let catalog = PublicationCatalog::new(
        documents,
        PublicationOptions {
            source_site_url: Some("https://example.test/notebook".parse().unwrap()),
            limits,
            ..Default::default()
        },
    )
    .unwrap();
    let introduction = Introduction {
        text: opening.into(),
    }
    .into_call();
    let card = FeaturedWork {
        source: ResourceRef::local("source").unwrap(),
        annotation: Some("My annotation <&>".into()),
    }
    .into_call();
    let root = Portfolio {}.into_call().with_body(ViewFragment(vec![
        introduction,
        Section {
            title: "Storage".into(),
        }
        .into_call()
        .with_body(card),
        Section {
            title: "Decisions".into(),
        }
        .into_call()
        .with_body(
            Callout {
                kind: CalloutKind::Note,
                title: Some("Scope".into()),
            }
            .into_call()
            .with_body(
                Introduction {
                    text: "A deliberate choice.".into(),
                }
                .into_call(),
            ),
        ),
        RelatedWork {
            sources: vec![ResourceRef::local("source").unwrap()],
            title: Some("Original work".into()),
        }
        .into_call(),
    ]));
    let spec = PublicationSpec {
        schema_version: 1,
        queries: Default::default(),
        metadata: PublicationMetadata {
            id: PublicationId("portfolio".into()),
            title: "Selected work".into(),
            description: None,
        },
        root,
    };
    PublicationAssembly::resolve(&spec, catalog, &ComponentRegistry::builtins()).unwrap()
}
fn site() -> PublicationSite {
    PublicationSite {
        url: "https://example.test/portfolio".into(),
        title: Some("Site identity, not publication title".into()),
        author: Some("Synthetic author".into()),
    }
}
fn render(sources: BTreeMap<String, String>) -> Result<HtmlPublication> {
    PublicationHtmlRenderer::new(sources, HtmlBindings::builtins())?.render(
        &assembly("Opening"),
        &ComponentRegistry::builtins(),
        &site(),
    )
}

#[test]
fn c04_c05_escaped_editorial_text_and_trusted_slots_are_distinct() {
    let renderer = PublicationHtmlRenderer::new(templates(), HtmlBindings::builtins()).unwrap();
    let result = renderer
        .render(
            &assembly("<script>bad('&')</script>\n\nSecond paragraph."),
            &ComponentRegistry::builtins(),
            &site(),
        )
        .unwrap();
    assert!(result.html.contains("&lt;script&gt;"));
    assert!(!result.html.contains("<script>bad"));
    assert!(result.html.contains("Source &lt;title&gt; &amp; work"));
    assert!(result.html.contains("<strong>description</strong>"));
    assert!(!result.html.contains("&lt;strong&gt;description"));
    let decoded = html_escape::decode_html_entities(&result.html);
    assert!(decoded.contains("https://example.test/notebook/notes/source-work.html"));
    assert!(decoded.contains("https://example.test/portfolio/assets/css/portfolio.css"));
    assert!(result.html.contains("<title>Selected work</title>"));
    assert!(!result.html.contains("<title>Site identity"));
    assert!(!result.html.contains("file:///"));
    assert!(result.html.contains("<p>Second paragraph.</p>"));
    assert_eq!(result.stylesheets.len(), 1);
    assert_eq!(result.metrics.environments_prepared, 1);
    assert_eq!(result.metrics.template_snapshots_parsed, 1);
    assert_eq!(result.metrics.components_rendered, 8);
}

#[test]
fn c02_c03_c12_replaceable_templates_and_inheritance_share_one_composition() {
    let original = render(templates()).unwrap();
    let mut sources = templates();
    sources.insert("components/berlin/introduction.v1.tera".into(),"{% extends \"introduction-base.tera\" %}{% import \"macros/test.tera\" as sample %}{% block contents %}{{ sample::paragraph(text=props.text) }}{% endblock contents %}".into());
    sources.insert(
        "introduction-base.tera".into(),
        "<div class=\"alternate-opening\">{% block contents %}{% endblock contents %}</div>".into(),
    );
    sources.insert(
        "macros/test.tera".into(),
        "{% macro paragraph(text) %}<p>{{text}}</p>{% endmacro paragraph %}".into(),
    );
    let alternate = render(sources).unwrap();
    assert!(alternate.html.contains("alternate-opening"));
    assert_ne!(original.html, alternate.html);
    assert_eq!(
        original.manifest.composition_fingerprint,
        alternate.manifest.composition_fingerprint
    );
    assert_ne!(
        original.manifest.render_input_fingerprint,
        alternate.manifest.render_input_fingerprint
    );
}

#[test]
fn c06_c07_c08_slot_lookup_is_explicit_and_enforces_consumption() {
    for (template, code) in [
        (
            "<article><h1>{{publication.title}}</h1></article>",
            Code::SlotOmission,
        ),
        (
            "<h1>{{publication.title}}</h1>{{render_slot(owner=view.id,name=\"body\")}}{{render_slot(owner=view.id,name=\"body\")}}",
            Code::SlotRepetition,
        ),
        (
            "{{render_slot(owner=\"someone-else\",name=\"body\")}}",
            Code::UnknownSlot,
        ),
        (
            "{{render_slot(owner=view.id,name=\"unknown\")}}",
            Code::UnknownSlot,
        ),
        (
            "{{render_slot(owner=view.id,name=\"body\",html=\"<script>bad</script>\")}}",
            Code::TemplateContextFailure,
        ),
    ] {
        let mut sources = templates();
        sources.insert(
            "components/berlin/portfolio.v1.tera".into(),
            template.into(),
        );
        let e = render(sources)
            .err()
            .expect("must reject invalid slot usage");
        assert_eq!(e.code, code, "{e:?}");
        assert_eq!(
            e.template.as_deref(),
            Some("components/berlin/portfolio.v1.tera")
        );
    }
}

#[test]
fn c14_slot_insertion_rejects_attribute_script_style_and_comment_contexts() {
    for template in [
        "<div title=\"{{render_slot(owner=view.id,name=\"body\")}}\"></div>",
        "<script>{{render_slot(owner=view.id,name=\"body\")}}</script>",
        "<style>{{render_slot(owner=view.id,name=\"body\")}}</style>",
        "<!-- {{render_slot(owner=view.id,name=\"body\")}} -->",
    ] {
        let mut sources = templates();
        sources.insert(
            "components/berlin/portfolio.v1.tera".into(),
            template.into(),
        );
        let outcome = render(sources);
        assert!(outcome.is_err(), "slot insertion unexpectedly succeeded");
        assert_eq!(outcome.err().unwrap().code, Code::TemplateContextFailure);
    }
}

#[test]
fn c15_ambient_functions_and_safe_filter_are_disabled_in_new_sessions() {
    for expression in [
        "now()",
        "get_random(start=0,end=10)",
        "get_env(name=\"HOME\")",
        "props.text | safe",
    ] {
        let mut sources = templates();
        sources.insert(
            "components/berlin/introduction.v1.tera".into(),
            format!("<p>{{{{{expression}}}}}</p>"),
        );
        let outcome = render(sources);
        assert!(outcome.is_err(), "{expression} unexpectedly succeeded");
        assert_eq!(
            outcome.err().unwrap().code,
            Code::TemplateContextFailure,
            "{expression}"
        );
    }
}

#[test]
fn c09_two_concurrent_sessions_cannot_observe_each_others_fragments() {
    let renderer = PublicationHtmlRenderer::new(templates(), HtmlBindings::builtins()).unwrap();
    let registry = ComponentRegistry::builtins();
    let first = assembly("FIRST UNIQUE SESSION");
    let second = assembly("SECOND UNIQUE SESSION");
    let site = site();
    std::thread::scope(|scope| {
        let a = scope.spawn(|| renderer.render(&first, &registry, &site).unwrap());
        let b = scope.spawn(|| renderer.render(&second, &registry, &site).unwrap());
        let a = a.join().unwrap();
        let b = b.join().unwrap();
        assert!(a.html.contains("FIRST UNIQUE SESSION"));
        assert!(!a.html.contains("SECOND UNIQUE SESSION"));
        assert!(b.html.contains("SECOND UNIQUE SESSION"));
        assert!(!b.html.contains("FIRST UNIQUE SESSION"));
    });
}

#[test]
fn c11_missing_and_broken_templates_fail_explicitly() {
    let mut sources = templates();
    sources.remove("components/berlin/section.v1.tera");
    assert_eq!(render(sources).err().unwrap().code, Code::MissingTemplate);
    let mut sources = templates();
    sources.insert(
        "components/berlin/section.v1.tera".into(),
        "{% broken syntax %}".into(),
    );
    assert_eq!(
        render(sources).err().unwrap().code,
        Code::TemplateContextFailure
    );
}

#[test]
fn f17_output_and_fragment_budgets_fail_before_success() {
    let renderer = PublicationHtmlRenderer::new(templates(), HtmlBindings::builtins()).unwrap();
    for limits in [
        PublicationLimits {
            output_bytes: 64,
            ..Default::default()
        },
        PublicationLimits {
            fragment_bytes: 32,
            ..Default::default()
        },
    ] {
        let result = renderer.render(
            &assembly_with_limits("A sufficiently long opening.", limits),
            &ComponentRegistry::builtins(),
            &site(),
        );
        assert_eq!(result.err().unwrap().code, Code::ResourceLimit);
    }
}

#[test]
fn f17_output_and_retained_fragment_limits_accept_the_exact_boundary() {
    let renderer = PublicationHtmlRenderer::new(templates(), HtmlBindings::builtins()).unwrap();
    let baseline = renderer
        .render(
            &assembly("A long enough opening to dominate markers."),
            &ComponentRegistry::builtins(),
            &site(),
        )
        .unwrap();
    for (output_bytes, fragment_bytes) in [
        (
            baseline.html.len(),
            PublicationLimits::default().fragment_bytes,
        ),
        (
            PublicationLimits::default().output_bytes,
            baseline.metrics.peak_fragment_bytes,
        ),
    ] {
        let limits = PublicationLimits {
            output_bytes,
            fragment_bytes,
            ..Default::default()
        };
        assert!(
            renderer
                .render(
                    &assembly_with_limits(
                        "A long enough opening to dominate markers.",
                        limits.clone()
                    ),
                    &ComponentRegistry::builtins(),
                    &site()
                )
                .is_ok()
        );
        let below = if output_bytes == baseline.html.len() {
            PublicationLimits {
                output_bytes: output_bytes - 1,
                ..limits
            }
        } else {
            PublicationLimits {
                fragment_bytes: fragment_bytes - 1,
                ..limits
            }
        };
        assert_eq!(
            renderer
                .render(
                    &assembly_with_limits("A long enough opening to dominate markers.", below),
                    &ComponentRegistry::builtins(),
                    &site()
                )
                .err()
                .unwrap()
                .code,
            Code::ResourceLimit
        );
    }
}

#[test]
fn c03_actual_notebook_macros_work_and_unused_legacy_templates_remain_compatible() {
    let mut sources = templates();
    sources.insert(
        "macros/tag.tera".into(),
        include_str!("../../../themes/notebook/shared/pages/macros/tag.tera").into(),
    );
    // This file is outside the selected presentation's dependency closure.
    sources.insert("legacy.tera".into(), "{{ old_document | safe }}".into());
    sources.insert("components/berlin/introduction.v1.tera".into(),"{% import \"macros/tag.tera\" as tags %}{{ tags::tag(label=\"rust\",target=\"/tags/rust.html\") }}<p>{{props.text}}</p>".into());
    // The adapter bridges the one legacy layout constant used by this macro;
    // it does not expose arbitrary project or process context.
    let result = render(sources).unwrap();
    assert!(result.html.contains("data-tag-color=\"39\""));
}

#[test]
fn c13_stylesheets_must_be_present_once_in_the_page_head() {
    for wrapper in [
        "<html><body>{{render_slot(owner=view.id,name=\"body\")}}</body></html>",
        "<html><head>{% for s in site.stylesheets %}<link rel=\"stylesheet\" href=\"{{s}}\"><link rel=\"stylesheet\" href=\"{{s}}\">{% endfor %}</head><body>{{render_slot(owner=view.id,name=\"body\")}}</body></html>",
    ] {
        let mut sources = templates();
        sources.insert("publications/page.tera".into(), wrapper.into());
        assert_eq!(
            render(sources).err().unwrap().code,
            Code::TemplateContextFailure
        );
    }
}

#[test]
fn p05_nested_source_callout_renders_through_the_same_session_slots() {
    use berlin_document::{Block, ComponentId, Inline, PropertyValue};
    let document = Document {
        id: ContentId("original".into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some("Source".into()),
            ..Default::default()
        },
        blocks: vec![Block::Section {
            id: ComponentId("section".into()),
            level: 2,
            role: None,
            title: vec![Inline::Text {
                value: "An excerpt".into(),
            }],
            blocks: vec![Block::Component {
                id: ComponentId("notice".into()),
                name: "berlin.callout@1".into(),
                properties: BTreeMap::from([("kind".into(), PropertyValue::Text("tip".into()))]),
                blocks: vec![Block::Paragraph {
                    content: vec![Inline::Text {
                        value: "Source callout body <&>".into(),
                    }],
                }],
            }],
        }],
        relations: vec![],
        provenance: Provenance {
            source: "file:///private/original.org".into(),
            source_format: SourceFormat::Org,
            source_hash: "b".repeat(64),
        },
    };
    let catalog = PublicationCatalog::new(
        DocumentCollection::new(vec![document]),
        PublicationOptions {
            source_site_url: Some("https://source.example/notebook".parse().unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    let excerpt = Excerpt {
        source: catalog.section_ref("original", "section").unwrap(),
        annotation: None,
    }
    .into_call();
    let spec = PublicationSpec {
        schema_version: 1,
        queries: Default::default(),
        metadata: PublicationMetadata {
            id: PublicationId("excerpt".into()),
            title: "Source excerpts".into(),
            description: None,
        },
        root: Portfolio {}.into_call().with_body(ViewFragment(vec![
            excerpt.clone().keyed("one"),
            excerpt.keyed("two"),
        ])),
    };
    let assembly =
        PublicationAssembly::resolve(&spec, catalog, &ComponentRegistry::builtins()).unwrap();
    let result = PublicationHtmlRenderer::new(templates(), HtmlBindings::builtins())
        .unwrap()
        .render(&assembly, &ComponentRegistry::builtins(), &site())
        .unwrap();
    assert_eq!(result.html.matches("publication-callout--tip").count(), 2);
    assert_eq!(
        result
            .html
            .matches("Source callout body &lt;&amp;&gt;")
            .count(),
        2
    );
    assert!(
        html_escape::decode_html_entities(&result.html)
            .contains("https://source.example/notebook/notes/source.html")
    );
}
