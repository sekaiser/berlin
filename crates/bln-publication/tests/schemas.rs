use berlin_publication::*;
use std::collections::BTreeMap;

fn normalize(call: &mut ComponentCall) -> Result<()> {
    ComponentRegistry::builtins().normalize_call(call)
}

#[test]
fn a03_shared_schema_rejects_unknown_fields_types_slots_and_missing_values() {
    let mut section = Section {
        title: "Work".into(),
    }
    .into_call();
    assert_eq!(normalize(&mut section).unwrap_err().code, Code::MissingSlot);
    section.slots.insert("body".into(), ViewFragment::default());
    assert_eq!(normalize(&mut section).unwrap_err().code, Code::EmptySlot);
    section = section.with_body(
        Introduction {
            text: "Hello".into(),
        }
        .into_call(),
    );
    normalize(&mut section).unwrap();
    section
        .properties
        .insert("wrong".into(), Property::Text("bad".into()));
    assert_eq!(
        normalize(&mut section).unwrap_err().code,
        Code::UnknownProperty
    );
    section.properties.remove("wrong");
    section
        .properties
        .insert("title".into(), Property::Boolean(false));
    assert_eq!(
        normalize(&mut section).unwrap_err().code,
        Code::InvalidProperty
    );
    section.properties.clear();
    assert_eq!(
        normalize(&mut section).unwrap_err().code,
        Code::MissingProperty
    );
    section
        .properties
        .insert("title".into(), Property::Text("Work".into()));
    section
        .slots
        .insert("wrong".into(), ViewFragment::default());
    assert_eq!(normalize(&mut section).unwrap_err().code, Code::UnknownSlot);
}

#[test]
fn a02_a04_registration_versions_and_duplicate_contracts() {
    let mut builder = RegistryBuilder::default();
    builder.register::<Introduction>().unwrap();
    assert_eq!(
        builder.register::<Introduction>().unwrap_err().code,
        Code::DuplicateComponent
    );
    let registry = builder.finish();
    assert_eq!(
        registry
            .schema(&"berlin.introduction@2".parse().unwrap())
            .unwrap_err()
            .code,
        Code::UnknownVersion
    );
    assert_eq!(
        registry
            .schema(&"berlin.unknown@1".parse().unwrap())
            .unwrap_err()
            .code,
        Code::UnknownComponent
    );
}

#[test]
fn typed_and_dynamic_components_share_defaults_and_schema() {
    let typed = FeaturedWork {
        source: ResourceRef::local("source").unwrap(),
        annotation: None,
    }
    .into_call();
    let mut dynamic = ComponentCall::new("berlin.featured_work@1".parse().unwrap());
    dynamic.properties.insert(
        "source".into(),
        Property::Resource(ResourceRef::local("source").unwrap()),
    );
    normalize(&mut dynamic).unwrap();
    assert_eq!(dynamic, typed);
    let mut callout = Callout {
        kind: CalloutKind::Warning,
        title: None,
    }
    .into_call()
    .with_body(
        Introduction {
            text: "Careful".into(),
        }
        .into_call(),
    );
    normalize(&mut callout).unwrap();
    callout
        .properties
        .insert("kind".into(), Property::Text("css-class".into()));
    assert_eq!(
        normalize(&mut callout).unwrap_err().code,
        Code::InvalidProperty
    );
}

#[test]
fn e06_channel_support_is_explicit() {
    let mut schema = Introduction::schema();
    schema.channels.remove(&Channel::Text);
    let mut builder = RegistryBuilder::default();
    builder.register_schema(schema).unwrap();
    let registry = builder.finish();
    let spec = PublicationSpec {
        schema_version: 1,
        queries: Default::default(),
        metadata: PublicationMetadata {
            id: PublicationId("example".into()),
            title: "Example".into(),
            description: None,
        },
        root: Introduction {
            text: "text".into(),
        }
        .into_call(),
    };
    registry
        .validate_channel(&spec, Channel::Html, &PublicationLimits::default())
        .unwrap();
    let error = registry
        .validate_channel(&spec, Channel::Text, &PublicationLimits::default())
        .unwrap_err();
    assert_eq!(error.code, Code::UnsupportedChannelComponent);
    assert_eq!(error.publication_id.as_deref(), Some("example"));
}

#[test]
fn b11_registered_semantic_callout_bridge_is_opt_in_and_strict() {
    use berlin_document::{Block, PropertyValue};
    let registry = ComponentRegistry::builtins();
    let blocks = vec![Block::ThematicBreak];
    let mut props = BTreeMap::from([("kind".into(), PropertyValue::Text("note".into()))]);
    registry
        .validate_semantic_component("berlin.callout@1", &props, &blocks)
        .unwrap();
    props.insert("kind".into(), PropertyValue::Text("unknown".into()));
    assert!(
        registry
            .validate_semantic_component("berlin.callout@1", &props, &blocks)
            .is_err()
    );
    assert_eq!(
        registry
            .validate_semantic_component("old-unregistered", &BTreeMap::new(), &blocks)
            .unwrap_err()
            .code,
        Code::UnsupportedChannelComponent
    );
    props.insert("kind".into(), PropertyValue::Text("note".into()));
    assert_eq!(
        registry
            .validate_semantic_component("berlin.callout@1", &props, &[])
            .unwrap_err()
            .code,
        Code::EmptySlot
    );
}
