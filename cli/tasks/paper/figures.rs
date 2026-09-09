//! Build public figure assets without publishing source paths or a UI screenshot.
use super::*;
use berlin_paper::tex::{commands, group};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Asset {
    pub name: String,
    pub bytes: Vec<u8>,
}
impl Asset {
    fn new(bytes: Vec<u8>, extension: &str) -> Self {
        Self {
            name: format!("{}.{}", digest(&bytes), extension),
            bytes,
        }
    }
    pub fn valid(&self) -> bool {
        self.name.rsplit_once('.').is_some_and(|(hash, ext)| {
            matches!(ext, "png" | "jpg" | "jpeg" | "svg") && hash == digest(&self.bytes)
        })
    }
}

pub(super) struct Prepared {
    pub html_source: String,
    pub assets: Vec<Asset>,
    pub urls: BTreeMap<String, String>,
}

pub(super) fn prepare(source: &Source, work: &Path, url_prefix: &str) -> Result<Prepared, Error> {
    let mut assets = BTreeMap::new();
    let mut urls = BTreeMap::new();
    for resource in &source.snapshot.resources {
        let Some(extension) = resource.relative.extension().and_then(|v| v.to_str()) else {
            continue;
        };
        if !matches!(extension, "png" | "jpg" | "jpeg" | "pdf") {
            continue;
        }
        let asset = if extension == "pdf" {
            convert_pdf(
                work,
                resource.relative.to_str().context("non-UTF-8 image path")?,
                "converted-figure.svg",
            )?
        } else {
            Asset::new(resource.bytes.clone(), extension)
        };
        urls.insert(
            resource.relative.to_string_lossy().into_owned(),
            format!("{url_prefix}/assets/{}", asset.name),
        );
        assets.insert(asset.name.clone(), asset);
    }
    let mut html_source = source.snapshot.expanded.clone();
    let mut ranges = Vec::new();
    let mut start = None;
    for token in commands(&html_source) {
        if !matches!(token.name, "begin" | "end") {
            continue;
        }
        let (name, tail) = group(&html_source[token.end..])?;
        if name != "tikzpicture" {
            continue;
        }
        if token.name == "begin" {
            if start.replace(token.start).is_some() {
                bail!("nested TikZ pictures are unsupported");
            }
        } else {
            ranges
                .push(start.take().context("unmatched TikZ end")?..html_source.len() - tail.len());
        }
    }
    if start.is_some() {
        bail!("unclosed TikZ picture");
    }
    if ranges.len() > 32 {
        bail!("paper exceeds 32 TikZ pictures");
    }
    let mut replacements = Vec::new();
    for (index, range) in ranges.into_iter().enumerate() {
        let stem = format!("berlin-tikz-{index}");
        let latex = format!(
            "\\documentclass[tikz,border=4pt]{{standalone}}\n\\usepackage{{amsmath,amssymb}}\n{}\n{}\n{}\n\\begin{{document}}\n{}\n\\end{{document}}\n",
            if source.snapshot.details.tikz_libraries.is_empty() {
                String::new()
            } else {
                format!(
                    "\\usetikzlibrary{{{}}}",
                    source.snapshot.details.tikz_libraries.join(",")
                )
            },
            source.snapshot.class_macros,
            source.snapshot.details.macros,
            &html_source[range.clone()]
        );
        let asset = Asset::new(
            crate::tasks::graphics::render_in(
                work,
                &stem,
                &latex,
                crate::tasks::graphics::Engine::PdfLatex,
            )?,
            "svg",
        );
        let path = format!("{stem}.svg");
        urls.insert(path.clone(), format!("{url_prefix}/assets/{}", asset.name));
        assets.insert(asset.name.clone(), asset);
        replacements.push((range, format!("\\includegraphics{{{path}}}")));
    }
    for (range, replacement) in replacements.into_iter().rev() {
        html_source.replace_range(range, &replacement);
    }
    // Pandoc does not load local .cls definitions. Supply their validated inline
    // macros explicitly and the base class, keeping the real class in the PDF.
    let token = commands(&html_source)
        .into_iter()
        .find(|c| c.name == "documentclass")
        .context("missing class")?;
    let (_, rest) = berlin_paper::tex::optional(&html_source[token.end..])?;
    let (_, tail) = group(rest)?;
    let end = html_source.len() - tail.len();
    html_source.replace_range(
        token.start..end,
        &format!("\\documentclass{{{}}}", source.snapshot.base_class),
    );
    html_source = html_source.replacen(
        "\\begin{document}",
        &format!("{}\n\\begin{{document}}", source.snapshot.class_macros),
        1,
    );
    let assets = assets.into_values().collect::<Vec<_>>();
    if assets.iter().map(|asset| asset.bytes.len()).sum::<usize>() > 64 * 1024 * 1024 {
        bail!("generated figures exceed 64 MiB");
    }
    Ok(Prepared {
        html_source,
        assets,
        urls,
    })
}

fn convert_pdf(work: &Path, input: &str, output: &str) -> Result<Asset, Error> {
    Ok(Asset::new(
        crate::tasks::graphics::pdf_to_svg(work, input, output)?,
        "svg",
    ))
}

pub(super) fn rewrite_images(
    ast: &mut serde_json::Value,
    urls: &BTreeMap<String, String>,
) -> Result<(), Error> {
    match ast {
        serde_json::Value::Object(object) => {
            if object.get("t").and_then(|v| v.as_str()) == Some("Image") {
                let source = object
                    .get_mut("c")
                    .and_then(|v| v.get_mut(2))
                    .and_then(|v| v.get_mut(0))
                    .context("invalid Pandoc image")?;
                let url = urls
                    .get(source.as_str().context("invalid image source")?)
                    .context("image was not a declared paper resource")?;
                *source = url.clone().into();
            }
            for value in object.values_mut() {
                rewrite_images(value, urls)?;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                rewrite_images(value, urls)?;
            }
        }
        _ => {}
    }
    Ok(())
}
