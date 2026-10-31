//! Independent text adapter boundary. Does not depend on an HTML renderer.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;

use berlin_document_linkedin::{LINKEDIN_POST_CHARACTER_LIMIT, LinkedInDraftDiagnostic, fragment};
use berlin_publication::*;
use serde::{Deserialize, Serialize};

pub use berlin_publication::{TextConfig, TextProfile};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextManifest {
    pub schema_version: u32,
    pub publication: PublicationManifest,
    pub profile: TextProfile,
    pub character_count: usize,
    pub diagnostics: Vec<LinkedInDraftDiagnostic>,
}
pub struct TextPublication {
    pub text: String,
    pub manifest: TextManifest,
}

pub fn render(
    assembly: &PublicationAssembly,
    registry: &ComponentRegistry,
    config: &TextConfig,
) -> Result<TextPublication> {
    registry.validate_channel(assembly.spec(), Channel::Text, assembly.catalog().limits())?;
    let mut output = BoundedWriter::new(assembly.catalog().limits().output_bytes);
    write!(output, "{}\n\n", assembly.spec().metadata.title).map_err(write_error)?;
    if let Some(description) = &assembly.spec().metadata.description {
        write!(output, "{description}\n\n").map_err(write_error)?;
    }
    let mut descriptions = HashMap::new();
    for placement in assembly
        .placements()
        .iter()
        .filter(|p| p.occurrence.component.to_string() == "berlin.featured_work@1")
    {
        for reference in &placement.sources {
            if descriptions.contains_key(&reference.id.0) {
                continue;
            }
            if let Some(description) = &assembly
                .catalog()
                .resolve(reference)?
                .document()
                .metadata
                .description
            {
                if description.len() > assembly.catalog().limits().string_bytes {
                    return Err(failure(
                        Code::ResourceLimit,
                        "source description exceeds string budget",
                    ));
                }
                let source = markdown::Source::in_memory(description);
                let document = markdown::Parser::new()
                    .parse(&source)
                    .map_err(|e| failure(Code::InvalidSource, e.to_string()))?;
                descriptions.insert(reference.id.0.clone(), document.blocks);
            }
        }
    }
    let mut session = Session {
        assembly,
        descriptions,
        cursor: 0,
    };
    session.component(&assembly.spec().root, &mut output)?;
    output.write_all(b"\n").map_err(write_error)?;
    let text = String::from_utf8(output.into_bytes())
        .map_err(|e| failure(Code::InvalidProperty, e.to_string()))?;
    let diagnostics = if config.profile == TextProfile::LinkedInDraft {
        berlin_document_linkedin::draft_diagnostics(&text, LINKEDIN_POST_CHARACTER_LIMIT)
    } else {
        Vec::new()
    };
    let publication = assembly.manifest(
        Channel::Text,
        BTreeMap::new(),
        fingerprint("text-config/v1", &(config, assembly.catalog().options()))?,
    )?;
    Ok(TextPublication {
        text,
        manifest: TextManifest {
            schema_version: 1,
            publication,
            profile: config.profile,
            character_count: 0,
            diagnostics,
        },
    })
    .map(|mut result| {
        result.manifest.character_count = result.text.chars().count();
        result
    })
}

