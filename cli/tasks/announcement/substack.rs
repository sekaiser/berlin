//! A reviewed local edition for Substack's editor, not a private publishing API.
use super::*;
use comrak::{Arena, Options, nodes::NodeValue};
use std::{io::Write, path::PathBuf};

pub(super) const INPUT_LIMIT: usize = 512 * 1024;
const HTML_LIMIT: usize = 2 * 1024 * 1024;
const PREVIEW_LIMIT: u64 = 4 * 1024 * 1024;
const MARKER: &str = "<meta name=\"berlin-substack-preview\" content=\"1\">";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Draft {
    pub schema_version: u8,
    pub title: String,
    pub subtitle: String,
    pub publication_url: Option<Url>,
    pub source_url: Url,
    pub audience: Audience,
    pub send_email: bool,
    pub body_markdown: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Audience {
    Everyone,
    Free,
    Paid,
    Founding,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Body {
    schema_version: u8,
    pub html: String,
}
fn root(url: &Url) -> Result<(), Error> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        bail!("publication_url must be the HTTPS root of your Substack or custom domain");
    }
    Ok(())
}
pub(super) fn read(text: &str, original: &Url) -> Result<Draft, Error> {
    if original.scheme() != "https"
        || !original.username().is_empty()
        || original.password().is_some()
    {
        bail!("Substack source_url must identify the public HTTPS article");
    }
    if text.len() > INPUT_LIMIT {
        bail!("Substack draft exceeds the 512 KiB local edition budget");
    }
    let draft: Draft = serde_json::from_str(text)?;
    if draft.schema_version != 1
        || draft.source_url != *original
        || draft.title.trim().is_empty()
        || draft.title.len() > 1024
        || draft.subtitle.len() > 4096
        || draft.title.chars().any(char::is_control)
        || draft.subtitle.chars().any(char::is_control)
        || draft.body_markdown.trim().is_empty()
        || draft
            .body_markdown
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        || draft.body_markdown.trim_start().starts_with("---")
    {
        bail!(
            "Substack requires a title, nonempty body without frontmatter, and the sealed source_url"
        );
    }
    if let Some(url) = &draft.publication_url {
        root(url)?;
    }
    Ok(draft)
}
pub(super) fn has_destination(text: &str) -> bool {
    serde_json::from_str::<Draft>(text)
        .is_ok_and(|draft| draft.publication_url.is_some_and(|url| root(&url).is_ok()))
}
pub(super) fn validate(snapshot: &Snapshot, body: &Body) -> Result<(), Error> {
    read(&snapshot.text, &snapshot.article_url)?;
    if body.schema_version != 1 || body.html.is_empty() || body.html.len() > HTML_LIMIT {
        bail!("invalid or oversized captured Substack HTML");
    }
    Ok(())
}
pub(super) fn prepare(article: &packages::Article, original: &Url) -> Result<String, Error> {
    let markdown = article
        .markdown
        .as_ref()
        .context("missing static edition")?;
    let attribution = format!("Originally published at {original}.");
    // Attribution is appended by the HTML renderer and cannot be edited away.
    let markdown = markdown.trim_end();
    let body = markdown
        .strip_suffix(&attribution)
        .unwrap_or(markdown)
        .trim_end();
    Ok(serde_json::to_string_pretty(&Draft {
        schema_version: 1,
        title: article.document.title.clone(),
        subtitle: article
            .summary
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        publication_url: None,
        source_url: original.clone(),
        audience: Audience::Everyone,
        send_email: false,
        body_markdown: body.into(),
    })?)
}
pub(super) fn render(text: &str, original: &Url) -> Result<(Body, Vec<String>), Error> {
    let draft = read(text, original)?;
    let mut warnings = vec!["Manual delivery only: check figures, captions, code, tables and footnotes in Substack's editor. Audience and email settings must be selected there; Berlin records intentions, not remote delivery.".into()];
    if draft.publication_url.is_none() {
        warnings.push("Set publication_url in substack.json before approving this edition.".into());
    }
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.footnotes = true;
    options.extension.strikethrough = true;
    // Raw HTML remains disabled, including iframe/script embeds and handlers.
    let document = comrak::parse_document(&arena, &draft.body_markdown, &options);
    let nodes = document.descendants().collect::<Vec<_>>();
    for node in nodes {
        let mut ast = node.data.borrow_mut();
        match &mut ast.value {
            NodeValue::Link(link) | NodeValue::Image(link) => {
                let url = original.join(&link.url)?;
                if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some()
                {
                    bail!("Substack links and images must resolve to public HTTPS URLs");
                }
                // Substack may rewrite heading IDs. Source-section links retain
                // their meaning on the original site; generated footnotes stay local.
                link.url = url.to_string();
            }
            NodeValue::HtmlInline(value) if matches!(value.trim(), "<br>" | "<br/>" | "<br />") => {
                ast.value = NodeValue::LineBreak;
            }
            NodeValue::HtmlBlock(_) | NodeValue::HtmlInline(_) | NodeValue::Raw(_) => {
                warnings.push("Raw HTML and explicit source anchors were omitted from this edition. Embeds need manual adaptation.".into());
                node.detach();
            }
            _ => {}
        }
    }
    struct BoundedHtml(String);
    impl std::fmt::Write for BoundedHtml {
        fn write_str(&mut self, value: &str) -> std::fmt::Result {
            if self.0.len().saturating_add(value.len()) > HTML_LIMIT {
                return Err(std::fmt::Error);
            }
            self.0.push_str(value);
            Ok(())
        }
    }
    let mut output = BoundedHtml(String::new());
    comrak::format_html(document, &options, &mut output)
        .map_err(|_| anyhow::anyhow!("Substack HTML exceeds the 2 MiB local edition budget"))?;
    let mut html = output.0;
    let url = html_escape::encode_double_quoted_attribute(original.as_str());
    html.push_str(&format!(
        "\n<p>Originally published at <a href=\"{url}\">{url}</a>.</p>\n"
    ));
    if html.len() > HTML_LIMIT {
        bail!("Substack HTML exceeds the 2 MiB local edition budget");
    }
    warnings.sort();
    warnings.dedup();
    Ok((
        Body {
            schema_version: 1,
            html,
        },
        warnings,
    ))
}
pub(super) fn check_publication(text: &str, url: &Url) -> Result<(), Error> {
    let draft: Draft = serde_json::from_str(text)?;
    let publication = draft
        .publication_url
        .context("set publication_url before recording a post")?;
    root(&publication)?;
    if publication.origin() != url.origin() {
        bail!("Substack post does not belong to the approved publication URL");
    }
    Ok(())
}
fn page(snapshot: &Snapshot) -> Result<String, Error> {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let draft = read(&snapshot.text, &snapshot.article_url)?;
    let body = snapshot
        .substack
        .as_ref()
        .context("missing Substack HTML")?;
    let template = include_str!("substack/preview.html");
    let digest = |tag: &str| {
        let value = template
            .split_once(&format!("<{tag}>"))
            .unwrap()
            .1
            .split_once(&format!("</{tag}>"))
            .unwrap()
            .0;
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(value.as_bytes()))
    };
    let context = tera::Context::from_serialize(serde_json::json!({
        "draft":draft,"body":body.html,"image":snapshot.image,
        "warnings":snapshot.warnings,"review_id":snapshot.id()?,
        "script_sha":digest("script"),"style_sha":digest("style")
    }))?;
    Ok(tera::Tera::one_off(template, &context, true)?)
}
fn path(project: &Project, source: &Source) -> PathBuf {
    project.root().join(&source.directory).join("substack.html")
}
pub(super) fn check_preview(
    project: &Project,
    source: &Source,
    snapshot: &Snapshot,
) -> Result<(), Error> {
    let expected = page(snapshot)?;
    let actual = super::review::read_text(&path(project, source), PREVIEW_LIMIT).context(
        "generate a Substack preview with 'announcement preview' before approving or recording",
    )?;
    if expected != actual {
        bail!(
            "Substack preview is stale or edited; run 'announcement preview' again and inspect it"
        );
    }
    Ok(())
}
pub(crate) fn preview(project: &Project, source: &Source) -> Result<serde_json::Value, Error> {
    if source.channel != "substack" {
        bail!("formatted preview currently supports only Substack");
    }
    let snapshot = super::load_source(project, source)?;
    let html = page(&snapshot)?;
    if html.len() as u64 > PREVIEW_LIMIT {
        bail!("Substack preview exceeds its 4 MiB budget");
    }
    let path = path(project, source);
    if path.try_exists()? {
        let previous = super::review::read_text(&path, PREVIEW_LIMIT)?;
        if !previous.contains(MARKER) {
            bail!("refusing to overwrite a file that is not a generated Substack preview");
        }
    }
    let parent = path.parent().unwrap();
    release::ensure_directory(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(html.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(&path).map_err(|error| error.error)?;
    release::sync_directory(parent)?;
    Ok(
        serde_json::json!({"path":path,"review_id":snapshot.id()?,"approval_allowed":snapshot.approval_allowed(),"warnings":snapshot.warnings,"remote_io":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draft(body: &str) -> (Url, String) {
        let original = Url::parse("https://example.test/notes/note.html").unwrap();
        let text = serde_json::to_string(&Draft {
            schema_version: 1,
            title: "Title <&>".into(),
            subtitle: "Subtitle".into(),
            publication_url: Some(Url::parse("https://writer.substack.com/").unwrap()),
            source_url: original.clone(),
            audience: Audience::Everyone,
            send_email: false,
            body_markdown: body.into(),
        })
        .unwrap();
        (original, text)
    }
    #[test]
    fn static_edition_preserves_technical_content_and_resolves_urls_without_fetching() {
        let (url, text) = draft(
            "## Example\n\nA **clear** explanation with [context](#section) and a note.[^n]\n\n![Diagram](../static/diagram.png)\n\nA caption.\n\n```rust\nlet x = \"<tag>\";\n```\n\n| Name | Value |\n| --- | --- |\n| A | 1 |\n\n[^n]: Evidence.\n",
        );
        let (body, _) = render(&text, &url).unwrap();
        for expected in [
            "<h2>Example</h2>",
            "<strong>clear</strong>",
            "https://example.test/notes/note.html#section",
            "https://example.test/static/diagram.png",
            "A caption.",
            "<table>",
            "Evidence.",
            "&lt;tag&gt;",
            "Originally published at",
        ] {
            assert!(body.html.contains(expected), "missing {expected}");
        }
        assert!(!body.html.contains("Copy code"));
        assert!(!body.html.contains("<script"));
    }
    #[test]
    fn active_html_is_omitted_and_nonpublic_urls_or_frontmatter_refuse_rendering() {
        let (url, text) = draft(
            "<script>steal()</script>\n\n<iframe src=\"https://evil.test/\"></iframe>\n\nText.<img src=x onerror=steal()>\n",
        );
        let (body, warnings) = render(&text, &url).unwrap();
        for dangerous in ["<script", "<iframe", "onerror=", "src=x"] {
            assert!(!body.html.contains(dangerous));
        }
        assert!(warnings.iter().any(|w| w.contains("Raw HTML")));
        for unsafe_body in [
            "[click](javascript:alert%281%29)",
            "![image](file:///secret)",
            "[local](http://localhost/)",
            "[credentials](https://name:secret@example.test/)",
            "---\ntitle: override\n---\nBody",
        ] {
            let (url, text) = draft(unsafe_body);
            assert!(render(&text, &url).is_err(), "accepted {unsafe_body}");
        }
    }
    #[test]
    fn malformed_settings_and_resource_limits_fail_locally() {
        let (url, text) = draft("Body");
        for (field, invalid_value) in [
            ("source_url", serde_json::json!("https://other.test/note")),
            (
                "publication_url",
                serde_json::json!("https://writer.substack.com/p/a-post"),
            ),
            (
                "publication_url",
                serde_json::json!("http://writer.substack.com"),
            ),
            (
                "publication_url",
                serde_json::json!("https://writer.substack.com?tracking=1"),
            ),
            ("send_email", serde_json::json!("yes")),
            ("audience", serde_json::json!("subscribers")),
            ("body_markdown", serde_json::json!("bad\u{0000}body")),
        ] {
            let mut invalid: serde_json::Value = serde_json::from_str(&text).unwrap();
            invalid[field] = invalid_value;
            assert!(
                render(&invalid.to_string(), &url).is_err(),
                "accepted invalid {field}"
            );
        }
        assert!(read(&"x".repeat(INPUT_LIMIT + 1), &url).is_err());
        let mut local_source: serde_json::Value = serde_json::from_str(&text).unwrap();
        let local_url = Url::parse("http://localhost/note.html").unwrap();
        local_source["source_url"] = serde_json::json!(local_url);
        assert!(read(&local_source.to_string(), &local_url).is_err());
        let (url, expanded) = draft(&"& ".repeat(220_000));
        // Input fits its budget; HTML escaping must still honor the output budget.
        assert!(expanded.len() < INPUT_LIMIT);
        let _ = render(&expanded, &url).unwrap();
        let (url, expanded) = draft(&"&".repeat(450_000));
        assert!(expanded.len() < INPUT_LIMIT);
        assert!(render(&expanded, &url).is_err());
    }
}
