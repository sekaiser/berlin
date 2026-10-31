use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use berlin_content::DocumentCollection;
use berlin_core::FunctionRef;
use berlin_pipeline_dsl::RhaiPipelineLoader;
use berlin_publication::*;

struct Meter;
thread_local! {
    static LARGE_BYTES: Cell<Option<usize>> = const { Cell::new(None) };
}
// Count only this test thread's large payload allocations, not unrelated tests
// or small interpreter objects. Tracking is enabled after compilation.
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() >= 65_536 {
            let _ = LARGE_BYTES.try_with(|bytes| {
                if let Some(total) = bytes.get() {
                    bytes.set(Some(total.saturating_add(layout.size())));
                }
            });
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Meter = Meter;

#[derive(Component)]
#[component(key = "test.strings@1", channels(html, text))]
struct Strings {
    values: Vec<String>,
}

fn allocations(stop_before_conversion: bool, limits: PublicationLimits) -> (usize, String) {
    let source = format!(
        r#"fn p(c) {{
            let text={:?};
            let values=[];
            for n in 0..8 {{ values.push(text); }}
            if {} {{ throw("control: stop before native conversion"); }}
            let v=view_component("test.strings@1",#{{values:values}},#{{}});
            publication(#{{id:"p",title:"P"}},v)
        }}"#,
        "x".repeat(65_536),
        stop_before_conversion,
    );
    let mut registry = RegistryBuilder::default();
    registry.register::<Strings>().unwrap();
    let loader = RhaiPipelineLoader::with_components(&source, registry.finish()).unwrap();
    let catalog = PublicationCatalog::new(
        DocumentCollection::default(),
        PublicationOptions {
            limits,
            ..Default::default()
        },
    )
    .unwrap();
    LARGE_BYTES.with(|bytes| bytes.set(Some(0)));
    let result = loader.compose_artifact(&FunctionRef::new("p"), catalog);
    let allocated = LARGE_BYTES.with(|bytes| bytes.replace(None).unwrap());
    (
        allocated,
        result.err().expect("fixture must fail").to_string(),
    )
}

#[test]
fn shared_strings_are_rejected_before_native_payload_expansion() {
    for limits in [
        PublicationLimits {
            string_bytes: 1024,
            property_bytes: 1024,
            ..Default::default()
        },
        PublicationLimits {
            string_bytes: 65_536,
            property_bytes: 200_000,
            ..Default::default()
        },
    ] {
        let (control, _) = allocations(true, limits.clone());
        let (actual, error) = allocations(false, limits);
        assert!(error.contains("ResourceLimit"), "{error}");
        // Eight shared 64-KiB strings must not become eight owned copies before
        // refusal. Allow one payload of slack for interpreter error reporting.
        assert!(
            actual <= control + 65_536,
            "native property conversion expanded rejected payload: control={control}, actual={actual}"
        );
    }
}

#[test]
fn preflight_respects_configured_shapes_and_preserves_valid_properties() {
    let source = r#"fn p(c) {
        publication(#{id:"p",title:"P"},
            view_component("test.strings@1",#{values:["a","b"]},#{}))
    }"#;
    for (limits, expected_success) in [
        (PublicationLimits::default(), true),
        (
            PublicationLimits {
                depth: 1,
                ..Default::default()
            },
            false,
        ),
        (
            PublicationLimits {
                query_sources: 1,
                ..Default::default()
            },
            false,
        ),
        (
            PublicationLimits {
                property_bytes: 7,
                ..Default::default()
            },
            false,
        ),
        (
            PublicationLimits {
                string_bytes: 5,
                ..Default::default()
            },
            false,
        ),
    ] {
        let mut registry = RegistryBuilder::default();
        registry.register::<Strings>().unwrap();
        let loader = RhaiPipelineLoader::with_components(source, registry.finish()).unwrap();
        let catalog = PublicationCatalog::new(
            DocumentCollection::default(),
            PublicationOptions {
                limits,
                ..Default::default()
            },
        )
        .unwrap();
        let result = loader.compose_artifact(&FunctionRef::new("p"), catalog);
        if expected_success {
            let artifact = result.unwrap();
            assert_eq!(
                artifact.single().unwrap().spec().root.properties["values"],
                Property::List(vec![Property::Text("a".into()), Property::Text("b".into())])
            );
        } else {
            assert!(result.err().unwrap().to_string().contains("ResourceLimit"));
        }
    }
}
