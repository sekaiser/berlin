//! Synthetic in-process stage measurements, with attributed Rust allocator counters.
//! Run in release mode; no authoring parser, CSS compiler, network or deployment.
use berlin_content::DocumentCollection;
use berlin_document::*;
use berlin_publication::*;
use berlin_publication_html::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

struct Tracked;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);
fn allocated(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(live, Ordering::Relaxed);
    CALLS.fetch_add(1, Ordering::Relaxed);
}
unsafe impl GlobalAlloc for Tracked {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            allocated(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            allocated(layout.size());
        }
        p
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, layout, size) };
        if !q.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            allocated(size);
        }
        q
    }
}
#[global_allocator]
static ALLOCATOR: Tracked = Tracked;

fn templates() -> BTreeMap<String, String> {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes/notebook/shared/pages");
    let mut files = BTreeMap::new();
    for name in [
        "publications/page.tera",
        "partials/engineering-desk.tera",
        "macros/notebook_entry_layout.tera",
        "components/berlin/portfolio.v1.tera",
        "components/berlin/resource_card.v1.tera",
        "components/berlin/featured_work.v1.tera",
        "components/berlin/excerpt.v1.tera",
    ] {
        files.insert(
            name.into(),
            std::fs::read_to_string(root.join(name)).unwrap(),
        );
    }
    files
}
fn local_document() -> Document {
    Document {
        id: ContentId("source".into()),
        kind: DocumentKind::Article,
        metadata: Metadata {
            title: Some("Synthetic local work".into()),
            description: Some("A synthetic description.".into()),
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
                    value: "An explicitly synthetic excerpt. ".repeat(20),
                }],
            }],
        }],
        relations: vec![],
        provenance: Provenance {
            source: "memory:source".into(),
            source_format: SourceFormat::Markdown,
            source_hash: "a".repeat(64),
        },
    }
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let count: usize = args.first().expect("catalog size").parse().unwrap();
    let placements: usize = args.get(1).expect("placements").parse().unwrap();
    let kind = args.get(2).map(String::as_str).unwrap_or("cards");
    let repetitions: usize = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(3);
    assert!((2..=10_000).contains(&count));
    assert!((1..=1_000).contains(&placements));
    for repetition in 0..repetitions {
        let baseline = LIVE.load(Ordering::Relaxed);
        PEAK.store(baseline, Ordering::Relaxed);
        let calls = CALLS.load(Ordering::Relaxed);
        let begin = Instant::now();
        let records=(0..count-1).map(|n|(serde_json::from_value(serde_json::json!({"id":format!("r{n}"),"kind":"paper","url":format!("https://example.test/{n}"),"title":format!("R{n}"),"topics":["s"]})).unwrap(),ResourceSource{namespace:"bench".into(),logical_name:"resources.yaml".into(),sha256:"b".repeat(64),record:n+1})).collect();
        let resources = ExternalResourceCollection::new(
            records,
            &ResourceLimits::default(),
            &PublicationLimits::default(),
        )
        .unwrap();
        let ingestion = begin.elapsed();
        let started = Instant::now();
        let catalog = PublicationCatalog::with_resources(
            DocumentCollection::new(vec![local_document()]),
            resources,
            PublicationOptions {
                source_site_url: Some("https://example.test/notebook".parse().unwrap()),
                ..Default::default()
            },
        )
        .unwrap();
        let catalog_time = started.elapsed();
        assert_eq!(catalog.len(), count);
        assert_eq!(catalog.index_builds(), 1);
        let started = Instant::now();
        let selected = catalog
            .select(&ResourceQuery {
                origins_any: Some(vec![ResourceOrigin::External]),
                topics_all: vec!["s".into()],
                order: ResourceOrder::IdAsc,
                limit: Some(placements.min(count - 1)),
                ..Default::default()
            })
            .unwrap();
        let query_time = started.elapsed();
        let started = Instant::now();
        let nodes = (0..placements)
            .map(|n| match kind {
                "cards" => ResourceCard::builder()
                    .resource(ResourceRef::new(selected.ids()[n % selected.len()].clone()))
                    .build(),
                "repeated-resource" => ResourceCard::builder()
                    .resource(ResourceRef::new(selected.ids()[0].clone()))
                    .annotation(Some(format!("Placement {n}")))
                    .build(),
                "repeated-excerpt" => Excerpt::builder()
                    .source(catalog.section_ref("source", "selected").unwrap())
                    .build(),
                _ => panic!("unknown workload"),
            })
            .collect();
        let spec = PublicationSpec::new(
            PublicationMetadata {
                id: PublicationId("scale".into()),
                title: "Synthetic scaling workload".into(),
                description: None,
            },
            Portfolio::builder().body(ViewFragment(nodes)).build(),
        )
        .unwrap()
        .with_selection(&selected, catalog.limits())
        .unwrap();
        let registry = ComponentRegistry::builtins();
        let assembly = PublicationAssembly::resolve(&spec, catalog, &registry).unwrap();
        let composition_time = started.elapsed();
        let started = Instant::now();
        let renderer = PublicationHtmlRenderer::new(templates(), HtmlBindings::builtins()).unwrap();
        let preparation_time = started.elapsed();
        let started = Instant::now();
        let html = renderer
            .render(
                &assembly,
                &registry,
                &PublicationSite {
                    url: "https://example.test/portfolio".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let html_time = started.elapsed();
        let started = Instant::now();
        let text =
            berlin_publication_text::render(&assembly, &registry, &TextConfig::default()).unwrap();
        let text_time = started.elapsed();
        let peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
        let allocations = CALLS.load(Ordering::Relaxed) - calls;
        println!(
            "{}",
            serde_json::json!({"kind":kind,"repetition":repetition+1,"catalog_items":count,"placements":placements,"selected_resources":selected.len(),"examined_candidates":selected.examined_candidates(),"catalog_index_builds":assembly.catalog().index_builds(),"ingestion_us":ingestion.as_micros(),"catalog_us":catalog_time.as_micros(),"query_us":query_time.as_micros(),"composition_us":composition_time.as_micros(),"template_prepare_us":preparation_time.as_micros(),"html_us":html_time.as_micros(),"text_us":text_time.as_micros(),"html_bytes":html.html.len(),"text_bytes":text.text.len(),"allocator_peak_extra_live_bytes":peak,"allocator_calls":allocations,"metrics":html.metrics})
        );
    }
}
