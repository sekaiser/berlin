use std::collections::BTreeMap;

use crate::{
    Channel, Component, ComponentProperty, ComponentRegistry, ComponentSchema, ContentRef,
    ExcerptRef, Property, PropertySchema, PropertyType, RegistryBuilder, SlotSchema, ViewFragment,
};

macro_rules! scalar {
    ($ty:ty, $kind:ident, $variant:ident) => {
        impl ComponentProperty for $ty {
            fn property_type() -> PropertyType {
                PropertyType::$kind
            }
            fn into_property(self) -> Property {
                Property::$variant(self)
            }
        }
    };
}
scalar!(String, Text, Text);
scalar!(bool, Boolean, Boolean);
scalar!(i64, Integer, Integer);
scalar!(ContentRef, Source, Source);
scalar!(ExcerptRef, Excerpt, Excerpt);
impl<T: ComponentProperty> ComponentProperty for Option<T> {
    fn property_type() -> PropertyType {
        PropertyType::Optional(Box::new(T::property_type()))
    }
    fn default_property() -> Option<Property> {
        Some(Property::None)
    }
    fn into_property(self) -> Property {
        self.map_or(Property::None, T::into_property)
    }
}
impl<T: ComponentProperty> ComponentProperty for Vec<T> {
    fn property_type() -> PropertyType {
        PropertyType::List(Box::new(T::property_type()))
    }
    fn into_property(self) -> Property {
        Property::List(self.into_iter().map(T::into_property).collect())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalloutKind {
    Note,
    Warning,
    Tip,
}
impl ComponentProperty for CalloutKind {
    fn property_type() -> PropertyType {
        PropertyType::Enum(
            ["note", "warning", "tip"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        )
    }
    fn into_property(self) -> Property {
        Property::Text(
            match self {
                Self::Note => "note",
                Self::Warning => "warning",
                Self::Tip => "tip",
            }
            .into(),
        )
    }
}

// One field declaration supplies both schema and typed lowering. P08 replaces
// this local definition helper with the public derive, preserving this contract.
macro_rules! component {
    ($name:ident, $key:literal, $body:literal, { $($field:ident: $ty:ty),* $(,)? }) => {
        pub struct $name { $(pub $field: $ty),* }
        impl Component for $name {
            fn schema() -> ComponentSchema {
                ComponentSchema {
                    key: $key.parse().expect("constant built-in key"),
                    properties: BTreeMap::from([$( (stringify!($field).into(), PropertySchema { kind: <$ty>::property_type(), default: <$ty>::default_property() }) ),*]),
                    slots: if $body { BTreeMap::from([("body".into(), SlotSchema { minimum: 1, maximum: 10_000 })]) } else { BTreeMap::new() },
                    channels: [Channel::Html, Channel::Text].into_iter().collect(),
                }
            }
            fn properties(self) -> BTreeMap<String, Property> { BTreeMap::from([$( (stringify!($field).into(), self.$field.into_property()) ),*]) }
        }
    };
}

component!(Portfolio, "berlin.portfolio@1", true, {});
component!(Section, "berlin.section@1", true, { title: String });
component!(Introduction, "berlin.introduction@1", false, { text: String });
component!(FeaturedWork, "berlin.featured_work@1", false, { source: ContentRef, annotation: Option<String> });
component!(Excerpt, "berlin.excerpt@1", false, { source: ExcerptRef, annotation: Option<String> });
component!(RelatedWork, "berlin.related_work@1", false, { sources: Vec<ContentRef>, title: Option<String> });
component!(Callout, "berlin.callout@1", true, { kind: CalloutKind, title: Option<String> });

impl ComponentRegistry {
    pub fn builtins() -> Self {
        let mut builder = RegistryBuilder::default();
        builder.register::<Portfolio>().expect("built-in schema");
        builder.register::<Section>().expect("built-in schema");
        builder.register::<Introduction>().expect("built-in schema");
        builder.register::<FeaturedWork>().expect("built-in schema");
        builder.register::<Excerpt>().expect("built-in schema");
        builder.register::<RelatedWork>().expect("built-in schema");
        builder.register::<Callout>().expect("built-in schema");
        builder.finish()
    }
}

impl crate::ComponentCall {
    pub fn with_body(mut self, body: impl Into<ViewFragment>) -> Self {
        self.slots.insert("body".into(), body.into());
        self
    }
}
