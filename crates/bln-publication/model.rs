use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use berlin_document::ContentId;
use serde::{Deserialize, Serialize};

use crate::{Code, Diagnostic, PublicationLimits, Result, Stage, fingerprint};

pub(crate) fn identifier(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(Diagnostic::new(
            Code::InvalidIdentifier,
            Stage::Construction,
            "identity must be nonempty and contain no control characters",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PublicationId(pub String);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextProfile {
    #[default]
    Plain,
    #[serde(rename = "linkedin_draft")]
    LinkedInDraft,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextConfig {
    pub profile: TextProfile,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ComponentKey {
    namespace: String,
    name: String,
    version: u32,
}

impl ComponentKey {
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn version(&self) -> u32 {
        self.version
    }
}

impl FromStr for ComponentKey {
    type Err = Diagnostic;
    fn from_str(value: &str) -> Result<Self> {
        let invalid = || {
            Diagnostic::new(
                Code::UnknownComponent,
                Stage::Construction,
                format!(
                    "invalid component key '{value}'; expected namespace.name@positive_version"
                ),
            )
        };
        let (qualified, version) = value.rsplit_once('@').ok_or_else(invalid)?;
        let (namespace, name) = qualified.split_once('.').ok_or_else(invalid)?;
        let valid = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        };
        let version: u32 = version.parse().map_err(|_| invalid())?;
        if !valid(namespace)
            || !valid(name)
            || version == 0
            || version.to_string() != value.rsplit_once('@').unwrap().1
        {
            return Err(invalid());
        }
        Ok(Self {
            namespace: namespace.into(),
            name: name.into(),
            version,
        })
    }
}

impl TryFrom<String> for ComponentKey {
    type Error = Diagnostic;
    fn try_from(value: String) -> Result<Self> {
        value.parse()
    }
}
impl From<ComponentKey> for String {
    fn from(key: ComponentKey) -> Self {
        key.to_string()
    }
}
impl fmt::Display for ComponentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}@{}", self.namespace, self.name, self.version)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentRef {
    pub id: ContentId,
    #[serde(default)]
    pub expected_source_hash: Option<String>,
    #[serde(default)]
    pub expected_document_digest: Option<String>,
}

impl ContentRef {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: ContentId(id.into()),
            expected_source_hash: None,
            expected_document_digest: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExcerptRef {
    pub source: ContentRef,
    pub section: String,
}

/// Dynamic properties carry typed reference values, never unchecked JSON/HTML.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Property {
    Text(String),
    Boolean(bool),
    Integer(i64),
    Source(ContentRef),
    Excerpt(ExcerptRef),
    List(Vec<Property>),
    None,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentCall {
    pub component: ComponentKey,
    pub key: Option<String>,
    pub properties: BTreeMap<String, Property>,
    pub slots: BTreeMap<String, ViewFragment>,
}

impl ComponentCall {
    pub fn measure(&self, limits: &PublicationLimits) -> Result<(usize, usize)> {
        limits.validate()?;
        let mut budget = Budget {
            occurrences: Vec::new(),
            property_bytes: 0,
        };
        budget.visit(self, "root", 1, "constructing", limits)?;
        Ok((budget.occurrences.len(), budget.property_bytes))
    }
    pub fn own_property_bytes(&self, limits: &PublicationLimits) -> Result<usize> {
        let mut budget = Budget {
            occurrences: Vec::new(),
            property_bytes: 0,
        };
        if let Some(key) = &self.key {
            budget.string(key, limits)?;
        }
        for (name, value) in &self.properties {
            budget.string(name, limits)?;
            budget.property(value, limits, 1)?;
        }
        for name in self.slots.keys() {
            budget.string(name, limits)?;
        }
        Ok(budget.property_bytes)
    }
    pub fn new(component: ComponentKey) -> Self {
        Self {
            component,
            key: None,
            properties: BTreeMap::new(),
            slots: BTreeMap::new(),
        }
    }
    pub fn keyed(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ViewFragment(pub Vec<ComponentCall>);
pub type ViewSpec = ComponentCall;

impl From<ComponentCall> for ViewFragment {
    fn from(call: ComponentCall) -> Self {
        Self(vec![call])
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationMetadata {
    pub id: PublicationId,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationSpec {
    pub schema_version: u32,
    pub metadata: PublicationMetadata,
    pub root: ViewSpec,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OccurrenceId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Occurrence {
    pub id: OccurrenceId,
    pub path: String,
    pub component: ComponentKey,
}

impl PublicationSpec {
    /// Construct a publication from a builder, component call or `view!` result.
    /// A publication has one root; helpers may still return multiple siblings.
    pub fn new(metadata: PublicationMetadata, root: impl Into<ViewFragment>) -> Result<Self> {
        let mut roots = root.into().0;
        if roots.len() != 1 {
            return Err(Diagnostic::new(
                Code::InvalidProperty,
                Stage::Construction,
                "publication requires exactly one root component",
            )
            .at(&metadata.id.0, "root"));
        }
        Ok(Self {
            schema_version: 1,
            metadata,
            root: roots.remove(0),
        })
    }
    /// Validates structural identities and budgets. Registry/schema and source
    /// validation are separate, mandatory steps before any rendering.
    pub fn occurrences(&self, limits: &PublicationLimits) -> Result<Vec<Occurrence>> {
        limits.validate()?;
        let publication = &self.metadata.id.0;
        let run = || {
            if self.schema_version != 1 {
                return Err(Diagnostic::new(
                    Code::UnknownVersion,
                    Stage::Construction,
                    "supported publication schema version is 1",
                ));
            }
            identifier(publication)?;
            identifier(&self.metadata.title)?;
            let mut budget = Budget {
                occurrences: Vec::new(),
                property_bytes: 0,
            };
            budget.string(&self.metadata.title, limits)?;
            if let Some(description) = &self.metadata.description {
                budget.string(description, limits)?;
            }
            budget.visit(&self.root, "root", 1, publication, limits)?;
            Ok(budget.occurrences)
        };
        run().map_err(|mut e| {
            e.publication_id = Some(publication.as_str().into());
            e
        })
    }
    pub fn normalized_digest(&self) -> Result<String> {
        fingerprint("spec/v1", self)
    }
}

struct Budget {
    occurrences: Vec<Occurrence>,
    property_bytes: usize,
}
impl Budget {
    fn string(&mut self, text: &str, limits: &PublicationLimits) -> Result<()> {
        if text.len() > limits.string_bytes {
            return Err(Diagnostic::new(
                Code::ResourceLimit,
                Stage::Construction,
                "composition text/property byte budget exceeded",
            ));
        }
        self.bytes(text.len(), limits)
    }
    fn bytes(&mut self, amount: usize, limits: &PublicationLimits) -> Result<()> {
        self.property_bytes = self.property_bytes.saturating_add(amount);
        if self.property_bytes > limits.property_bytes {
            return Err(Diagnostic::new(
                Code::ResourceLimit,
                Stage::Construction,
                "composition property byte budget exceeded",
            ));
        }
        Ok(())
    }
    fn property(
        &mut self,
        value: &Property,
        limits: &PublicationLimits,
        depth: usize,
    ) -> Result<()> {
        if depth > limits.depth {
            return Err(Diagnostic::new(
                Code::ResourceLimit,
                Stage::Construction,
                "property nesting budget exceeded",
            ));
        }
        match value {
            Property::Text(text) => self.string(text, limits),
            Property::Source(source) => self.source(source, limits),
            Property::Excerpt(excerpt) => {
                self.source(&excerpt.source, limits)?;
                self.string(&excerpt.section, limits)
            }
            Property::List(values) => {
                if values.len() > limits.query_sources {
                    return Err(Diagnostic::new(
                        Code::ResourceLimit,
                        Stage::Construction,
                        "property list budget exceeded",
                    ));
                }
                for value in values {
                    self.property(value, limits, depth + 1)?;
                }
                Ok(())
            }
            Property::Boolean(_) | Property::Integer(_) | Property::None => self.bytes(8, limits),
        }
    }
    fn source(&mut self, source: &ContentRef, limits: &PublicationLimits) -> Result<()> {
        identifier(&source.id.0)?;
        self.string(&source.id.0, limits)?;
        for hash in [
            &source.expected_source_hash,
            &source.expected_document_digest,
        ]
        .into_iter()
        .flatten()
        {
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                return Err(Diagnostic::new(
                    Code::InvalidProperty,
                    Stage::Construction,
                    "expected digest must be lowercase hexadecimal SHA-256",
                ));
            }
            self.string(hash, limits)?;
        }
        Ok(())
    }
    fn visit(
        &mut self,
        call: &ComponentCall,
        path: &str,
        depth: usize,
        publication: &str,
        limits: &PublicationLimits,
    ) -> Result<()> {
        let result = (|| {
            if self.occurrences.len() >= limits.occurrences
                || depth > limits.depth
                || call.slots.len() > limits.slots
            {
                return Err(Diagnostic::new(
                    Code::ResourceLimit,
                    Stage::Construction,
                    "component count, depth or slot budget exceeded",
                ));
            }
            if let Some(key) = &call.key {
                identifier(key)?;
                self.string(key, limits)?;
            }
            self.occurrences.push(Occurrence {
                id: OccurrenceId(format!(
                    "occ-{}",
                    &fingerprint("occurrence/v1", &(publication, path))?[..32]
                )),
                path: path.into(),
                component: call.component.clone(),
            });
            for (name, value) in &call.properties {
                self.string(name, limits)?;
                self.property(value, limits, 1)?;
            }
            for (name, children) in &call.slots {
                identifier(name)?;
                self.string(name, limits)?;
                let mut keys = BTreeSet::new();
                for (index, child) in children.0.iter().enumerate() {
                    if let Some(key) = &child.key
                        && !keys.insert(key)
                    {
                        return Err(Diagnostic::new(
                            Code::DuplicateOccurrenceKey,
                            Stage::Construction,
                            format!("duplicate sibling key '{key}' in slot '{name}'"),
                        ));
                    }
                    // Length-delimited segments prevent authored keys from making
                    // positional/slot paths ambiguous (including slashes/colons).
                    let segment = match &child.key {
                        Some(k) => format!("k{}:{k}", k.len()),
                        None => format!("p{index}"),
                    };
                    let child_path = format!("{path}/s{}:{name}/{segment}", name.len());
                    self.visit(child, &child_path, depth + 1, publication, limits)?;
                }
            }
            Ok(())
        })();
        result.map_err(|e: Diagnostic| {
            if e.publication_id.is_none() {
                e.at(publication, path)
            } else {
                e
            }
        })
    }
}
