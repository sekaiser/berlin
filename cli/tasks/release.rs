//! Sealed website bundles. No credentials, network calls, or source rewrites.
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Error, bail};
use berlin_core::{DeploymentTarget, Operation, PipelineLoader as _, PipelineNode, PipelinePlan};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::project::Project;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub schema_version: u8,
    pub berlin_version: String,
    pub pipeline: String,
    pub website_node: String,
    pub url: Url,
    pub destination: DeploymentTarget,
    pub files: Vec<ReleaseFile>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReleaseFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

/// Obtained only by sealing or verifying a complete bundle. Reverify before use.
pub(crate) struct WebsiteRelease {
    pub id: String,
    pub manifest: Manifest,
    pub directory: PathBuf,
}

impl WebsiteRelease {
    pub fn site(&self) -> PathBuf {
        self.directory.join("site")
    }
}

pub(super) fn seal(
    project: &Project,
    pipeline: &str,
    plan: &PipelinePlan,
    staging: &Path,
) -> Result<WebsiteRelease, Error> {
    let node = website_output(plan)?;
    let source = staging.join(
        node.output_path
            .as_ref()
            .context("website has no output directory")?,
    );
    seal_output(project, pipeline, node, &source)
}

/// Import externally prepared bytes. The selected pipeline declares identity and
/// destination only: none of its build operations execute.
pub(crate) fn from_directory(
    project: &Project,
    pipeline: &str,
    directory: &Path,
) -> Result<WebsiteRelease, Error> {
    let _lock = super::output::ProjectLock::acquire(project.root())?;
    let program = crate::pipeline::load_pipeline_program(project)?;
    let plan = program
        .load(pipeline)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    plan.validate()?;
    let node = website_output(&plan)?;
    let source = project.root().join(directory);
    ensure_directory(&source)?;
    let source = source.canonicalize()?;
    seal_output(project, pipeline, node, &source)
}

fn website_output(plan: &PipelinePlan) -> Result<&PipelineNode, Error> {
    let websites = plan
        .nodes()
        .iter()
        .filter(|node| matches!(node.operation, Operation::RenderWebsite { .. }))
        .collect::<Vec<_>>();
    let [node] = websites.as_slice() else {
        bail!("release requires exactly one website renderer");
    };
    let output = node
        .output_path
        .as_ref()
        .context("website has no output directory")?;
    // Do not silently omit another declared output from a website release.
    for path in plan
        .nodes()
        .iter()
        .filter_map(|node| node.output_path.as_ref())
    {
        if !path.starts_with(output) {
            bail!("release output '{}' is outside the website", path.display());
        }
    }
    Ok(node)
}

