use std::sync::{Arc, Mutex};

use berlin_core::task::template_name::TemplateName;
use libs::{
    anyhow::Error,
    tera::{Context, Tera},
};

#[derive(Debug)]
pub struct TeraRenderer {
    renderer: Arc<Mutex<Tera>>,
}

impl TeraRenderer {
    pub fn render(&self, template_name: &TemplateName, context: &Context) -> Result<String, Error> {
        let renderer = self.renderer.lock().unwrap();
        Ok(renderer.render(template_name.as_ref(), context)?)
    }
}
