//! Captured public editions and editable channel exports. Never reads authoring files.
use super::*;
use berlin_document::{Block, ContentId};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs};

pub(crate) const LIMIT: usize = 16 * 1024 * 1024;
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Image {
    pub url: Url,
    pub alt: String,
    pub path: String,
    pub sha256: String,
}
impl Image {
    pub(super) fn filename(&self) -> Result<String, Error> {
        let extension = Path::new(&self.path)
            .extension()
            .and_then(|s| s.to_str())
            .context("teaser image requires a file extension")?;
        if !extension.bytes().all(|b| b.is_ascii_alphanumeric()) || extension.len() > 8 {
            bail!("invalid teaser image extension");
        }
        Ok(format!("teaser.{extension}"))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Article {
    pub document: DocumentRevision,
    pub summary: String,
    pub image: Option<Image>,
    pub markdown: Option<String>,
    pub tags: Vec<String>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Articles {
    pub schema_version: u8,
    pub articles: Vec<Article>,
}

pub(crate) fn capture(
    site: &berlin_publication::SiteAssembly,
    mount: &Url,
    root: &Path,
) -> Result<Vec<u8>, Error> {
    let mount = berlin_publication::mount_url(mount)?;
    let documents = site
        .document_routes()
        .iter()
        .map(|(id, path)| {
            Ok((
                ContentId(id.leaf().into()),
                release::announcements::document_url(&mount, path)?.to_string(),
            ))
        })
        .collect::<Result<HashMap<_, _>, Error>>()?;
    let mut articles = Vec::new();
    for (id, route) in site.document_routes() {
        let document = site.catalog().get(id)?.handle().try_local_document()?;
        let mut revision = DocumentRevision::from_document(document)?;
        revision.path = route.clone();
        let original = release::announcements::document_url(&mount, route)?;
        let mut edition = super::markdown::Edition {
            original: &original,
            mount: &mount,
            documents: &documents,
            warnings: vec![],
        };
        let markdown = match edition.render(document) {
            Ok(text) if text.len() <= 2 * 1024 * 1024 => Some(text),
            Ok(_) => {
                edition
                    .warnings
                    .push("Article exceeds the 2 MiB edition budget.".into());
                None
            }
            Err(_) => {
                edition.warnings.push("Article has links or structures that need manual adaptation before syndication.".into());
                None
            }
        };
        let summary = if let Some(description) = &document.metadata.description {
            let parsed =
                ::markdown::Parser::new().parse(&::markdown::Source::in_memory(description))?;
            let mut out = Vec::new();
            berlin_document_linkedin::fragment::write_blocks(
                &parsed.blocks,
                &Default::default(),
                &mut out,
            )?;
            String::from_utf8(out)?.trim().to_owned()
        } else {
            let paragraph = document
                .blocks
                .iter()
                .find_map(|b| {
                    if let Block::Paragraph { content } = b {
                        Some(berlin_document::plain_text(content))
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            edition.warnings.push("No authored description; summary uses the first paragraph. Review it before posting.".into());
            paragraph
        };
        let image = if let Some(preview) = &document.metadata.preview {
            let url =
                berlin_publication::resolve_source_url(&preview.source, &original, Some(&mount))?;
            if url.origin() == mount.origin()
                && url.path().starts_with(mount.path())
                && url.query().is_none()
                && url.fragment().is_none()
            {
                let path = url.path().strip_prefix(mount.path()).unwrap().to_string();
                let path = percent_encoding::percent_decode_str(&path)
                    .decode_utf8()?
                    .to_string();
                if path
                    .split('/')
                    .any(|p| matches!(p, "" | "." | "..") || p.contains('\\'))
                {
                    bail!("unsafe teaser image path");
                }
                let file = root.join(&path);
                if file.is_file() && fs::metadata(&file)?.len() <= 10 * 1024 * 1024 {
                    release::ensure_file(&file)?;
                    Some(Image {
                        url,
                        alt: preview.alt.clone(),
                        path,
                        sha256: release::digest(&fs::read(file)?),
                    })
                } else {
                    edition
                        .warnings
                        .push("Teaser image is absent from this release or exceeds 10 MiB.".into());
                    None
                }
            } else {
                edition.warnings.push("External teaser image was not captured; use a release-owned image for a reproducible export.".into());
                None
            }
        } else {
            None
        };
        edition.warnings.sort();
        edition.warnings.dedup();
        articles.push(Article {
            document: revision,
            summary,
            image,
            markdown,
            tags: document.metadata.tags.clone(),
            warnings: edition.warnings,
        });
    }
    articles.sort_by(|a, b| a.document.id.0.cmp(&b.document.id.0));
    let bytes = serde_json::to_vec(&Articles {
        schema_version: 1,
        articles,
    })?;
    if bytes.len() > LIMIT {
        bail!("public edition snapshot exceeds 16 MiB");
    }
    Ok(bytes)
}

pub(crate) fn load(
    project: &Project,
    id: &str,
) -> Result<(release::WebsiteRelease, Articles), Error> {
    let release = release::open(project, id)?;
    let file =
        release.manifest.articles.as_ref().context(
            "This release has no captured public editions; prepare a new native release.",
        )?;
    let bytes = release::read_verified_file(&release.directory, file, LIMIT)?;
    let articles: Articles = serde_json::from_slice(&bytes)?;
    if articles.schema_version != 1 {
        bail!("unsupported public edition snapshot");
    }
    let inventory = release
        .manifest
        .documents
        .as_ref()
        .context("missing document inventory")?;
    if articles.articles.len() != inventory.entries.len() {
        bail!("public edition inventory mismatch");
    }
    for (article, revision) in articles.articles.iter().zip(&inventory.entries) {
        if &article.document != revision {
            bail!("public edition revision mismatch");
        }
        if let Some(image) = &article.image
            && !release
                .manifest
                .files
                .iter()
                .any(|f| f.path == image.path && f.sha256 == image.sha256)
        {
            bail!("teaser image is not a sealed asset");
        }
    }
    Ok((release, articles))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    schema_version: u8,
    release: String,
    document: String,
    kind: AnnouncementKind,
}

pub(crate) fn prepare(
    project: &Project,
    id: &str,
    document: &str,
    output: &Path,
    updated: bool,
) -> Result<serde_json::Value, Error> {
    let (sealed, articles) = load(project, id)?;
    let article = articles
        .articles
        .iter()
        .find(|a| a.document.id.0 == document)
        .context("document is not public in this release")?;
    let url = release::announcements::document_url(&sealed.manifest.url, &article.document.path)?;
    let output = release::announcements::export_directory(project, output)?;
    if output.exists() {
        bail!("export destination already exists; choose a new directory");
    }
    let staging = tempfile::Builder::new()
        .prefix(".announcement-")
        .tempdir_in(output.parent().unwrap())?;
    let directory = staging.path();
    let prefix = if updated { "Updated" } else { "New" };
    let linkedin = format!(
        "{prefix}: {}\n\n{}\n\n{url}",
        article.document.title, article.summary
    );
    fs::write(directory.join("linkedin.txt"), linkedin)?;
    fs::write(
        directory.join("x.txt"),
        format!("{}\n\n{}\n\n{url}", article.document.title, article.summary),
    )?;
    if let Some(markdown) = &article.markdown {
        fs::write(directory.join("article.md"), markdown)?;
        let substack = super::substack::prepare(article, &url)?;
        fs::write(directory.join("substack.json"), substack)?;
        let mut seen_tags = std::collections::BTreeSet::new();
        let tags = article
            .tags
            .iter()
            .map(|t| {
                t.chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_lowercase()
            })
            .filter(|t| !t.is_empty() && t.len() <= 30)
            .filter(|t| seen_tags.insert(t.clone()))
            .take(4)
            .collect::<Vec<_>>();
        let payload = serde_json::json!({"article":{"title":article.document.title,"body_markdown":markdown,"published":false,"canonical_url":url,"main_image":article.image.as_ref().map(|i|i.url.as_str()),"tags":tags.join(", ")}});
        fs::write(
            directory.join("devto.json"),
            serde_json::to_vec_pretty(&payload)?,
        )?;
    }
    if let Some(image) = &article.image {
        let asset = sealed
            .manifest
            .files
            .iter()
            .find(|f| f.path == image.path)
            .context("missing sealed teaser asset")?;
        let bytes = release::read_verified_file(&sealed.site(), asset, 10 * 1024 * 1024)?;
        fs::write(directory.join(image.filename()?), bytes)?;
        fs::write(directory.join("teaser-alt.txt"), &image.alt)?;
    }
    let package = Package {
        schema_version: 2,
        release: id.into(),
        document: document.into(),
        kind: if updated {
            AnnouncementKind::Updated
        } else {
            AnnouncementKind::New
        },
    };
    fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&package)?,
    )?;
    fs::write(
        directory.join("warnings.json"),
        serde_json::to_vec_pretty(&article.warnings)?,
    )?;
    for entry in fs::read_dir(directory)? {
        fs::File::open(entry?.path())?.sync_all()?;
    }
    fs::rename(directory, &output)?;
    release::sync_directory(output.parent().unwrap())?;
    Ok(
        serde_json::json!({"directory":output,"release":id,"document":document,"warnings":article.warnings,"channels":if article.markdown.is_some() {vec!["linkedin","x","devto","substack"]} else {vec!["linkedin","x"]},"remote_io":false}),
    )
}

pub(super) fn snapshot(
    project: &Project,
    directory: &Path,
    document: &str,
    account: &str,
    channel: Channel,
) -> Result<Snapshot, Error> {
    let directory = project.root().join(directory);
    release::ensure_directory(&directory)?;
    let package: Package = serde_json::from_str(&super::review::read_text(
        &directory.join("manifest.json"),
        64 * 1024,
    )?)?;
    if package.schema_version != 2 || package.document != document {
        bail!("package document/version mismatch");
    }
    let (sealed, articles) = load(project, &package.release)?;
    let article = articles
        .articles
        .iter()
        .find(|a| a.document.id.0 == document)
        .context("document absent from sealed edition")?;
    let text = super::review::read_text(
        &directory.join(match channel {
            Channel::LinkedIn => "linkedin.txt",
            Channel::X => "x.txt",
            Channel::DevTo => "devto.json",
            Channel::Substack => "substack.json",
        }),
        2 * 1024 * 1024,
    )?;
    if let Some(image) = &article.image {
        let file = directory.join(image.filename()?);
        release::ensure_file(&file)?;
        if fs::metadata(&file)?.len() > 10 * 1024 * 1024
            || release::digest(&fs::read(file)?) != image.sha256
        {
            bail!("teaser image changed; export a new package");
        }
        if super::review::read_text(&directory.join("teaser-alt.txt"), 64 * 1024)? != image.alt {
            bail!("teaser alternative text changed; export a new package");
        }
    }
    let linkedin = if channel == Channel::LinkedIn {
        super::linkedin::connection::target(project, account)?
    } else {
        None
    };
    let mut warnings = article.warnings.clone();
    let substack = if channel == Channel::Substack {
        let original =
            release::announcements::document_url(&sealed.manifest.url, &article.document.path)?;
        let (body, adaptations) = super::substack::render(&text, &original)?;
        warnings.extend(adaptations);
        warnings.sort();
        warnings.dedup();
        Some(body)
    } else {
        None
    };
    let snapshot = Snapshot {
        schema_version: 2,
        channel,
        account: account.into(),
        before_release: package.release.clone(),
        after_release: package.release,
        website_url: sealed.manifest.url.clone(),
        website_destination: sealed.manifest.destination.clone(),
        fingerprint_version: 1,
        document: article.document.clone(),
        kind: package.kind,
        article_url: release::announcements::document_url(
            &sealed.manifest.url,
            &article.document.path,
        )?,
        text_sha256: release::digest(text.as_bytes()),
        text,
        image: article.image.clone(),
        linkedin,
        substack,
        warnings,
    };
    snapshot.validate()?;
    Ok(snapshot)
}
