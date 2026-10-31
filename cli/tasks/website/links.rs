//! Website-level link behavior, applied to complete HTML including raw author markup.
//! Only off-origin web navigation is rewritten; other URL schemes remain untouched.

use anyhow::{Context, Error};
use lol_html::{HtmlRewriter, Settings, element};
use url::Url;

pub(in crate::tasks) fn open_external_in_new_tabs(
    html: &str,
    site_url: &Url,
) -> Result<String, Error> {
    open_external_in_new_tabs_bounded(html, site_url, usize::MAX)
}
pub(in crate::tasks) fn open_external_in_new_tabs_bounded(
    html: &str,
    site_url: &Url,
    maximum: usize,
) -> Result<String, Error> {
    let mut output = berlin_publication::BoundedWriter::new(maximum);
    let mut failure = None;
    let mut rewriter = HtmlRewriter::new(
        Settings::new().append_element_content_handler(element!("a[href]", |link| {
            let href = link.get_attribute("href").expect("selector requires href");
            let href = html_escape::decode_html_entities(&href);
            let Ok(destination) = site_url.join(href.trim()) else {
                return Ok(());
            };
            if !matches!(destination.scheme(), "http" | "https")
                || destination.origin() == site_url.origin()
            {
                return Ok(());
            }

            link.set_attribute("target", "_blank")?;
            let mut rel = link.get_attribute("rel").unwrap_or_default();
            if !rel
                .split_ascii_whitespace()
                .any(|token| token.eq_ignore_ascii_case("noopener"))
            {
                if !rel.is_empty() {
                    rel.push(' ');
                }
                rel.push_str("noopener");
                link.set_attribute("rel", &rel)?;
            }
            Ok(())
        })),
        |bytes: &[u8]| {
            use std::io::Write;
            if failure.is_none() {
                failure = output.write_all(bytes).err();
            }
        },
    );
    rewriter
        .write(html.as_bytes())
        .context("Unable to apply external website link behavior")?;
    rewriter
        .end()
        .context("Unable to finish external website link behavior")?;
    if let Some(error) = failure {
        return Err(error.into());
    }
    Ok(String::from_utf8(output.into_bytes())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewrite(html: &str) -> String {
        open_external_in_new_tabs(
            html,
            &Url::parse("https://notebook.example/journal/").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn opens_external_web_links_and_preserves_existing_relationships() {
        assert_eq!(
            rewrite(r#"<a href="https://other.example/?a=1&amp;b=2" rel="nofollow">Other</a>"#),
            r#"<a href="https://other.example/?a=1&amp;b=2" rel="nofollow noopener" target="_blank">Other</a>"#
        );
        for href in [
            "//other.example/path",
            "HTTP://OTHER.EXAMPLE/path",
            "https://notebook.example:8443/path",
        ] {
            let html = rewrite(&format!(r#"<a href="{href}">Other</a>"#));
            assert!(html.contains(r#"target="_blank""#));
            assert!(html.contains(r#"rel="noopener""#));
        }
    }

    #[test]
    fn keeps_same_origin_links_fragments_downloads_and_non_web_schemes_unchanged() {
        for href in [
            "/notes/one.html",
            "../one.html",
            "#references",
            "https://notebook.example/journal/one.html",
            "//notebook.example/file.pdf",
            "mailto:author@example.com",
            "tel:+491234",
            "data:text/plain,hello",
        ] {
            let html = format!(r#"<a href="{href}" download>Link</a>"#);
            assert_eq!(rewrite(&html), html);
        }
    }

    #[test]
    fn is_idempotent_and_does_not_rewrite_code_or_script_text() {
        let html = r#"<a href="https://other.example" rel="NOOPENER noreferrer" target="named">Other</a><pre>&lt;a href="https://other.example"&gt;</pre><script>const source = '<a href="https://other.example">';</script>"#;
        let once = rewrite(html);
        assert_eq!(rewrite(&once), once);
        assert!(once.contains(r#"rel="NOOPENER noreferrer" target="_blank""#));
        assert!(once.contains(r#"<pre>&lt;a href="https://other.example"&gt;</pre>"#));
        assert!(once.contains(r#"const source = '<a href="https://other.example">';"#));
    }
}
