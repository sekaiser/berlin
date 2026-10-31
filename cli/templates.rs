use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::{Context as _, Error};
use berlin_document::Document;
use tera::Context;
use tera::Function;
use tera::Tera;
use tera::Value;

pub struct Templates {
    tera: Tera,
    document_renderer: berlin_document_html::Renderer,
}

impl Templates {
    /// Parse the effective template set together, after resolving overrides.
    /// Loading each layer separately would reject cross-layer inheritance.
    pub fn from_sources(
        sources: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, Error> {
        let mut tera = Tera::default();
        tera.autoescape_on(vec![".tera"]);
        tera.register_filter("stable_hash", stable_hash);
        tera.add_raw_templates(sources)?;
        if tera.templates.is_empty() {
            return Err(anyhow!("No templates found"));
        }
        Ok(Self {
            tera,
            document_renderer: berlin_document_html::Renderer::default(),
        })
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tera.templates.contains_key(name)
    }

    pub fn set_document_routes(&mut self, routes: HashMap<berlin_document::ContentId, String>) {
        self.document_renderer =
            berlin_document_html::Renderer::default().with_document_routes(routes);
    }
    pub fn load(template_root: impl Into<PathBuf>) -> Result<Self, Error> {
        let glob = format!("{}/**/*.tera", template_root.into().display());
        let mut tera = Tera::new(&glob)?;
        tera.autoescape_on(vec![".tera"]);
        tera.register_filter("stable_hash", stable_hash);

        if tera.templates.is_empty() {
            return Err(anyhow!("No templates found in {glob:?}"));
        }

        Ok(Self {
            tera,
            document_renderer: berlin_document_html::Renderer::default(),
        })
    }

    pub fn render(
        &mut self,
        template: &str,
        document: &Document,
        context: &Context,
    ) -> Result<String, Error> {
        self.tera.register_function(
            "render",
            RenderedDocument(self.document_renderer.render(document).with_context(|| {
                format!(
                    "Unable to render mathematics in document '{}'",
                    document.id.0
                )
            })?),
        );
        Ok(self.tera.render(template, context)?)
    }

    pub fn render_template(&self, template: &str, context: &Context) -> Result<String, Error> {
        Ok(self.tera.render(template, context)?)
    }
}

/// FNV-1a over UTF-8. Fixed across builds; do not use Rust's randomized hasher
/// for presentation choices that must survive rebuilds and client rendering.
fn stable_hash(value: &Value, _args: &HashMap<String, Value>) -> tera::Result<Value> {
    let text = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("stable_hash expects a string"))?;
    let hash = text.bytes().fold(2_166_136_261_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(16_777_619)
    });
    Ok(Value::from(hash))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn stable_hash_uses_fixed_utf8_vectors() {
        // Shared contract with the notebook search client's palette assignment.
        for (label, expected) in [
            ("", 2_166_136_261_u32),
            ("rust", 490_716_647),
            ("RUST", 456_192_487),
            ("café", 2_821_410_889),
            ("東京", 1_759_422_319),
            ("🦀", 135_463_398),
            ("system-design", 3_389_755_793),
            ("org-roam", 345_417_365),
        ] {
            assert_eq!(
                stable_hash(&Value::from(label), &HashMap::new()).unwrap(),
                Value::from(expected)
            );
        }
        assert!(stable_hash(&Value::from(42), &HashMap::new()).is_err());
    }

    #[test]
    fn notebook_tags_have_stable_colors_without_javascript() {
        let sources = [
            ("macros/tag.tera".into(), include_str!("../themes/notebook/shared/pages/macros/tag.tera").into()),
            ("page.tera".into(), "{% import \"macros/tag.tera\" as tags %}{{ tags::tag(label=label, target=target) }}".into()),
        ];
        let templates = Templates::from_sources(sources).unwrap();
        for label in ["rust", "Rust", "RUST"] {
            let mut context = Context::new();
            context.insert("label", label);
            context.insert("target", "/tags/rust.html");
            context.insert("config_site_url", "https://notebook.example");
            let html = templates.render_template("page.tera", &context).unwrap();
            assert!(html.contains("data-tag-color=\"39\""));
            assert!(html.contains("data-tag=\"rust\""));
        }
    }

    #[test]
    fn paper_tags_keep_labels_and_attributes_escaped() {
        let mut tera = Tera::default();
        tera.autoescape_on(vec![".tera"]);
        tera.add_raw_templates([
            ("macros/tag.tera", include_str!("../support/fixtures/templates/tag.tera")),
            ("page.tera", "{% import \"macros/tag.tera\" as tags %}{{ tags::tag(label=label, target=target) }}"),
        ]).unwrap();
        let mut context = Context::new();
        context.insert("label", "Rust <script> & \"tags\"");
        context.insert("target", "/tags/rust.html");

        let rendered = tera.render("page.tera", &context).unwrap();

        assert!(rendered.contains("class=\"paper-tag\""));
        assert!(rendered.contains("Rust &lt;script&gt; &amp; &quot;tags&quot;"));
        assert!(rendered.contains("data-tag=\"rust &lt;script&gt; &amp; &quot;tags&quot;\""));
        assert!(!rendered.contains("<script>"));
        assert!(rendered.contains("href="));
    }

    #[test]
    fn templates_escape_context_values_by_default() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("page.tera"), "<h1>{{ title }}</h1>").unwrap();
        let templates = Templates::load(directory.path()).unwrap();
        let mut context = Context::new();
        context.insert("title", "<script>alert('xss')</script>");

        let rendered = templates.render_template("page.tera", &context).unwrap();

        assert_eq!(
            rendered,
            "<h1>&lt;script&gt;alert(&#x27;xss&#x27;)&lt;&#x2F;script&gt;</h1>"
        );
    }
}

#[derive(Clone)]
struct RenderedDocument(berlin_document_html::Html);

impl Function for RenderedDocument {
    fn call(&self, _args: &HashMap<String, Value>) -> tera::Result<Value> {
        Ok(Value::String(self.0.as_str().to_owned()))
    }

    fn is_safe(&self) -> bool {
        true
    }
}
