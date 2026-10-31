//! Orchestrates opt-in composed outputs. Source/catalog resolution remains pure;
//! only the caller's transaction-owned staging root receives generated files.
use crate::project::Project;
use anyhow::{Context, Error};
use berlin_core::WebsiteConfig;
use berlin_publication::{ComponentRegistry, PublicationAssembly, TextConfig, canonical_bytes};
use berlin_publication_html::{HtmlBindings, PublicationHtmlRenderer, PublicationSite};
use std::path::Path;

pub(super) fn site(
    project: &Project,
    assembly: &berlin_publication::SiteAssembly,
    config: &WebsiteConfig,
    output: &Path,
    dry_run: bool,
    consumed_inputs: &mut Vec<super::receipt::ConsumedInput>,
    assets: &[(std::path::PathBuf, Vec<u8>)],
) -> Result<(), Error> {
    if let Some(settings) = &config.giscus {
        settings.validate().map_err(anyhow::Error::msg)?;
    }
    let theme = match config.theme.as_deref() {
        Some(selected) => super::theme::Theme::load(project.root(), selected)?,
        None => super::theme::Theme::project_only(project.root())?,
    };
    let snapshot = theme.snapshot()?;
    consumed_inputs.extend(snapshot.template_inputs());
    let renderer =
        PublicationHtmlRenderer::new(snapshot.component_templates()?, HtmlBindings::builtins())?;
    let options = berlin_publication_html::SiteHtmlOptions {
        description: config.description.clone(),
        github: config.profiles.github.as_ref().map(ToString::to_string),
        linkedin: config.profiles.linkedin.as_ref().map(ToString::to_string),
        twitter: config.profiles.twitter.as_ref().map(ToString::to_string),
        discussion: config.giscus.as_ref().map(|settings| {
            berlin_publication_html::DiscussionSettings {
                repo: settings.repo.clone(),
                repo_id: settings.repo_id.clone(),
                category: settings.category.clone(),
                category_id: settings.category_id.clone(),
                theme: settings.theme.as_ref().map(ToString::to_string),
            }
        }),
    };
    let mut result = renderer.render_site(
        assembly,
        &ComponentRegistry::builtins(),
        &PublicationSite {
            url: config
                .url
                .as_ref()
                .context("site requires a destination URL")?
                .to_string(),
            title: config.title.clone(),
            author: config.author.clone(),
        },
        &options,
    )?;
    if config.external_links_new_tab {
        for (path, bytes) in &mut result.files {
            if path.ends_with(".html") {
                let html = std::str::from_utf8(bytes)?;
                *bytes = super::website::links::open_external_in_new_tabs_bounded(
                    html,
                    config
                        .url
                        .as_ref()
                        .context("external links require a site URL")?,
                    assembly.catalog().limits().output_bytes,
                )?
                .into_bytes();
            }
        }
    }
    let prepared_assets = snapshot.prepare_assets(&result.stylesheets)?;
    consumed_inputs.extend_from_slice(prepared_assets.consumed_inputs());
    let mut paths = prepared_assets.output_paths();
    paths.extend(result.files.keys().map(std::path::PathBuf::from));
    paths.insert("publication.json".into());
    validate_assets(&paths, assets)?;
    result.manifest.render_inputs.extend(
        consumed_inputs
            .iter()
            .map(|input| (input.logical_name.clone(), input.sha256.clone())),
    );
    result.manifest.render_input_fingerprint = berlin_publication::fingerprint(
        "resource-site-operational-input/v1",
        &(
            &result.manifest.render_input_fingerprint,
            &result.manifest.render_inputs,
            config,
        ),
    )?;
    if dry_run {
        return Ok(());
    }
    std::fs::create_dir_all(output)?;
    prepared_assets.write(output)?;
    for (path, bytes) in result.files {
        let path = output.join(path);
        std::fs::create_dir_all(path.parent().context("site file has no parent")?)?;
        std::fs::write(path, bytes)?;
    }
    std::fs::write(
        output.join("publication.json"),
        canonical_bytes(&result.manifest)?,
    )?;
    write_assets(output, assets)?;
    Ok(())
}

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
    assets: &[(std::path::PathBuf, Vec<u8>)],
) -> Result<(), Error> {
    let theme = match config.theme.as_deref() {
        Some(selected) => super::theme::Theme::load(project.root(), selected)?,
        None => super::theme::Theme::project_only(project.root())?,
    };
    let snapshot = theme.snapshot()?;
    consumed_inputs.extend(snapshot.template_inputs());
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
    let prepared_assets = snapshot.prepare_assets(&result.stylesheets)?;
    consumed_inputs.extend_from_slice(prepared_assets.consumed_inputs());
    let mut paths = prepared_assets.output_paths();
    paths.extend(["index.html".into(), "publication.json".into()]);
    validate_assets(&paths, assets)?;
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
    prepared_assets.write(output)?;
    std::fs::write(output.join("index.html"), result.html)?;
    std::fs::write(
        output.join("publication.json"),
        canonical_bytes(&result.manifest)?,
    )?;
    write_assets(output, assets)?;
    Ok(())
}

fn validate_assets(
    paths: &std::collections::BTreeSet<std::path::PathBuf>,
    assets: &[(std::path::PathBuf, Vec<u8>)],
) -> Result<(), Error> {
    let mut all = std::collections::BTreeSet::<String>::new();
    for path in paths.iter().chain(assets.iter().map(|(path, _)| path)) {
        if path
            .components()
            .any(|p| !matches!(p, std::path::Component::Normal(_)))
            || path.as_os_str().is_empty()
        {
            anyhow::bail!("asset destination is not confined");
        }
        let name = path.to_str().context("asset destination is not UTF-8")?;
        if name.chars().any(|c| c.is_control() || c == '\\') {
            anyhow::bail!("asset destination contains unsafe characters");
        }
        let name = name.to_ascii_lowercase();
        let prefix = format!("{name}/");
        let ancestor = path
            .ancestors()
            .skip(1)
            .filter_map(|p| p.to_str())
            .any(|p| all.contains(&p.to_ascii_lowercase()));
        if all.contains(&name)
            || ancestor
            || all
                .range(prefix.clone()..)
                .next()
                .is_some_and(|p| p.starts_with(&prefix))
        {
            anyhow::bail!(
                "asset destination conflicts with another output: {}",
                path.display()
            );
        }
        all.insert(name);
    }
    Ok(())
}
fn write_assets(root: &Path, assets: &[(std::path::PathBuf, Vec<u8>)]) -> Result<(), Error> {
    for (path, bytes) in assets {
        let destination = root.join(path);
        std::fs::create_dir_all(destination.parent().context("asset has no parent")?)?;
        std::fs::write(destination, bytes)?;
    }
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
                website(
                    &project,
                    &assembly,
                    &config,
                    &staged,
                    false,
                    &mut vec![],
                    &[]
                )
                .is_err(),
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
