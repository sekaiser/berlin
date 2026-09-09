//! Snapshot the declared paper closure. Includes are expanded identically for
//! TeX and Pandoc; resource paths are rewritten to confined staging names.
use super::*;
use berlin_paper::tex::{commands, group, optional, simple_name};
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

#[derive(Clone, Debug)]
pub(in crate::tasks) struct Resource {
    pub path: PathBuf,
    pub uri: String,
    pub relative: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(in crate::tasks) struct Snapshot {
    pub expanded: String,
    pub dependencies: Vec<SourceFile>,
    pub resources: Vec<Resource>,
    pub class_macros: String,
    pub base_class: String,
    pub details: berlin_paper::SourceDetails,
}

struct Loader<'a> {
    directory: &'a Path,
    capability: &'a Path,
    active: HashSet<PathBuf>,
    texts: BTreeMap<PathBuf, SourceFile>,
    resources: BTreeMap<PathBuf, Resource>,
    total: usize,
}

impl Snapshot {
    pub fn load(source: &SourceFile, capability: &Path) -> Result<Self, Error> {
        let directory = source.path.parent().context("paper has no directory")?;
        let mut loader = Loader {
            directory,
            capability,
            active: HashSet::new(),
            texts: BTreeMap::new(),
            resources: BTreeMap::new(),
            total: 0,
        };
        loader.active.insert(source.path.clone());
        let mut expanded = loader.expand(&source.text, 0)?;
        let (base_class, class_macros) = loader.class(&expanded)?;
        // Hyperref supplies authoritative label kinds and destinations for both editions.
        if !expanded
            .split_once("\\begin{document}")
            .context("missing document")?
            .0
            .contains("{hyperref}")
        {
            expanded = expanded.replacen(
                "\\begin{document}",
                "\\usepackage{hyperref}\n\\begin{document}",
                1,
            );
        }
        let details = berlin_paper::inspect_source(&expanded)?;
        Ok(Self {
            expanded,
            dependencies: loader.texts.into_values().collect(),
            resources: loader.resources.into_values().collect(),
            class_macros,
            base_class,
            details,
        })
    }
}

