use super::*;
use crate::tasks::release;
use anyhow::Context;
use std::path::{Path, PathBuf};
use std::{fs::File, io::Read};

pub(crate) const SOURCE_LIMIT: u64 = 1024 * 1024;
pub(crate) fn read_text(path: &Path, limit: u64) -> Result<String, Error> {
    release::ensure_file(path)?;
    let mut value = String::new();
    File::open(path)?
        .take(limit + 1)
        .read_to_string(&mut value)?;
    ensure!(
        value.len() as u64 <= limit,
        "review input exceeds its size limit"
    );
    Ok(value)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Snapshot {
    pub path: PathBuf,
    pub document_id: String,
    pub title: String,
    pub sha256: String,
    pub text: String,
}
impl Snapshot {
    pub(crate) fn capture(path: &Path) -> Result<Self, Error> {
        // Read exactly once, before deriving identity or revision. No exporter or
        // Babel is invoked; review the authored source, including unpublished drafts.
        release::ensure_file(path)?;
        let path = path.canonicalize()?;
        let text = read_text(&path, SOURCE_LIMIT)?;
        let (document_id, title) = identity(&path, &text)?;
        Ok(Self {
            path,
            document_id,
            title,
            sha256: release::digest(text.as_bytes()),
            text,
        })
    }
    pub(crate) fn validate(&self) -> Result<(), Error> {
        ensure!(
            self.path.is_absolute() && self.text.len() as u64 <= SOURCE_LIMIT,
            "invalid review source path/size"
        );
        ensure!(
            release::digest(self.text.as_bytes()) == self.sha256,
            "review source hash mismatch"
        );
        let (id, title) = identity(&self.path, &self.text)?;
        ensure!(
            id == self.document_id && title == self.title,
            "review source identity mismatch"
        );
        Ok(())
    }
    pub(crate) fn revision_status(&self) -> &'static str {
        match read_text(&self.path, SOURCE_LIMIT) {
            Ok(current) if release::digest(current.as_bytes()) == self.sha256 => "current",
            Ok(_) => "stale",
            Err(_) => "unavailable",
        }
    }
    pub(crate) fn require_current(&self) -> Result<(), Error> {
        ensure!(
            self.revision_status() == "current",
            "article changed or is unavailable; this review is historical. Start a new review of the current source"
        );
        Ok(())
    }
}
fn identity(path: &Path, source: &str) -> Result<(String, String), Error> {
    let (id, title) = match path.extension().and_then(|e| e.to_str()) {
        Some("org") => org_identity(source)?,
        Some("md" | "markdown") => {
            let uri = url::Url::from_file_path(path)
                .map_err(|_| anyhow::anyhow!("invalid Markdown path"))?
                .to_string();
            let document = markdown::Parser::new().parse(&markdown::Source::new(source, &uri)?)?;
            ensure!(
                document.id.0 != uri,
                "Markdown review requires an explicit frontmatter id"
            );
            (
                document.id.0,
                document
                    .metadata
                    .title
                    .context("article requires a title")?,
            )
        }
        _ => bail!("review supports .org, .md and .markdown articles"),
    };
    text(&id)?;
    text(&title)?;
    Ok((id, title))
}
fn org_identity(source: &str) -> Result<(String, String), Error> {
    let mut lines = source.lines().skip_while(|l| l.trim().is_empty());
    ensure!(
        lines
            .next()
            .is_some_and(|l| l.trim().eq_ignore_ascii_case(":PROPERTIES:")),
        "Org review requires a file-level property drawer with an ID"
    );
    let mut id = None;
    let mut ended = false;
    for line in lines.by_ref() {
        let line = line.trim();
        if line.eq_ignore_ascii_case(":END:") {
            ended = true;
            break;
        }
        if line
            .get(..4)
            .is_some_and(|v| v.eq_ignore_ascii_case(":ID:"))
        {
            ensure!(id.is_none(), "duplicate file-level Org ID");
            id = Some(line[4..].trim().to_owned());
        }
    }
    ensure!(ended, "unclosed Org property drawer");
    let mut title = None;
    // Only file keywords before the first heading/body block establish the title.
    for line in lines {
        if line.starts_with('*')
            || line
                .trim_start()
                .get(..8)
                .is_some_and(|v| v.eq_ignore_ascii_case("#+begin_"))
        {
            break;
        }
        if line
            .get(..8)
            .is_some_and(|v| v.eq_ignore_ascii_case("#+TITLE:"))
        {
            ensure!(title.is_none(), "duplicate Org title");
            title = Some(line[8..].trim().to_owned());
        }
    }
    Ok((
        id.context("Org review requires a file-level ID")?,
        title.context("article requires #+TITLE")?,
    ))
}
