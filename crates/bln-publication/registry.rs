use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    Code, ComponentCall, ComponentKey, Diagnostic, Property, PublicationLimits, PublicationSpec,
    Result, Stage,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Html,
    Text,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum PropertyType {
    Text,
    Boolean,
    Integer,
    Source,
    Resource,
    ResourceKind,
    Excerpt,
    Enum(Vec<String>),
    Optional(Box<PropertyType>),
    List(Box<PropertyType>),
}

impl PropertyType {
    pub fn accepts(&self, property: &Property) -> bool {
        match (self, property) {
            (Self::Text, Property::Text(_))
            | (Self::Boolean, Property::Boolean(_))
            | (Self::Integer, Property::Integer(_))
            | (Self::Source, Property::Source(_))
            | (Self::Resource, Property::Resource(_))
            | (Self::Excerpt, Property::Excerpt(_)) => true,
            (Self::Enum(variants), Property::Text(text)) => variants.contains(text),
            (Self::ResourceKind, Property::Text(text)) => {
                text.parse::<crate::ResourceKind>().is_ok()
            }
            (Self::Optional(_), Property::None) => true,
            (Self::Optional(inner), value) => inner.accepts(value),
            (Self::List(inner), Property::List(values)) => values.iter().all(|v| inner.accepts(v)),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PropertySchema {
    pub kind: PropertyType,
    pub default: Option<Property>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SlotSchema {
    pub minimum: usize,
    pub maximum: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComponentSchema {
    pub key: ComponentKey,
    pub properties: BTreeMap<String, PropertySchema>,
    pub slots: BTreeMap<String, SlotSchema>,
    pub channels: BTreeSet<Channel>,
}

/// Deliberate schema participation, shared by typed components and dynamic
/// decoding. Implementations must not treat serialization as schema inference.
pub trait ComponentProperty {
    fn property_type() -> PropertyType;
    fn default_property() -> Option<Property> {
        None
    }
    fn into_property(self) -> Property;
}

/// A native component's contract. HTML bindings live in the HTML crate.
pub trait Component: Sized {
    fn schema() -> ComponentSchema;
    fn properties(self) -> BTreeMap<String, Property>;
    fn into_call(self) -> ComponentCall {
        let mut call = ComponentCall::new(Self::schema().key);
        call.properties = self.properties();
        call
    }
}

/// Build explicitly, then freeze. Render-time registration is not supported.
#[derive(Default)]
pub struct RegistryBuilder {
    schemas: BTreeMap<ComponentKey, ComponentSchema>,
}

#[derive(Clone, Debug)]
pub struct ComponentRegistry {
    schemas: BTreeMap<ComponentKey, ComponentSchema>,
}

impl RegistryBuilder {
    pub fn register<C: Component>(&mut self) -> Result<()> {
        self.register_schema(C::schema())
    }
    pub fn register_schema(&mut self, schema: ComponentSchema) -> Result<()> {
        if self.schemas.contains_key(&schema.key) {
            return Err(Diagnostic::new(
                Code::DuplicateComponent,
                Stage::Construction,
                format!("component '{}' already registered", schema.key),
            ));
        }
        if schema.slots.len() > PublicationLimits::default().slots {
            return Err(Diagnostic::new(
                Code::ResourceLimit,
                Stage::Construction,
                "component schema has too many slots",
            ));
        }
        for (name, property) in &schema.properties {
            crate::model::identifier(name)?;
            if ["props", "publication", "site", "view", "slots", "key"].contains(&name.as_str()) {
                return Err(Diagnostic::new(
                    Code::InvalidProperty,
                    Stage::Construction,
                    format!("reserved property '{name}'"),
                ));
            }
            if let Some(default) = &property.default
                && !property.kind.accepts(default)
            {
                return Err(Diagnostic::new(
                    Code::InvalidProperty,
                    Stage::Construction,
                    format!("invalid default for '{name}'"),
                ));
            }
        }
        for (name, slot) in &schema.slots {
            crate::model::identifier(name)?;
            if slot.minimum > slot.maximum
                || slot.maximum > PublicationLimits::default().occurrences
            {
                return Err(Diagnostic::new(
                    Code::InvalidProperty,
                    Stage::Construction,
                    format!("invalid cardinality for slot '{name}'"),
                ));
            }
        }
        self.schemas.insert(schema.key.clone(), schema);
        Ok(())
    }
    pub fn finish(self) -> ComponentRegistry {
        ComponentRegistry {
            schemas: self.schemas,
        }
    }
}

impl ComponentRegistry {
    pub fn schema(&self, key: &ComponentKey) -> Result<&ComponentSchema> {
        self.schemas.get(key).ok_or_else(|| {
            let code = if self
                .schemas
                .keys()
                .any(|k| k.namespace() == key.namespace() && k.name() == key.name())
            {
                Code::UnknownVersion
            } else {
                Code::UnknownComponent
            };
            Diagnostic::new(
                code,
                Stage::Construction,
                format!("no registered contract for '{key}'"),
            )
        })
    }

    /// Returns a normalized copy with explicit defaults. Validates the logical
    /// tree before recursion; shared native fragment reuse cannot hide its size.
    pub fn normalize(
        &self,
        spec: &PublicationSpec,
        limits: &PublicationLimits,
    ) -> Result<PublicationSpec> {
        let occurrences = spec.occurrences(limits)?;
        let mut normalized = spec.clone();
        let mut occurrences = occurrences.iter();
        fn visit(
            registry: &ComponentRegistry,
            call: &mut ComponentCall,
            occurrences: &mut std::slice::Iter<'_, crate::Occurrence>,
            publication: &str,
        ) -> Result<()> {
            let occurrence = occurrences.next().ok_or_else(|| {
                Diagnostic::new(
                    Code::InvalidProperty,
                    Stage::Construction,
                    "occurrence traversal mismatch",
                )
            })?;
            registry
                .normalize_call(call)
                .map_err(|e| e.at(publication, &occurrence.path))?;
            for children in call.slots.values_mut() {
                for child in &mut children.0 {
                    visit(registry, child, occurrences, publication)?;
                }
            }
            Ok(())
        }
        visit(
            self,
            &mut normalized.root,
            &mut occurrences,
            &spec.metadata.id.0,
        )?;
        // Defaults consume budgets too, including optional empty slots.
        normalized.occurrences(limits)?;
        Ok(normalized)
    }

    pub fn normalize_call(&self, call: &mut ComponentCall) -> Result<()> {
        let counts = call
            .slots
            .iter()
            .map(|(name, children)| (name.clone(), children.0.len()))
            .collect();
        self.normalize_parts(&call.component, &mut call.properties, &counts)?;
        for name in self.schema(&call.component)?.slots.keys() {
            call.slots.entry(name.clone()).or_default();
        }
        Ok(())
    }

    /// Shared validation for both owned trees and native shared fragment graphs.
    /// Slot counts avoid expanding a reused child merely to validate its parent.
    pub fn normalize_parts(
        &self,
        component: &ComponentKey,
        properties: &mut BTreeMap<String, Property>,
        slots: &BTreeMap<String, usize>,
    ) -> Result<()> {
        let schema = self.schema(component)?;
        for name in properties.keys() {
            if !schema.properties.contains_key(name) {
                return Err(Diagnostic::new(
                    Code::UnknownProperty,
                    Stage::Construction,
                    format!("unknown property '{name}' for {component}"),
                ));
            }
        }
        for (name, property) in &schema.properties {
            match properties.get(name) {
                Some(value) if !property.kind.accepts(value) => {
                    return Err(Diagnostic::new(
                        Code::InvalidProperty,
                        Stage::Construction,
                        format!("property '{name}' requires {:?}", property.kind),
                    ));
                }
                Some(_) => {}
                None => {
                    properties.insert(
                        name.clone(),
                        property.default.clone().ok_or_else(|| {
                            Diagnostic::new(
                                Code::MissingProperty,
                                Stage::Construction,
                                format!("missing property '{name}'"),
                            )
                        })?,
                    );
                }
            }
        }
        for name in slots.keys() {
            if !schema.slots.contains_key(name) {
                return Err(Diagnostic::new(
                    Code::UnknownSlot,
                    Stage::Construction,
                    format!("unknown slot '{name}'"),
                ));
            }
        }
        for (name, slot) in &schema.slots {
            let count = slots.get(name).copied();
            if count.is_none() && slot.minimum > 0 {
                return Err(Diagnostic::new(
                    Code::MissingSlot,
                    Stage::Construction,
                    format!("missing slot '{name}'"),
                ));
            }
            let count = count.unwrap_or_default();
            if count < slot.minimum {
                return Err(Diagnostic::new(
                    Code::EmptySlot,
                    Stage::Construction,
                    format!("slot '{name}' needs at least {} children", slot.minimum),
                ));
            }
            if count > slot.maximum {
                return Err(Diagnostic::new(
                    Code::ResourceLimit,
                    Stage::Construction,
                    format!("slot '{name}' exceeds {} children", slot.maximum),
                ));
            }
        }
        Ok(())
    }

    pub fn validate_channel(
        &self,
        spec: &PublicationSpec,
        channel: Channel,
        limits: &PublicationLimits,
    ) -> Result<()> {
        for occurrence in spec.occurrences(limits)? {
            if !self
                .schema(&occurrence.component)?
                .channels
                .contains(&channel)
            {
                return Err(Diagnostic::new(
                    Code::UnsupportedChannelComponent,
                    Stage::Capability,
                    format!("{} does not support {channel:?}", occurrence.component),
                )
                .at(&spec.metadata.id.0, &occurrence.path));
            }
        }
        Ok(())
    }

    /// Opt-in bridge. Legacy generic source components are untouched by this
    /// crate; portable excerpts explicitly reject names without an interpretation.
    pub fn validate_semantic_component(
        &self,
        name: &str,
        properties: &BTreeMap<String, berlin_document::PropertyValue>,
        blocks: &[berlin_document::Block],
    ) -> Result<()> {
        // Parser-generated bibliographies are semantic prose containers, not
        // arbitrary registered presentation components. Their children still
        // undergo the normal portable-content validation.
        if name == "bibliography-entry" {
            if !properties.is_empty() || blocks.is_empty() {
                return Err(Diagnostic::new(
                    Code::InvalidProperty,
                    Stage::Construction,
                    "bibliography entries require child blocks and no properties",
                ));
            }
            return Ok(());
        }
        if name != "berlin.callout@1" {
            return Err(Diagnostic::new(
                Code::UnsupportedChannelComponent,
                Stage::Capability,
                format!("semantic component '{name}' has no portable interpretation"),
            ));
        }
        let mut call = ComponentCall::new(name.parse()?);
        for (key, value) in properties {
            let value = match value {
                berlin_document::PropertyValue::Text(text) => Property::Text(text.clone()),
                berlin_document::PropertyValue::Boolean(b) => Property::Boolean(*b),
                berlin_document::PropertyValue::Integer(i) => Property::Integer(*i),
                berlin_document::PropertyValue::List(_) => {
                    return Err(Diagnostic::new(
                        Code::InvalidProperty,
                        Stage::Construction,
                        "callout properties do not accept lists",
                    ));
                }
            };
            call.properties.insert(key.clone(), value);
        }
        // Validate scalar schema without manufacturing a new semantic AST.
        let schema = self.schema(&call.component)?;
        let mut scalar = schema.clone();
        scalar.slots.clear();
        let mut builder = RegistryBuilder::default();
        builder.register_schema(scalar)?;
        builder.finish().normalize_call(&mut call)?;
        if blocks.is_empty() {
            return Err(Diagnostic::new(
                Code::EmptySlot,
                Stage::Construction,
                "semantic callout needs child blocks",
            ));
        }
        Ok(())
    }
}
