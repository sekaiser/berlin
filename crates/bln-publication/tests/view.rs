use berlin_publication::*;

#[derive(Component)]
#[component(
    key = "example.panel@1",
    channels(html, text),
    slot(name = "body", required),
    slot(name = "caption")
)]
pub struct Panel {
    pub title: String,
    pub count: i32,
    pub visible: bool,
    pub explanation: Option<String>,
}

fn spec(root: ViewFragment) -> PublicationSpec {
    PublicationSpec {
        schema_version: 1,
        metadata: PublicationMetadata {
            id: PublicationId("view".into()),
            title: "View".into(),
            description: None,
        },
        root: root.0.into_iter().next().unwrap(),
    }
}
fn helper() -> ViewFragment {
    view! {Introduction(text:"helper")}
}
mod slot {
    pub use berlin_publication::Introduction;
}

#[test]
fn d01_nested_paths_loops_conditions_named_slots_and_helpers() {
    let __berlin_fragment = "caller name stays visible";
    let root = view! {
        berlin_publication::Portfolio() {
            Section(title:"Work",) {
                slot body {
                    for n in 0..2 {
                        if n==0 { Introduction(text:format!("entry {n}"),key:format!("key-{n}")) }
                        else { (helper()) }
                    }
                }
            }
        }
    };
    assert_eq!(__berlin_fragment, "caller name stays visible");
    let spec = spec(root);
    let normalized = ComponentRegistry::builtins()
        .normalize(&spec, &PublicationLimits::default())
        .unwrap();
    assert_eq!(
        normalized
            .occurrences(&PublicationLimits::default())
            .unwrap()
            .len(),
        4
    );
    let custom = view! {Panel(title:"Panel",count:2,visible:true) {slot body {(helper())} slot caption {Introduction(text:"Caption")}}};
    let mut registry = RegistryBuilder::default();
    registry.register::<Panel>().unwrap();
    registry.register::<Introduction>().unwrap();
    registry
        .finish()
        .normalize(&spec_from(custom), &PublicationLimits::default())
        .unwrap();
}
fn spec_from(root: ViewFragment) -> PublicationSpec {
    spec(root)
}

#[test]
fn d03_property_and_key_expressions_run_once_in_source_order() {
    let mut order = vec![];
    let view = view! {Panel(visible:{order.push(1);true},title:{order.push(2);"Title"},key:{order.push(3);"root"},count:{order.push(4);42}) {
        Introduction(text:{order.push(5);"Child"})
    }};
    assert_eq!(order, vec![1, 2, 3, 4, 5]);
    assert_eq!(view.0[0].key.as_deref(), Some("root"));
    assert_eq!(view.0[0].properties["explanation"], Property::None);
}

#[test]
fn data_dependent_empty_slots_and_duplicate_keys_are_runtime_errors() {
    let empty_items: Vec<usize> = vec![];
    let empty = view! {Portfolio(){for _ in empty_items {Introduction(text:"none")}}};
    assert!(
        ComponentRegistry::builtins()
            .normalize(&spec(empty), &PublicationLimits::default())
            .is_err()
    );
    let duplicate = view! {Portfolio(){for _ in 0..2 {Introduction(text:"same",key:"duplicate")}}};
    assert!(
        ComponentRegistry::builtins()
            .normalize(&spec(duplicate), &PublicationLimits::default())
            .is_err()
    );
}

#[test]
fn builders_and_macro_lower_identically() {
    let typed = Portfolio::builder()
        .body(Introduction::builder().text("Intro").build())
        .build();
    assert_eq!(
        typed,
        view! {Portfolio(){Introduction(text:"Intro")}}.0.remove(0)
    );
    let path = view! {Portfolio(){slot::Introduction(text:"namespaced component")}};
    assert_eq!(path.0[0].slots["body"].0.len(), 1);
}

#[test]
fn publication_constructor_accepts_a_typed_view_and_rejects_multiple_roots() {
    let metadata = PublicationMetadata {
        id: PublicationId("p".into()),
        title: "P".into(),
        description: None,
    };
    assert!(
        PublicationSpec::new(
            metadata.clone(),
            view! {Portfolio(){Introduction(text:"Intro")}}
        )
        .is_ok()
    );
    assert!(PublicationSpec::new(metadata.clone(), ViewFragment::default()).is_err());
    assert!(
        PublicationSpec::new(
            metadata,
            view! {Introduction(text:"one") Introduction(text:"two")}
        )
        .is_err()
    );
}

#[test]
fn d02_d04_compiler_contract_and_renamed_runtime() {
    let dependencies = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let library = std::fs::read_dir(&dependencies)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("libberlin_publication-")
                && p.extension().is_some_and(|e| e == "rlib")
        })
        .max_by_key(|p| std::fs::metadata(p).unwrap().modified().unwrap())
        .unwrap();
    let cases = [
        (
            "pass",
            r#"let _=view!{Portfolio(){Introduction(text:"ok")}};"#,
            true,
        ),
        ("unknown_component", r#"let _=view!{Unknown()};"#, false),
        (
            "unknown_property",
            r#"let _=view!{Introduction(other:"x")};"#,
            false,
        ),
        (
            "duplicate_property",
            r#"let _=view!{Introduction(text:"x",text:"y")};"#,
            false,
        ),
        (
            "wrong_type",
            r#"let _=view!{Introduction(text:42)};"#,
            false,
        ),
        ("missing_property", r#"let _=view!{Introduction()};"#, false),
        ("missing_slot", r#"let _=view!{Portfolio()};"#, false),
        (
            "unknown_slot",
            r#"let _=view!{Portfolio(){slot wrong{Introduction(text:"x")}}};"#,
            false,
        ),
        (
            "duplicate_slot",
            r#"let _=view!{Portfolio(){slot body{Introduction(text:"x")}slot body{Introduction(text:"y")}}};"#,
            false,
        ),
        ("html", r#"let _=view!{<div>};"#, false),
        ("syntax", r#"let _=view!{Introduction(text "x")};"#, false),
    ];
    let temp = tempfile::tempdir().unwrap();
    for (name, body, success) in cases {
        let source = temp.path().join(format!("{name}.rs"));
        std::fs::write(&source, format!("use notebook::*;fn main(){{{body}}}")).unwrap();
        let result = std::process::Command::new("rustc")
            .args([
                "--edition=2024",
                "--emit=metadata",
                "--crate-name",
                name,
                "--extern",
            ])
            .arg(format!("notebook={}", library.display()))
            .arg("-L")
            .arg(format!("dependency={}", dependencies.display()))
            .arg("--out-dir")
            .arg(temp.path())
            .arg(&source)
            .output()
            .unwrap();
        assert_eq!(
            result.status.success(),
            success,
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let source = temp.path().join("derive_alias.rs");
    std::fs::write(
        &source,
        r#"use notebook::*;
        #[derive(Component)] #[component(runtime=notebook,key="example.card@1",channels(html,text))]
        pub struct Card {pub text:String}
        fn main(){let _=view!{Card(text:"renamed")};}"#,
    )
    .unwrap();
    let result = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--extern"])
        .arg(format!("notebook={}", library.display()))
        .arg("-L")
        .arg(format!("dependency={}", dependencies.display()))
        .arg("--out-dir")
        .arg(temp.path())
        .arg(source)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
