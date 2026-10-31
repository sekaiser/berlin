//! Local synthetic measurements; not a portable performance threshold.
use berlin_content::DocumentCollection;
use berlin_document::*;
use berlin_publication::*;
use berlin_publication_html::*;
use std::collections::BTreeMap;
use std::time::Instant;

fn templates() -> BTreeMap<String, String> {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes/notebook/shared/pages");
    let mut files = BTreeMap::new();
    files.insert(
        "publications/page.tera".into(),
        std::fs::read_to_string(root.join("publications/page.tera")).unwrap(),
    );
    for extra in [
        "partials/engineering-desk.tera",
        "macros/notebook_entry_layout.tera",
    ] {
        files.insert(
            extra.into(),
            std::fs::read_to_string(root.join(extra)).unwrap(),
        );
    }
    for name in [
        "portfolio",
        "section",
        "introduction",
        "featured_work",
        "excerpt",
        "related_work",
        "callout",
    ] {
        let path = format!("components/berlin/{name}.v1.tera");
        files.insert(
            path.clone(),
            std::fs::read_to_string(root.join(path)).unwrap(),
        );
    }
    files
}
fn document(n: usize) -> Document {
    Document {
        id: ContentId(format!("source-{n}")),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some(format!("Synthetic work {n}")),
            description: Some("A **synthetic** description.".into()),
            ..Default::default()
        },
        blocks: vec![Block::Section {
            id: ComponentId("selected".into()),
            level: 2,
            role: None,
            title: vec![Inline::Text {
                value: "Selected section".into(),
            }],
            blocks: vec![Block::Paragraph {
                content: vec![Inline::Text {
                    value: "Synthetic excerpt body. ".repeat(40),
                }],
            }],
        }],
        relations: vec![],
        provenance: Provenance {
            source: format!("memory:source-{n}"),
            source_format: SourceFormat::Markdown,
            source_hash: "a".repeat(64),
        },
    }
}
fn measure(label: &str, sources: usize, repeats: usize, depth: usize) {
    let started = Instant::now();
    let catalog = PublicationCatalog::new(
        DocumentCollection::new((0..sources).map(document).collect::<Vec<_>>()),
        PublicationOptions {
            source_site_url: Some("https://example.test/notebook".parse().unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(catalog.index_builds(), 1);
    let shared = catalog.clone();
    assert!(std::ptr::eq(
        catalog.entry("source-0").unwrap().document(),
        shared.entry("source-0").unwrap().document()
    ));
    let body = if depth > 0 {
        Introduction::builder()
            .text("Depth specimen.")
            .build()
            .into()
    } else if repeats > 0 {
        ViewFragment(
            (0..repeats)
                .map(|_| {
                    Excerpt::builder()
                        .source(catalog.section_ref("source-0", "selected").unwrap())
                        .build()
                })
                .collect(),
        )
    } else {
        ViewFragment(
            (0..sources)
                .map(|n| {
                    FeaturedWork::builder()
                        .source(ResourceRef::local(&format!("source-{n}")).unwrap())
                        .build()
                })
                .collect(),
        )
    };
    let mut body = body;
    for n in 0..depth {
        body = Callout::builder()
            .kind(CalloutKind::Note)
            .title(Some(format!("Level {n}")))
            .body(body)
            .build()
            .into();
    }
    let root = Portfolio::builder().body(body).build();
    let spec = PublicationSpec {
        schema_version: 1,
        queries: Default::default(),
        metadata: PublicationMetadata {
            id: PublicationId(label.into()),
            title: label.into(),
            description: None,
        },
        root,
    };
    let registry = ComponentRegistry::builtins();
    let assembly = PublicationAssembly::resolve(&spec, catalog, &registry).unwrap();
    let assembled = started.elapsed();
    let renderer = PublicationHtmlRenderer::new(templates(), HtmlBindings::builtins()).unwrap();
    let rendered = renderer
        .render(
            &assembly,
            &registry,
            &PublicationSite {
                url: "https://example.test/portfolio".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(rendered.metrics.template_snapshots_parsed, 1);
    assert_eq!(rendered.metrics.environments_prepared, 1);
    assert_eq!(rendered.metrics.templates_parsed, 10);
    assert_eq!(
        rendered.metrics.components_rendered,
        assembly.placements().len()
    );
    assert_eq!(
        rendered.metrics.source_descriptions_parsed,
        if repeats == 0 && depth == 0 {
            sources
        } else {
            0
        }
    );
    if repeats > 1 {
        assert_eq!(assembly.sources().len(), 1);
        assert_eq!(
            assembly
                .excerpt(&shared.section_ref("source-0", "selected").unwrap())
                .unwrap()
                .blocks()
                .len(),
            1
        );
    }
    println!(
        "{}",
        serde_json::json!({"workload":label,"sources":sources,"occurrences":assembly.placements().len(),"catalog_index_builds":assembly.catalog().index_builds(),"semantic_source_parses":0,"assembly_us":assembled.as_micros(),"total_us":started.elapsed().as_micros(),"output_bytes":rendered.html.len(),"metrics":rendered.metrics})
    );
}
fn main() {
    for n in [10, 100, 1000] {
        measure(&format!("cards-{n}"), n, 0, 0);
    }
    measure("repeated-excerpt-100", 1, 100, 0);
    // Root + thirty non-heading callouts + leaf reaches the default depth of 32.
    measure("depth-32", 1, 0, 30);
}