fn seal_output(
    project: &Project,
    pipeline: &str,
    node: &PipelineNode,
    source: &Path,
) -> Result<WebsiteRelease, Error> {
    let Operation::RenderWebsite { config } = &node.operation else {
        unreachable!()
    };
    let url = config
        .url
        .clone()
        .context("release requires a website URL")?;
    validate_url(&url)?;
    let destination = node
        .deployment
        .clone()
        .context("website needs .deploy_to(github_pages(\"owner/repository\"))")?;
    destination.validate().map_err(Error::msg)?;
    let files = inventory(source)?;
    // Check before creating state: even an invalid input must remain untouched.
    if project
        .root()
        .canonicalize()?
        .join(".berlin/releases")
        .starts_with(source.canonicalize()?)
    {
        bail!("prepared website directory must not contain release state");
    }
    let releases = state_directory(project.root(), "releases")?;
    let temporary = tempfile::Builder::new()
        .prefix(".preparing-")
        .tempdir_in(&releases)?;
    let site = temporary.path().join("site");
    // Add support files only to our private copy, never the caller's directory.
    copy_site(source, &site, &files)?;
    add_file(&site, ".nojekyll", b"")?;
    let DeploymentTarget::GitHubPages { repository } = &destination;
    let owner = repository.split('/').next().expect("validated repository");
    let default_host = format!("{}.github.io", owner.to_lowercase());
    if url.host_str() != Some(default_host.as_str()) {
        add_file(
            &site,
            "CNAME",
            format!("{}\n", url.host_str().expect("validated URL")).as_bytes(),
        )?;
    } else if site.join("CNAME").try_exists()? {
        bail!("default github.io release must not contain a custom-domain CNAME");
    }
    for (name, bytes) in [
        ("LICENSE.txt", include_bytes!("../../LICENSE").as_slice()),
        ("NOTICE.txt", include_bytes!("../../NOTICE").as_slice()),
        (
            "THIRD_PARTY_NOTICES.txt",
            include_bytes!("../THIRD_PARTY_NOTICES.txt").as_slice(),
        ),
    ] {
        add_file(&site, &format!("licenses/berlin/{name}"), bytes)?;
    }
    let files = inventory(&site)?;
    validate_site(&site, &files)?;
    let manifest = Manifest {
        schema_version: 1,
        berlin_version: crate::version::berlin().to_string(),
        pipeline: pipeline.into(),
        website_node: node.id.as_str().into(),
        url,
        destination,
        files,
    };
    let id = digest(&serde_json::to_vec(&manifest)?);
    let directory = releases.join(&id);
    if directory.try_exists()? {
        return open(project, &id);
    }
    sync_tree(&site)?;
    atomic_json(&temporary.path().join("manifest.json"), &manifest)?;
    fs::rename(temporary.path(), &directory)?;
    sync_directory(&releases)?;
    open(project, &id)
}

pub(crate) fn open(project: &Project, id: &str) -> Result<WebsiteRelease, Error> {
    validate_id(id)?;
    let releases = project.root().join(".berlin/releases");
    ensure_directory(&project.root().join(".berlin"))?;
    ensure_directory(&releases)?;
    let directory = releases.join(id);
    ensure_directory(&directory)?;
    let manifest_path = directory.join("manifest.json");
    ensure_file(&manifest_path)?;
    let manifest: Manifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
    if manifest.schema_version != 1 || digest(&serde_json::to_vec(&manifest)?) != id {
        bail!("release manifest does not match its ID");
    }
    validate_url(&manifest.url)?;
    manifest.destination.validate().map_err(Error::msg)?;
    let site = directory.join("site");
    let files = inventory(&site)?;
    if files != manifest.files {
        bail!("release files changed; refusing to publish unreviewed bytes");
    }
    validate_site(&site, &files)?;
    Ok(WebsiteRelease {
        id: id.into(),
        manifest,
        directory,
    })
}

pub(crate) fn validate_id(id: &str) -> Result<(), Error> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("release ID must be a lowercase SHA-256 digest");
    }
    Ok(())
}

fn validate_url(url: &Url) -> Result<(), Error> {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port().is_some()
        || !matches!(url.host(), Some(url::Host::Domain(host)) if host != "localhost" && !host.ends_with(".localhost"))
    {
        bail!(
            "release requires a public HTTPS website URL without credentials, port, query, or fragment"
        );
    }
    Ok(())
}

fn validate_site(root: &Path, files: &[ReleaseFile]) -> Result<(), Error> {
    if !files
        .iter()
        .any(|file| file.path == "index.html" && file.bytes > 0)
    {
        bail!("release needs a nonempty index.html");
    }
    if !files.iter().any(|file| file.path == ".nojekyll") {
        bail!("Pages release needs .nojekyll");
    }
    for file in files.iter().filter(|file| file.path.ends_with(".html")) {
        let text = fs::read_to_string(root.join(&file.path))?;
        // This is a preview-leak guard, not an HTML/link validator or secret scanner.
        for marker in [
            "href=\"http://localhost:",
            "src=\"http://localhost:",
            "href=\"http://127.0.0.1:",
            "src=\"http://127.0.0.1:",
            "src=\"/__berlin/",
            "src=\"/static/js/live.js",
        ] {
            if text.contains(marker) {
                bail!("preview-only reference in {}: {marker}", file.path);
            }
        }
    }
    Ok(())
}

