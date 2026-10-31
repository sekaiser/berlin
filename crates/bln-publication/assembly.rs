use std::collections::{BTreeMap, BTreeSet};

use berlin_document::ContentId;
use serde::{Deserialize, Serialize};

use crate::{
    ComponentCall, ComponentKey, ComponentRegistry, ContentRef, Occurrence, Property,
    PublicationCatalog, PublicationSpec, Result, fingerprint,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceRevision {
    pub id: ContentId,
    pub source_sha256: String,
    pub document_digest: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub occurrence: Occurrence,
    pub sources: Vec<ContentRef>,
}

/// Construction is private to validated resolution. The original catalog is
/// shared, including unselected eligible link targets, never serialized publicly.
#[derive(Clone, Debug)]
pub struct PublicationAssembly {
    spec: PublicationSpec,
    catalog: PublicationCatalog,
    placements: Vec<Placement>,
    sources: Vec<SourceRevision>,
    components: BTreeSet<ComponentKey>,
    composition_fingerprint: String,
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
        fn references(property: &Property, sources: &mut Vec<ContentRef>) {
            match property {
                Property::Source(source) => sources.push(source.clone()),
                Property::Excerpt(excerpt) => sources.push(excerpt.source.clone()),
                Property::List(values) => {
                    for value in values {
                        references(value, sources);
                    }
                }
                _ => {}
            }
        }
        fn visit(
            call: &ComponentCall,
            catalog: &PublicationCatalog,
            occurrences: &mut impl Iterator<Item = Occurrence>,
            placements: &mut Vec<Placement>,
            revisions: &mut BTreeMap<String, SourceRevision>,
            publication: &str,
        ) -> Result<()> {
            let occurrence = occurrences
                .next()
                .expect("validated traversal is unchanged");
            let mut sources = Vec::new();
            for property in call.properties.values() {
                references(property, &mut sources);
            }
            for source in &sources {
                let entry = catalog
                    .resolve(source)
                    .map_err(|e| e.at(publication, &occurrence.path))?;
                revisions
                    .entry(source.id.0.clone())
                    .or_insert_with(|| SourceRevision {
                        id: source.id.clone(),
                        source_sha256: entry.source_sha256.clone(),
                        document_digest: entry.document_digest.clone(),
                    });
                if entry.original_url.is_none() {
                    return Err(crate::Diagnostic::new(
                        crate::Code::UnsafeUrl,
                        crate::Stage::Resolution,
                        "source references require source_site_url or an explicit original URL",
                    )
                    .source(&source.id.0, None)
                    .at(publication, &occurrence.path));
                }
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
        let components: BTreeSet<_> = placements
            .iter()
            .map(|p| p.occurrence.component.clone())
            .collect();
        let schemas = components
            .iter()
            .map(|key| registry.schema(key))
            .collect::<Result<Vec<_>>>()?;
        let composition_fingerprint =
            fingerprint("composition/v1", &(&spec, &placements, &sources, schemas))?;
        Ok(Self {
            spec,
            catalog,
            placements,
            sources,
            components,
            composition_fingerprint,
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
    pub fn sources(&self) -> &[SourceRevision] {
        &self.sources
    }
    pub fn composition_fingerprint(&self) -> &str {
        &self.composition_fingerprint
    }

    /// Exact immutable render inputs must be supplied by the consuming adapter.
    /// Logical names, not source filesystem locations, identify presentation data.
    pub fn manifest(
        &self,
        channel: crate::Channel,
        render_inputs: BTreeMap<String, String>,
        configuration_digest: String,
    ) -> Result<PublicationManifest> {
        let render_input_fingerprint = fingerprint(
            "render-input/v1",
            &(
                self.composition_fingerprint(),
                channel,
                "1",
                &render_inputs,
                &configuration_digest,
            ),
        )?;
        Ok(PublicationManifest {
            schema_version: 1,
            publication_id: self.spec.metadata.id.clone(),
            metadata_digest: fingerprint("metadata/v1", &self.spec.metadata)?,
            normalized_spec_digest: self.spec.normalized_digest()?,
            placements: self.placements.clone(),
            sources: self.sources.clone(),
            components: self.components.clone(),
            channel,
            renderer_contract: 1,
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
    pub sources: Vec<SourceRevision>,
    pub components: BTreeSet<ComponentKey>,
    pub channel: crate::Channel,
    pub renderer_contract: u32,
    pub composition_fingerprint: String,
    pub configuration_digest: String,
    pub render_inputs: BTreeMap<String, String>,
    pub render_input_fingerprint: String,
    pub diagnostics: Vec<crate::Diagnostic>,
}
