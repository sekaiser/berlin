use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Error;
use anyhow::anyhow;
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
    pub fn load(template_root: impl Into<PathBuf>) -> Result<Self, Error> {
        let glob = format!("{}/**/*.tera", template_root.into().display());
        let mut tera = Tera::new(&glob)?;
        tera.autoescape_on(vec![".tera"]);

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
            RenderedDocument(self.document_renderer.render(document)),
        );
        Ok(self.tera.render(template, context)?)
    }

    pub fn render_template(&self, template: &str, context: &Context) -> Result<String, Error> {
        Ok(self.tera.render(template, context)?)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

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