impl Loader<'_> {
    fn path(&self, name: &str, extension: &str) -> Result<PathBuf, Error> {
        if name.is_empty()
            || name.contains(['\\', '\0', '#', '$', '{', '}'])
            || Path::new(name).is_absolute()
        {
            bail!("paper resource needs a literal relative path: {name:?}");
        }
        let mut path = self.directory.join(name);
        if path.extension().is_none() {
            path.set_extension(extension);
        }
        let path = path
            .canonicalize()
            .with_context(|| format!("unable to load paper resource {name:?}"))?;
        if !path.starts_with(self.capability) {
            bail!("paper resource escapes its source root: {}", path.display());
        }
        Ok(path)
    }

    fn text(&mut self, path: &Path) -> Result<SourceFile, Error> {
        if let Some(text) = self.texts.get(path) {
            return Ok(text.clone());
        }
        let source = read_source(path.to_owned())?;
        self.budget(source.text.len())?;
        self.texts.insert(path.to_owned(), source.clone());
        Ok(source)
    }

    fn budget(&mut self, bytes: usize) -> Result<(), Error> {
        self.total += bytes;
        if self.total > 64 * 1024 * 1024 || self.texts.len() + self.resources.len() >= 256 {
            bail!("paper dependency closure exceeds 64 MiB or 256 files");
        }
        Ok(())
    }

    fn resource(
        &mut self,
        path: &Path,
        extension: &str,
        relative: Option<PathBuf>,
    ) -> Result<PathBuf, Error> {
        if let Some(resource) = self.resources.get(path) {
            return Ok(resource.relative.clone());
        }
        if fs::metadata(path)?.len() > 32 * 1024 * 1024 {
            bail!("paper resource exceeds 32 MiB");
        }
        // Text resources must be the same snapshot used by validation and receipts.
        let bytes = match self.texts.get(path) {
            Some(source) => source.text.as_bytes().to_vec(),
            None => fs::read(path)?,
        };
        self.budget(bytes.len())?;
        let relative = relative.unwrap_or_else(|| {
            PathBuf::from(format!("resources/{}.{}", digest(&bytes), extension))
        });
        let uri = url::Url::from_file_path(path)
            .map_err(|_| anyhow::anyhow!("invalid paper resource path"))?
            .to_string();
        self.resources.insert(
            path.to_owned(),
            Resource {
                path: path.to_owned(),
                uri,
                relative: relative.clone(),
                bytes,
            },
        );
        Ok(relative)
    }

    fn expand(&mut self, source: &str, depth: usize) -> Result<String, Error> {
        if depth > 32 {
            bail!("paper includes exceed 32 levels");
        }
        let mut edits = Vec::new();
        for token in commands(source) {
            if !matches!(
                token.name,
                "input" | "include" | "includegraphics" | "bibliography"
            ) {
                continue;
            }
            if token.depth != 0 {
                bail!(
                    "paper resource declarations cannot be hidden in command arguments or macros"
                );
            }
            let (options, rest) = optional(&source[token.end..])?;
            let (name, tail) = group(rest)?;
            let end = source.len() - tail.len();
            let replacement = match token.name {
                "input" | "include" => {
                    if options.is_some() {
                        bail!("include options are unsupported");
                    }
                    let path = self.path(name, "tex")?;
                    if !self.active.insert(path.clone()) {
                        bail!("cyclic paper include: {}", path.display());
                    }
                    let text = self.text(&path)?;
                    let expanded = self.expand(&text.text, depth + 1)?;
                    self.active.remove(&path);
                    if token.name == "include" {
                        format!("\n\\clearpage\n{expanded}\n\\clearpage\n")
                    } else {
                        format!("\n{expanded}\n")
                    }
                }
                "includegraphics" => {
                    if let Some(options) = options
                        && !options.split(',').all(|option| {
                            matches!(
                                option.trim().split('=').next().unwrap_or_default(),
                                "width" | "height" | "scale" | "keepaspectratio"
                            )
                        })
                    {
                        bail!(
                            "graphics options support width, height, scale and keepaspectratio; cropping/pages/rotation are not silently approximated"
                        );
                    }
                    let path = self.path(name, "pdf")?;
                    let extension = path
                        .extension()
                        .and_then(|v| v.to_str())
                        .context("image needs an extension")?
                        .to_ascii_lowercase();
                    if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "pdf") {
                        bail!("paper figures support PNG, JPEG and PDF sources");
                    }
                    let staged = self.resource(&path, &extension, None)?;
                    format!(
                        "\\includegraphics{}{{{}}}",
                        options.map(|o| format!("[{o}]")).unwrap_or_default(),
                        staged.display()
                    )
                }
                "bibliography" => {
                    let mut names = Vec::new();
                    for name in name.split(',').map(str::trim) {
                        let path = self.path(name, "bib")?;
                        self.text(&path)?;
                        let staged = self.resource(&path, "bib", None)?;
                        names.push(staged.with_extension("").to_string_lossy().into_owned());
                    }
                    format!("\\bibliography{{{}}}", names.join(","))
                }
                _ => unreachable!(),
            };
            edits.push((token.start..end, replacement));
        }
        let mut expanded = source.to_owned();
        for (range, value) in edits.into_iter().rev() {
            expanded.replace_range(range, &value);
        }
        if expanded.len() > 1024 * 1024 {
            bail!("expanded paper exceeds 1 MiB");
        }
        Ok(expanded)
    }

    fn class(&mut self, source: &str) -> Result<(String, String), Error> {
        let token = commands(source)
            .into_iter()
            .find(|token| token.name == "documentclass")
            .context("missing document class")?;
        let (_, rest) = optional(&source[token.end..])?;
        let (name, _) = group(rest)?;
        if matches!(name, "article" | "report" | "book") {
            return Ok((name.into(), String::new()));
        }
        if !simple_name(name) {
            bail!("invalid document class name");
        }
        let path = self.path(name, "cls")?;
        let text = self.text(&path)?;
        let (base, macros) = class_wrapper(&text.text)?;
        self.resource(&path, "cls", Some(format!("{name}.cls").into()))?;
        Ok((base, macros))
    }
}

fn class_wrapper(source: &str) -> Result<(String, String), Error> {
    let mut base = None;
    let mut macros = String::new();
    let mut consumed = 0;
    for token in commands(source) {
        if token.start < consumed {
            continue;
        }
        if token.depth != 0 {
            bail!("unsupported class wrapper command nesting");
        }
        if matches!(token.name, "newcommand" | "providecommand" | "renewcommand") {
            let definition = berlin_paper::tex::macro_definition(&source[token.start..])?;
            macros.push_str(definition);
            macros.push('\n');
            consumed = token.start + definition.len();
            continue;
        }
        let (_, rest) = optional(&source[token.end..])?;
        let (value, tail) = group(rest)?;
        consumed = source.len() - tail.len();
        // Version annotations after ProvidesClass / NeedsTeXFormat are metadata.
        let (_, tail) = optional(tail)?;
        consumed = consumed.max(source.len() - tail.len());
        match token.name {
            "NeedsTeXFormat" if value == "LaTeX2e" => {}
            "ProvidesClass" if simple_name(value) => {}
            "LoadClass" if matches!(value, "article" | "report" | "book") && base.is_none() => {
                base = Some(value.to_owned())
            }
            "RequirePackage"
                if value.split(',').all(|v| {
                    matches!(
                        v.trim(),
                        "amsmath" | "amsfonts" | "amssymb" | "graphicx" | "tikz" | "hyperref"
                    )
                }) => {}
            _ => bail!(
                "custom class wrappers support only standard class inheritance, supported packages and literal macros; found \\{}",
                token.name
            ),
        }
    }
    Ok((
        base.context("custom class needs one standard LoadClass")?,
        macros,
    ))
}
