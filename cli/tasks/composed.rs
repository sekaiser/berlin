//! Orchestrates opt-in composed outputs. Source/catalog resolution remains pure;
//! only the caller's transaction-owned staging root receives generated files.
use crate::project::Project;
use anyhow::{Context, Error};
use berlin_core::WebsiteConfig;
use berlin_publication::{
    ComponentRegistry, PublicationAssembly, TextConfig, canonical_bytes, input_digest,
};
use berlin_publication_html::{HtmlBindings, PublicationHtmlRenderer, PublicationSite};
use std::path::Path;

/// Run after all staged writes and optional production rewrites, never against
/// installed output. Input evidence remains the snapshots supplied by renderers;
/// only generated output bytes are read here.
pub(super) fn finalize_manifest(
    output: &Path,
    text: bool,
    production: bool,
    limits: &berlin_publication::PublicationLimits,
) -> Result<(), Error> {
    let filename = if text {
        "manifest.json"
    } else {
        "publication.json"
    };
    let manifest_path = output.join(filename);
    if std::fs::metadata(&manifest_path)?.len() > limits.output_bytes as u64 {
        anyhow::bail!("publication manifest exceeds output limit");
    }
    let mut manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
    let publication = if text {
        manifest
            .get_mut("publication")
            .context("text manifest has no publication")?
    } else {
        &mut manifest
    };
    let files = super::release::inventory(output)?;
    if files.len() > limits.occurrences {
        anyhow::bail!("publication output file count exceeded");
    }
    let processing = if production {
        "berlin-production/v1"
    } else {
        "berlin-development/v1"
    };
    let evidence = berlin_publication::OutputEvidence {
        policy: "all-files-except-own-manifest/v1".into(),
        processing: processing.into(),
        files: files
            .into_iter()
            .filter(|file| file.path != filename)
            .map(|file| berlin_publication::OutputFile {
                path: file.path,
                bytes: file.bytes,
                sha256: file.sha256,
            })
            .collect(),
    };
    let existing_processing = publication
        .get("output")
        .and_then(|output| output.get("processing"))
        .and_then(serde_json::Value::as_str);
    if existing_processing.is_some_and(|existing| existing != processing) {
        anyhow::bail!("cannot change a finalized publication processing policy");
    }
    let fingerprint = if existing_processing.is_some() {
        publication
            .get("render_input_fingerprint")
            .context("missing render input fingerprint")?
            .clone()
    } else {
        berlin_publication::fingerprint(
            "resource-final-render-input/v1",
            &(
                publication
                    .get("render_input_fingerprint")
                    .context("missing render input fingerprint")?,
                processing,
            ),
        )?
        .into()
    };
    publication["output"] = serde_json::to_value(evidence)?;
    publication["render_input_fingerprint"] = fingerprint;
    let mut bytes = berlin_publication::BoundedWriter::new(limits.output_bytes);
    serde_json::to_writer(&mut bytes, &manifest)?;
    std::fs::write(manifest_path, bytes.into_bytes())?;
    Ok(())
}

