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
) -> Result<Vec<berlin_document_linkedin::LinkedInDraftDiagnostic>, Error> {
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
        return Ok(result.manifest.diagnostics);
    }
    std::fs::create_dir_all(output)?;
    std::fs::write(output.join("publication.txt"), result.text)?;
    std::fs::write(
        output.join("manifest.json"),
        canonical_bytes(&result.manifest)?,
    )?;
    Ok(result.manifest.diagnostics)
}
pub(super) fn website(
    project: &Project,
    assembly: &PublicationAssembly,
    config: &WebsiteConfig,
    output: &Path,
    dry_run: bool,
    consumed_inputs: &mut Vec<super::receipt::ConsumedInput>,
) -> Result<(), Error> {
    let theme = match config.theme.as_deref() {
        Some(selected) => super::theme::Theme::load(project.root(), selected)?,
        None => super::theme::Theme::project_only(project.root())?,
    };
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
    if config.external_links_new_tab {
        result.html = super::website::links::open_external_in_new_tabs_bounded(
            &result.html,
            config
                .url
                .as_ref()
                .context("external links require a website URL")?,
            assembly.catalog().limits().output_bytes,
        )?;
    }
    result.manifest.configuration_digest = berlin_publication::fingerprint(
        "website-config/v1",
        &(&result.manifest.configuration_digest, config),
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
    snapshot.write_assets(output, &result.stylesheets)?;
    std::fs::write(output.join("index.html"), result.html)?;
    std::fs::write(
        output.join("publication.json"),
        canonical_bytes(&result.manifest)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use berlin_publication::*;
    #[test]
    fn f05_asset_and_manifest_write_failures_preserve_installed_output() {
        for failure in ["asset", "manifest"] {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir(root.path().join("_site")).unwrap();
            std::fs::write(root.path().join("_site/sentinel"), "installed output").unwrap();
            std::fs::create_dir_all(root.path().join("static/z")).unwrap();
            std::fs::write(root.path().join("static/a.txt"), "first copied asset").unwrap();
            std::fs::write(
                root.path().join("static/z/nested.txt"),
                "later copied asset",
            )
            .unwrap();
            let project = Project::new(root.path().to_owned(), vec![]);
            let catalog = PublicationCatalog::new(
                berlin_content::DocumentCollection::default(),
                PublicationOptions::default(),
            )
            .unwrap();
            let assembly = PublicationAssembly::resolve(
                &PublicationSpec {
                    schema_version: 1,
                    metadata: PublicationMetadata {
                        id: PublicationId("test".into()),
                        title: "Test".into(),
                        description: None,
                    },
                    root: Portfolio::builder()
                        .body(
                            Introduction::builder()
                                .text("Synthetic publication.")
                                .build(),
                        )
                        .build(),
                },
                catalog,
                &ComponentRegistry::builtins(),
            )
            .unwrap();
            let transaction =
                super::super::output::OutputTransaction::new(root.path(), [Path::new("_site")])
                    .unwrap();
            let staged = transaction.staging_root().join("_site");
            if failure == "asset" {
                std::fs::create_dir_all(staged.join("static")).unwrap();
                std::fs::write(staged.join("static/z"), "blocks asset directory").unwrap();
            } else {
                std::fs::create_dir_all(staged.join("publication.json")).unwrap();
            }
            let config = WebsiteConfig {
                theme: Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("../themes/notebook")),
                url: Some("https://example.test".parse().unwrap()),
                ..Default::default()
            };
            assert!(
                website(&project, &assembly, &config, &staged, false, &mut vec![]).is_err(),
                "{failure}"
            );
            assert!(
                staged.join("static/a.txt").is_file(),
                "failure must follow an actual staged write"
            );
            if failure == "manifest" {
                assert!(staged.join("index.html").is_file());
            }
            drop(transaction);
            assert_eq!(
                std::fs::read_to_string(root.path().join("_site/sentinel")).unwrap(),
                "installed output"
            );
            assert_eq!(
                std::fs::read_dir(root.path().join("_site"))
                    .unwrap()
                    .count(),
                1
            );
        }
    }
}