struct Session<'a> {
    assembly: &'a PublicationAssembly,
    descriptions: HashMap<String, Vec<berlin_document::Block>>,
    cursor: usize,
}
impl Session<'_> {
    fn component(&mut self, call: &ComponentCall, output: &mut BoundedWriter) -> Result<()> {
        let placement = self
            .assembly
            .placements()
            .get(self.cursor)
            .ok_or_else(|| failure(Code::InvalidProperty, "assembly traversal mismatch"))?;
        self.cursor += 1;
        let run = |this: &mut Self, output: &mut BoundedWriter| -> Result<()> {
            match call.component.to_string().as_str() {
                "berlin.portfolio@1" => this.slot(call, "body", output)?,
                "berlin.section@1" => {
                    write!(output, "{}\n\n", text(call, "title")?).map_err(write_error)?;
                    this.slot(call, "body", output)?;
                }
                "berlin.introduction@1" => output
                    .write_all(text(call, "text")?.as_bytes())
                    .map_err(write_error)?,
                "berlin.callout@1" => {
                    write!(output, "[{}]", text(call, "kind")?).map_err(write_error)?;
                    if let Some(title) = optional(call, "title")? {
                        write!(output, " {title}").map_err(write_error)?;
                    }
                    output.write_all(b"\n").map_err(write_error)?;
                    this.slot(call, "body", output)?;
                }
                "berlin.featured_work@1" => {
                    let Property::Source(reference) = &call.properties["source"] else {
                        return Err(failure(Code::InvalidProperty, "expected source reference"));
                    };
                    this.source(reference, output)?;
                    if let Some(blocks) = this.descriptions.get(&reference.id.0) {
                        let entry = this.assembly.catalog().resolve(reference)?;
                        let original = entry
                            .original_url
                            .as_ref()
                            .ok_or_else(|| failure(Code::UnsafeUrl, "missing original URL"))?;
                        let mut context = fragment::Context::default();
                        walk_blocks(blocks, &mut |block| {
                            visit_block_inlines(block, &mut |inline| {
                                let destination = match inline {
                                    berlin_document::Inline::Link { destination, .. }
                                        if !destination.starts_with("mailto:")
                                            && !destination.starts_with("tel:") =>
                                    {
                                        Some(destination)
                                    }
                                    berlin_document::Inline::Image { source, .. } => Some(source),
                                    _ => None,
                                };
                                if let Some(source) = destination {
                                    let mount = this
                                        .assembly
                                        .catalog()
                                        .options()
                                        .source_site_url
                                        .as_ref()
                                        .map(mount_url)
                                        .transpose()?;
                                    context.destinations.insert(
                                        source.clone(),
                                        resolve_source_url(source, original, mount.as_ref())?
                                            .to_string(),
                                    );
                                }
                                Ok(())
                            })
                        })?;
                        output.write_all(b"\n\n").map_err(write_error)?;
                        fragment::write_blocks(blocks, &context, output).map_err(write_error)?;
                    }
                    annotation(call, output)?;
                }
                "berlin.excerpt@1" => {
                    let Property::Excerpt(reference) = &call.properties["source"] else {
                        return Err(failure(Code::InvalidProperty, "expected excerpt reference"));
                    };
                    if let Some(annotation) = optional(call, "annotation")? {
                        write!(output, "{annotation}\n\n").map_err(write_error)?;
                    }
                    let excerpt = this.assembly.excerpt(reference)?;
                    let mut context = fragment::Context::default();
                    for anchor in &excerpt.source_anchors {
                        let mut url = excerpt.original_url.clone();
                        url.set_fragment(Some(anchor));
                        context
                            .destinations
                            .insert(format!("#{anchor}"), url.to_string());
                    }
                    for id in &excerpt.linked_sources {
                        let entry = this.assembly.catalog().entry(id)?;
                        context.document_urls.insert(
                            id.clone(),
                            entry
                                .original_url
                                .as_ref()
                                .ok_or_else(|| failure(Code::UnsafeUrl, "missing original URL"))?
                                .to_string(),
                        );
                    }
                    walk_blocks(excerpt.blocks(), &mut |block| {
                        if let berlin_document::Block::Figure(figure) = block {
                            context.destinations.insert(
                                figure.source.clone(),
                                excerpt.resolve_url(&figure.source)?.to_string(),
                            );
                        }
                        visit_block_inlines(block, &mut |inline| {
                            let source = match inline {
                                berlin_document::Inline::Link { destination, .. }
                                    if !destination.starts_with('#')
                                        && !destination.starts_with("mailto:")
                                        && !destination.starts_with("tel:") =>
                                {
                                    Some(destination)
                                }
                                berlin_document::Inline::Image { source, .. } => Some(source),
                                _ => None,
                            };
                            if let Some(source) = source {
                                context.destinations.insert(
                                    source.clone(),
                                    excerpt.resolve_url(source)?.to_string(),
                                );
                            }
                            Ok(())
                        })
                    })?;
                    fragment::write_blocks(excerpt.blocks(), &context, output)
                        .map_err(write_error)?;
                    output.write_all(b"\n\nFrom ").map_err(write_error)?;
                    this.source(&reference.source, output)?;
                }
                "berlin.related_work@1" => {
                    if let Some(title) = optional(call, "title")? {
                        write!(output, "{title}\n\n").map_err(write_error)?;
                    }
                    let Property::List(sources) = &call.properties["sources"] else {
                        return Err(failure(Code::InvalidProperty, "expected source list"));
                    };
                    for (index, source) in sources.iter().enumerate() {
                        if index > 0 {
                            output.write_all(b"\n").map_err(write_error)?;
                        }
                        let Property::Source(source) = source else {
                            return Err(failure(
                                Code::InvalidProperty,
                                "expected source reference",
                            ));
                        };
                        this.source(source, output)?;
                    }
                }
                key => {
                    return Err(failure(
                        Code::UnsupportedChannelComponent,
                        format!("no text interpretation for '{key}'"),
                    ));
                }
            }
            Ok(())
        };
        run(self, output).map_err(|e| {
            e.at(
                &self.assembly.spec().metadata.id.0,
                &placement.occurrence.path,
            )
        })
    }
    fn slot(&mut self, call: &ComponentCall, name: &str, output: &mut BoundedWriter) -> Result<()> {
        let children = call
            .slots
            .get(name)
            .ok_or_else(|| failure(Code::MissingSlot, format!("missing slot '{name}'")))?;
        for (index, child) in children.0.iter().enumerate() {
            if index > 0 {
                output.write_all(b"\n\n").map_err(write_error)?;
            }
            self.component(child, output)?;
        }
        Ok(())
    }
    fn source(&self, source: &ContentRef, output: &mut BoundedWriter) -> Result<()> {
        let entry = self.assembly.catalog().resolve(source)?;
        let title = entry
            .document()
            .metadata
            .title
            .as_deref()
            .ok_or_else(|| failure(Code::InvalidSource, "selected source requires a title"))?;
        let url = entry
            .original_url
            .as_ref()
            .ok_or_else(|| failure(Code::UnsafeUrl, "source requires an original URL"))?;
        write!(output, "{title}\n{url}").map_err(write_error)
    }
}
fn text<'a>(call: &'a ComponentCall, key: &str) -> Result<&'a str> {
    match call.properties.get(key) {
        Some(Property::Text(text)) => Ok(text),
        _ => Err(failure(
            Code::InvalidProperty,
            format!("expected text property '{key}'"),
        )),
    }
}
fn optional<'a>(call: &'a ComponentCall, key: &str) -> Result<Option<&'a str>> {
    match call.properties.get(key) {
        Some(Property::Text(text)) => Ok(Some(text)),
        Some(Property::None) => Ok(None),
        _ => Err(failure(
            Code::InvalidProperty,
            format!("expected optional text property '{key}'"),
        )),
    }
}
fn annotation(call: &ComponentCall, output: &mut BoundedWriter) -> Result<()> {
    if let Some(annotation) = optional(call, "annotation")? {
        write!(output, "\n\n{annotation}").map_err(write_error)?;
    }
    Ok(())
}
fn failure(code: Code, detail: impl Into<String>) -> Diagnostic {
    Diagnostic::new(code, Stage::Presentation, detail)
}
fn write_error(error: std::io::Error) -> Diagnostic {
    let detail = error.to_string();
    let code = if detail.contains("UnsupportedChannel") {
        Code::UnsupportedChannelFeature
    } else if detail.contains("UnresolvedReference") {
        Code::UnresolvedReference
    } else {
        Code::ResourceLimit
    };
    failure(code, detail)
}
