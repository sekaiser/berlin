use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    ComponentCall, ComponentKey, ComponentRegistry, Occurrence, Property, PublicationCatalog,
    PublicationSpec, ResourceId, ResourceRef, ResourceSource, Result, fingerprint,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceRevision {
    pub id: ResourceId,
    pub source: ResourceSource,
    pub record_digest: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub occurrence: Occurrence,
    pub sources: Vec<ResourceRef>,
}

/// Construction is private to validated resolution. The original catalog is
/// shared, including unselected eligible link targets, never serialized publicly.
#[derive(Clone, Debug)]
pub struct PublicationAssembly {
    spec: PublicationSpec,
    catalog: PublicationCatalog,
    placements: Vec<Placement>,
    sources: Vec<ResourceRevision>,
    components: BTreeSet<ComponentKey>,
    composition_fingerprint: String,
    excerpts: BTreeMap<(String, String), crate::ResolvedExcerpt>,
}

impl PublicationAssembly {
    pub fn resolve(
        spec: &PublicationSpec,
        catalog: PublicationCatalog,
        registry: &ComponentRegistry,
    ) -> Result<Self> {
        let spec = registry.normalize(spec, catalog.limits())?;
        let occurrences = spec.occurrences(catalog.limits())?;
        let mut placements = Vec::new();
        let mut revisions = BTreeMap::new();
        let mut occurrences = occurrences.into_iter();
        fn references(
            property: &Property,
            catalog: &PublicationCatalog,
            sources: &mut Vec<ResourceRef>,
        ) -> Result<()> {
            match property {
                Property::Resource(reference) => sources.push(reference.clone()),
                Property::Source(source) => {
                    catalog.resolve(source)?;
                    sources.push(ResourceRef::try_from(source)?);
                }
                Property::Excerpt(excerpt) => {
                    catalog.resolve(&excerpt.source)?;
                    sources.push(ResourceRef::try_from(&excerpt.source)?);
                }
                Property::List(values) => {
                    for value in values {
                        references(value, catalog, sources)?;
                    }
                }
                _ => {}
            }
            Ok(())
        }
        fn revision(
            catalog: &PublicationCatalog,
            source: &ResourceRef,
        ) -> Result<ResourceRevision> {
            let entry = catalog.resolve_resource(source)?;
            if entry.descriptor().title.trim().is_empty() {
                return Err(crate::Diagnostic::new(
                    crate::Code::InvalidSource,
                    crate::Stage::Resolution,
                    "selected resource requires a title",
                ));
            }
            if entry.original_url().is_none() {
                return Err(crate::Diagnostic::new(
                    crate::Code::UnsafeUrl,
                    crate::Stage::Resolution,
                    "selected resource requires an original URL mapping",
                ));
            }
            Ok(ResourceRevision {
                id: source.id.clone(),
                source: entry.source().clone(),
                record_digest: entry.descriptor().revision.clone(),
            })
        }
        fn visit(
            call: &ComponentCall,
            catalog: &PublicationCatalog,
            occurrences: &mut impl Iterator<Item = Occurrence>,
            placements: &mut Vec<Placement>,
            revisions: &mut BTreeMap<ResourceId, ResourceRevision>,
            publication: &str,
        ) -> Result<()> {
            let occurrence = occurrences
                .next()
                .expect("validated traversal is unchanged");
            let mut sources = Vec::new();
            for property in call.properties.values() {
                references(property, catalog, &mut sources)
                    .map_err(|e| e.at(publication, &occurrence.path))?;
            }
            for source in &sources {
                let resolved =
                    revision(catalog, source).map_err(|e| e.at(publication, &occurrence.path))?;
                revisions.entry(source.id.clone()).or_insert(resolved);
            }
            placements.push(Placement {
                occurrence,
                sources,
            });
            for children in call.slots.values() {
                for child in &children.0 {
                    visit(
                        child,
                        catalog,
                        occurrences,
                        placements,
                        revisions,
                        publication,
                    )?;
                }
            }
            Ok(())
        }
        visit(
            &spec.root,
            &catalog,
            &mut occurrences,
            &mut placements,
            &mut revisions,
            &spec.metadata.id.0,
        )?;
        let sources: Vec<_> = revisions.into_values().collect();
        let mut excerpts = BTreeMap::new();
        fn resolve_excerpts(
            call: &ComponentCall,
            catalog: &PublicationCatalog,
            registry: &ComponentRegistry,
            excerpts: &mut BTreeMap<(String, String), crate::ResolvedExcerpt>,
        ) -> Result<()> {
            fn property(
                value: &Property,
                catalog: &PublicationCatalog,
                registry: &ComponentRegistry,
                excerpts: &mut BTreeMap<(String, String), crate::ResolvedExcerpt>,
            ) -> Result<()> {
                match value {
                    Property::Excerpt(reference) => {
                        let key = (reference.source.id.0.clone(), reference.section.clone());
                        if let std::collections::btree_map::Entry::Vacant(entry) =
                            excerpts.entry(key)
                        {
                            entry.insert(crate::excerpt::resolve_excerpt(
                                catalog, reference, registry,
                            )?);
                        }
                    }
                    Property::List(values) => {
                        for value in values {
                            property(value, catalog, registry, excerpts)?;
                        }
                    }
                    _ => {}
                }
                Ok(())
            }
            for value in call.properties.values() {
                property(value, catalog, registry, excerpts)?;
            }
            for children in call.slots.values() {
                for child in &children.0 {
                    resolve_excerpts(child, catalog, registry, excerpts)?;
                }
            }
            Ok(())
        }
        resolve_excerpts(&spec.root, &catalog, registry, &mut excerpts)
            .map_err(|e| e.at(&spec.metadata.id.0, "root"))?;
        let mut sources: BTreeMap<_, _> = sources.into_iter().map(|s| (s.id.clone(), s)).collect();
        for excerpt in excerpts.values() {
            for id in &excerpt.linked_sources {
                let reference = ResourceRef::local(id)?;
                sources
                    .entry(reference.id.clone())
                    .or_insert(revision(&catalog, &reference)?);
            }
        }
        let sources: Vec<_> = sources.into_values().collect();
        let mut components: BTreeSet<_> = placements
            .iter()
            .map(|p| p.occurrence.component.clone())
            .collect();
        for excerpt in excerpts.values() {
            components.extend(excerpt.semantic_components.iter().cloned());
        }
        let schemas = components
            .iter()
            .map(|key| registry.schema(key))
            .collect::<Result<Vec<_>>>()?;
        let composition_fingerprint = fingerprint(
            "resource-composition/v1",
            &(
                &spec,
                &placements,
                sources
                    .iter()
                    .map(|s| (&s.id, &s.record_digest))
                    .collect::<Vec<_>>(),
                schemas,
            ),
        )?;
        Ok(Self {
            spec,
            catalog,
            placements,
            sources,
            components,
            composition_fingerprint,
            excerpts,
        })
    }
    pub fn spec(&self) -> &PublicationSpec {
        &self.spec
    }
    pub fn catalog(&self) -> &PublicationCatalog {
        &self.catalog
    }
    pub fn placements(&self) -> &[Placement] {
        &self.placements
    }
    pub fn sources(&self) -> &[ResourceRevision] {
        &self.sources
    }
    pub fn excerpt(&self, reference: &crate::ExcerptRef) -> Result<&crate::ResolvedExcerpt> {
        self.excerpts
            .get(&(reference.source.id.0.clone(), reference.section.clone()))
            .ok_or_else(|| {
                crate::Diagnostic::new(
                    crate::Code::MissingExcerpt,
                    crate::Stage::Resolution,
                    "excerpt was not resolved in this assembly",
                )
                .source(&reference.source.id.0, Some(&reference.section))
            })
    }
    pub fn composition_fingerprint(&self) -> &str {
        &self.composition_fingerprint
    }
    pub fn components(&self) -> &BTreeSet<ComponentKey> {
        &self.components
    }

