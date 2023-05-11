use berlin_core::anyhow::Error;
use berlin_core::error::generic_error;
use berlin_core::ParsedSource;
use std::path::PathBuf;
use tera::Tera;

#[derive(Clone)]
struct Content(String);

pub struct Hera {
    inner: HeraInner,
}

struct HeraInner {
    tera: Tera,
}

impl Hera {
    pub fn new(template_path: impl Into<PathBuf>) -> Result<Hera, Error> {
        let template_path = format!("{}/**/*.tera", template_path.into().display().to_string());
        let mut tera = Tera::new(&template_path)?;
        tera.autoescape_on(vec![]);
        if tera.templates.is_empty() {
            Err(generic_error(format!(
                "\nError: No templates found in {:?}\n",
                template_path
            )))
        } else {
            Ok(Hera {
                inner: HeraInner { tera },
            })
        }
    }

    pub fn render_parsed_source_with_context(
        &mut self,
        file_path: &str,
        parsed_source: &ParsedSource,
        context: &tera::Context,
    ) -> String {
        self.inner
            .tera
            .register_function("render", Content(parsed_source.data().to_string()));
        self.inner
            .tera
            .render(&file_path.to_string(), &context)
            .unwrap()
    }

    pub fn render_with_context(&mut self, file_path: &str, context: &tera::Context) -> String {
        self.inner
            .tera
            .render(&file_path.to_string(), &context)
            .unwrap()
    }

    pub fn full_reload(&mut self) -> Result<(), Error> {
        match self.inner.tera.full_reload() {
            Ok(res) => Ok(res),
            Err(e) => Err(generic_error(format!("{e}"))),
        }
    }
}

impl tera::Function for Content {
    fn call(
        &self,
        _args: &std::collections::HashMap<String, tera::Value>,
    ) -> tera::Result<tera::Value> {
        let mut content = self.0.clone();

        Ok(tera::Value::String(content))
    }

    fn is_safe(&self) -> bool {
        true
    }
}

// #[cfg(test)]
// mod tests {

//     use super::*;
//     #[test]
//     fn test_something() {
//         let content = r#"{{< figure src="/pics/athletics_db_excerpt.png" caption="<span class=\"figure-number\">Figure 1: </span>This is the caption for the next figure link (or table)" >}}"#;
//         if let Ok((name, shortcodes)) = parse_for_shortcodes(content) {
//             println!("shortcode_name={name}");
//             println!("shortcodes={shortcodes:?}");
//         }
//     }
// }
