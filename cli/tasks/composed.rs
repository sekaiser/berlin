//! Orchestrates opt-in composed outputs. Source/catalog resolution remains pure;
//! only the caller's transaction-owned staging root receives generated files.
use crate::project::Project;
use anyhow::{Context, Error};
use berlin_core::WebsiteConfig;
use berlin_publication::{ComponentRegistry, PublicationAssembly, TextConfig, canonical_bytes};
use berlin_publication_html::{HtmlBindings, PublicationHtmlRenderer, PublicationSite};
use std::path::Path;

pub(super) fn text(
    assembly: &PublicationAssembly,
    config: &TextConfig,
    output: &Path,
    dry_run: bool,
    consumed_inputs: &[super::receipt::ConsumedInput],
) -> Result<(), Error> {
    let mut result =
        berlin_publication_text::render(assembly, &ComponentRegistry::builtins(), config)?;
    let mut inputs = consumed_inputs
        .iter()
        .map(|input| (input.logical_name.clone(), input.sha256.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    inputs.insert(
        "composition/catalog".into(),
        assembly.catalog().input_digest().into(),
    );
    result.manifest.publication.record_inputs(inputs)?;
    if dry_run {
        return Ok(());
    }
    std::fs::create_dir_all(output)?;
    std::fs::write(output.join("publication.txt"), result.text)?;
    std::fs::write(
        output.join("manifest.json"),
        canonical_bytes(&result.manifest)?,
    )?;
    Ok(())
}
pub(super) fn website(
    project: &Project,
    assembly: &PublicationAssembly,
    config: &WebsiteConfig,
    output: &Path,
    dry_run: bool,
    consumed_inputs: &mut Vec<super::receipt::ConsumedInput>,
) -> Result<(), Error> {
    let selected = config
        .theme
        .as_deref()
        .context("composable website requires an explicit local theme")?;
    let theme = super::theme::Theme::load(project.root(), selected)?;
    let snapshot = theme.snapshot()?;
    consumed_inputs.extend(snapshot.consumed_inputs());
    let renderer =
        PublicationHtmlRenderer::new(snapshot.component_templates()?, HtmlBindings::builtins())?;
    let mut result = renderer.render(
        assembly,
        &ComponentRegistry::builtins(),
        &PublicationSite {
            url: config
                .url
                .as_ref()
                .context("composable website requires a destination URL")?
                .to_string(),
            title: config.title.clone(),
            author: config.author.clone(),
        },
    )?;
    snapshot.validate_entries(&result.stylesheets)?;
    let mut inputs = consumed_inputs
        .iter()
        .map(|input| (input.logical_name.clone(), input.sha256.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    inputs.insert(
        "composition/catalog".into(),
        assembly.catalog().input_digest().into(),
    );
    result.manifest.record_inputs(inputs)?;
    if dry_run {
        return Ok(());
    }
    std::fs::create_dir_all(output)?;
    snapshot.write_assets(output)?;
    std::fs::write(output.join("index.html"), result.html)?;
    std::fs::write(
        output.join("publication.json"),
        canonical_bytes(&result.manifest)?,
    )?;
    Ok(())
}