pub(crate) fn inventory(root: &Path) -> Result<Vec<ReleaseFile>, Error> {
    ensure_directory(root)?;
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<ReleaseFile>) -> Result<(), Error> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root)?;
        let name = relative
            .to_str()
            .context("release paths must be UTF-8")?
            .to_owned();
        if relative.components().any(is_control_path) {
            bail!("private or Git control path in release: {name}");
        }
        if name.contains('\\') || name.chars().any(char::is_control) {
            bail!("unsafe release path");
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            collect_files(root, &path, files)?;
        } else if metadata.is_file() {
            // GitHub's ordinary Git transport rejects files at/over 100 MiB.
            if metadata.len() >= 100 * 1024 * 1024 {
                bail!("release file is too large for Git: {name}");
            }
            let mut file = File::open(&path)?;
            let mut hasher = Sha256::new();
            let mut buffer = [0; 64 * 1024];
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                hasher.update(&buffer[..count]);
            }
            files.push(ReleaseFile {
                path: name,
                bytes: metadata.len(),
                sha256: hex(&hasher.finalize()),
            });
        } else {
            bail!("release contains a symlink or special file: {name}");
        }
    }
    Ok(())
}

fn is_control_path(component: Component<'_>) -> bool {
    let Component::Normal(name) = component else {
        return true;
    };
    name.to_str().is_none_or(|name| {
        [
            ".git",
            ".github",
            ".gitattributes",
            ".gitmodules",
            ".berlin",
        ]
        .iter()
        .any(|reserved| name.eq_ignore_ascii_case(reserved))
    })
}

pub(crate) fn copy_site(
    source: &Path,
    destination: &Path,
    files: &[ReleaseFile],
) -> Result<(), Error> {
    fs::create_dir(destination)?;
    for file in files {
        let target = destination.join(&file.path);
        fs::create_dir_all(target.parent().context("file has no parent")?)?;
        let mut output = File::create(&target)?;
        std::io::copy(&mut File::open(source.join(&file.path))?, &mut output)?;
        output.sync_all()?;
    }
    if inventory(destination)? != files {
        bail!("release changed during copying");
    }
    sync_tree(destination)?;
    Ok(())
}

fn add_file(root: &Path, path: &str, bytes: &[u8]) -> Result<(), Error> {
    let path = root.join(path);
    if path.try_exists()? {
        ensure_file(&path)?;
        if fs::read(&path)? != bytes {
            bail!("release notice conflicts with {}", path.display());
        }
    } else {
        fs::create_dir_all(path.parent().context("file has no parent")?)?;
        let mut file = File::create(path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    Ok(())
}

pub(crate) fn state_directory(root: &Path, name: &str) -> Result<PathBuf, Error> {
    let state = root.join(".berlin");
    for path in [&state, &state.join(name)] {
        match fs::create_dir(path) {
            Ok(()) => sync_directory(path.parent().context("directory has no parent")?)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        ensure_directory(path)?;
    }
    Ok(state.join(name))
}

pub(crate) fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    let parent = path.parent().context("state file has no parent")?;
    ensure_directory(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    sync_directory(parent)
}

fn sync_tree(path: &Path) -> Result<(), Error> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            sync_tree(&entry.path())?;
        }
    }
    sync_directory(path)
}

fn sync_directory(path: &Path) -> Result<(), Error> {
    File::open(path)?.sync_all()?;
    Ok(())
}
pub(crate) fn ensure_directory(path: &Path) -> Result<(), Error> {
    if !fs::symlink_metadata(path)?.is_dir() {
        bail!("expected a real directory: {}", path.display());
    }
    Ok(())
}
pub(crate) fn ensure_file(path: &Path) -> Result<(), Error> {
    if !fs::symlink_metadata(path)?.is_file() {
        bail!("expected a regular file: {}", path.display());
    }
    Ok(())
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
