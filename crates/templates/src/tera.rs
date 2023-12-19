use std::sync::{Arc, Mutex};

use libs::{
    anyhow::Error,
    tera::{Context, Tera},
};

use crate::template::TemplateName;

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