pub(super) fn site(
    project: &Project,
    assembly: &berlin_publication::SiteAssembly,
    config: &WebsiteConfig,
    output: &Path,
    options: super::ExecutionOptions,
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
    let html_options = berlin_publication_html::SiteHtmlOptions {
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
        &html_options,
    )?;
    if config.external_links_new_tab {
        for (path, bytes) in &mut result.files {
            if path.ends_with(".html") {
                let html = std::str::from_utf8(bytes)?;
                *bytes = super::links::open_external_in_new_tabs_bounded(
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
    let cache = (!options.dry_run && !options.no_cache)
        .then(|| super::cache::ArtifactCache::new(project.root()));
    let prepared_assets = snapshot.prepare_assets(&result.stylesheets, cache.as_ref())?;
    consumed_inputs.extend_from_slice(prepared_assets.consumed_inputs());
    let mut paths = prepared_assets.output_paths();
    paths.extend(result.files.keys().map(std::path::PathBuf::from));
    paths.insert("publication.json".into());
    validate_assets(&paths, assets)?;
    if !options.dry_run {
        validate_staged_assets(output, &paths, assets)?;
    }
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
    if options.dry_run {
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
    options: super::ExecutionOptions,
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
        result.html = super::links::open_external_in_new_tabs_bounded(
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
    let cache = (!options.dry_run && !options.no_cache)
        .then(|| super::cache::ArtifactCache::new(project.root()));
    let prepared_assets = snapshot.prepare_assets(&result.stylesheets, cache.as_ref())?;
    consumed_inputs.extend_from_slice(prepared_assets.consumed_inputs());
    let mut paths = prepared_assets.output_paths();
    paths.extend(["index.html".into(), "publication.json".into()]);
    validate_assets(&paths, assets)?;
    if !options.dry_run {
        validate_staged_assets(output, &paths, assets)?;
    }
    let mut inputs = consumed_inputs
        .iter()
        .map(|input| (input.logical_name.clone(), input.sha256.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    inputs.insert(
        "composition/catalog".into(),
        assembly.catalog().input_digest().into(),
    );
    result.manifest.record_inputs(inputs)?;
    if options.dry_run {
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
fn validate_staged_assets(
    root: &Path,
    planned: &std::collections::BTreeSet<std::path::PathBuf>,
    assets: &[(std::path::PathBuf, Vec<u8>)],
) -> Result<(), Error> {
    if !root.exists() {
        return Ok(());
    }
    let existing = super::release::inventory(root)?
        .into_iter()
        .filter(|file| {
            !assets.iter().any(|(path, bytes)| {
                path == Path::new(&file.path) && input_digest(bytes) == file.sha256
            })
        })
        .map(|file| (std::path::PathBuf::from(file.path), Vec::new()))
        .collect::<Vec<_>>();
    validate_assets(planned, &existing)
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
    fn final_inventory_is_idempotent_and_manifest_budget_failure_is_transactional() {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("_site");
        std::fs::create_dir(&installed).unwrap();
        std::fs::write(installed.join("sentinel"), "existing output").unwrap();
        let transaction =
            super::super::output::OutputTransaction::new(root.path(), [Path::new("_site")])
                .unwrap();
        let staged = transaction.staging_root().join("_site");
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(staged.join("index.html"), "<p>Prepared output</p>").unwrap();
        let manifest = serde_json::json!({"render_input_fingerprint":"a".repeat(64)});
        std::fs::write(
            staged.join("publication.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let limits = PublicationLimits {
            output_bytes: 150,
            ..Default::default()
        };
        assert!(finalize_manifest(&staged, false, false, &limits).is_err());
        assert_eq!(
            std::fs::read_to_string(installed.join("sentinel")).unwrap(),
            "existing output"
        );
        finalize_manifest(&staged, false, false, &PublicationLimits::default()).unwrap();
        let first = std::fs::read(staged.join("publication.json")).unwrap();
        finalize_manifest(&staged, false, false, &PublicationLimits::default()).unwrap();
        assert_eq!(
            std::fs::read(staged.join("publication.json")).unwrap(),
            first
        );
        drop(transaction);
        assert_eq!(
            std::fs::read_to_string(installed.join("sentinel")).unwrap(),
            "existing output"
        );
    }
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
                    queries: Default::default(),
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
                    super::super::ExecutionOptions::default(),
                    &mut vec![],
                    &[]
                )
                .is_err(),
                "{failure}"
            );
            if failure == "manifest" {
                assert!(
                    staged.join("static/a.txt").is_file(),
                    "manifest failure must follow an actual staged write"
                );
                assert!(staged.join("index.html").is_file());
            } else {
                assert!(
                    !staged.join("static/a.txt").exists(),
                    "staged path collisions must fail before copying assets"
                );
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
