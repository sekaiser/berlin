use crate::{
    Component, ComponentProperty, ComponentRegistry, ContentRef, ExcerptRef, Property,
    PropertyType, RegistryBuilder, ViewFragment,
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
macro_rules! signed {
    ($($ty:ty),*) => {$(impl ComponentProperty for $ty {
        fn property_type() -> PropertyType {PropertyType::Integer}
        fn into_property(self) -> Property {Property::Integer(self as i64)}
    })*};
}
signed!(i8, i16, i32, isize);
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

#[derive(Component)]
#[component(runtime = crate, key = "berlin.portfolio@1", channels(html, text), slot(name = "body", required))]
pub struct Portfolio {}
#[derive(Component)]
#[component(runtime = crate, key = "berlin.section@1", channels(html, text), slot(name = "body", required))]
pub struct Section {
    pub title: String,
}
#[derive(Component)]
#[component(runtime = crate, key = "berlin.introduction@1", channels(html, text))]
pub struct Introduction {
    pub text: String,
}
#[derive(Component)]
#[component(runtime = crate, key = "berlin.featured_work@1", channels(html, text))]
pub struct FeaturedWork {
    pub source: ContentRef,
    pub annotation: Option<String>,
}
#[derive(Component)]
#[component(runtime = crate, key = "berlin.excerpt@1", channels(html, text))]
pub struct Excerpt {
    pub source: ExcerptRef,
    pub annotation: Option<String>,
}
#[derive(Component)]
#[component(runtime = crate, key = "berlin.related_work@1", channels(html, text))]
pub struct RelatedWork {
    pub sources: Vec<ContentRef>,
    pub title: Option<String>,
}
#[derive(Component)]
#[component(runtime = crate, key = "berlin.callout@1", channels(html, text), slot(name = "body", required))]
pub struct Callout {
    pub kind: CalloutKind,
    pub title: Option<String>,
}

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