    /// Exact immutable render inputs must be supplied by the consuming adapter.
    /// Logical names, not source filesystem locations, identify presentation data.
    pub fn manifest(
        &self,
        channel: crate::Channel,
        mut render_inputs: BTreeMap<String, String>,
        configuration_digest: String,
    ) -> Result<PublicationManifest> {
        render_inputs.insert(
            "catalog/original-urls".into(),
            self.catalog.original_urls_digest().into(),
        );
        let render_input_fingerprint = fingerprint(
            "resource-render-input/v1",
            &(
                self.composition_fingerprint(),
                channel,
                "2",
                &render_inputs,
                &configuration_digest,
            ),
        )?;
        Ok(PublicationManifest {
            schema_version: 2,
            publication_id: self.spec.metadata.id.clone(),
            metadata_digest: fingerprint("metadata/v1", &self.spec.metadata)?,
            normalized_spec_digest: self.spec.normalized_digest()?,
            placements: self.placements.clone(),
            sources: self.sources.clone(),
            components: self.components.clone(),
            channel,
            renderer_contract: 2,
            composition_fingerprint: self.composition_fingerprint.clone(),
            configuration_digest,
            render_inputs,
            render_input_fingerprint,
            diagnostics: Vec::new(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationManifest {
    pub schema_version: u32,
    pub publication_id: crate::PublicationId,
    pub metadata_digest: String,
    pub normalized_spec_digest: String,
    pub placements: Vec<Placement>,
    pub sources: Vec<ResourceRevision>,
    pub components: BTreeSet<ComponentKey>,
    pub channel: crate::Channel,
    pub renderer_contract: u32,
    pub composition_fingerprint: String,
    pub configuration_digest: String,
    pub render_inputs: BTreeMap<String, String>,
    pub render_input_fingerprint: String,
    pub diagnostics: Vec<crate::Diagnostic>,
}
impl PublicationManifest {
    pub fn record_inputs(&mut self, inputs: BTreeMap<String, String>) -> Result<()> {
        for (name, digest) in &inputs {
            if name.is_empty()
                || name.starts_with('/')
                || name.contains('\\')
                || name.contains(':')
                || name.chars().any(char::is_control)
                || name
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
            {
                return Err(crate::Diagnostic::new(
                    crate::Code::UnsafeUrl,
                    crate::Stage::Presentation,
                    "manifest input names must be logical relative paths",
                ));
            }
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(crate::Diagnostic::new(
                    crate::Code::InvalidProperty,
                    crate::Stage::Presentation,
                    "manifest input digests must be lowercase SHA-256",
                ));
            }
        }
        self.render_inputs.extend(inputs);
        self.render_input_fingerprint = fingerprint(
            "resource-render-input/v1",
            &(
                &self.composition_fingerprint,
                self.channel,
                "2",
                &self.render_inputs,
                &self.configuration_digest,
            ),
        )?;
        Ok(())
    }
}
