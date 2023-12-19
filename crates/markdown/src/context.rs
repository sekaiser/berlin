use libs::tera::{Context, Tera};
use std::borrow::Cow;

#[derive(Debug)]
pub struct RenderContext<'a> {
    pub tera: Cow<'a, Tera>,
    pub tera_context: Context,
    pub current_page_path: Option<&'a str>,
}

impl<'a> RenderContext<'a> {
    pub fn new(tera: &'a Tera) -> RenderContext<'a> {
        let tera_context = Context::new();

        Self {
            tera: Cow::Borrowed(tera),
            tera_context,
            current_page_path: None,
        }
    }
}
