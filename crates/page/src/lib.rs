use libs::anyhow::Error;
use markdown::RenderContext;
use page::Page;
use parser::ParsedSource;
use std::collections::HashMap;

pub mod library;
pub mod library_cache;
pub mod model;
pub mod page;

// render([ParsedDocument], RenderContext) -> [Page]
pub trait Renderer {
    fn render_all(
        &self,
        documents: &HashMap<String, ParsedSource>,
        contexts: &HashMap<String, RenderContext>,
    ) -> Result<Vec<Page>, Error>;

    fn render_collection(
        collection: &Vec<ParsedSource>,
        context: &RenderContext,
    ) -> Result<Vec<Page>, Error> {
        Ok(vec![])
    }

    // /// Render a page.
    // /// If maybe_data is null, the template as provided by the render context will be rendered.
    // fn render(&self, page: &Page, context: &RenderContext) -> Result<RenderedPage, Error> {
    //     Ok(RenderedPage::new())
    // }
}

pub struct MarkdownRenderer();

// impl Renderer for MarkdownRenderer {
//     fn render(
//         &self,
//         documents: &Vec<ParsedSource>,
//         context: &RenderContext,
//     ) -> Result<Vec<Page>, Error> {
//         Ok(vec![])
//     }
// }

#[cfg(test)]
mod tests {
    use libs::tera::Tera;

    use super::*;

    // #[test]
    // fn should_render_single_markdown_document() {
    //     let tera = Tera::new("*.tera").unwrap();
    //     let render_context = RenderContext::new(&tera);
    //     let parsed_source =
    //         ParsedSourceBuilder::new("file:///a/b/c.md".to_string(), MediaType::Markdown).build();

    //     let renderer = MarkdownRenderer();
    //     let _res = renderer.render(&vec![parsed_source], &render_context);
    // }

    // #[test]
    // fn should_render_markdown_documents() {}

    // #[test]
    // fn should_render_template() {}
}
