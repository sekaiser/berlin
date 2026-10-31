//! Track the bundler's actual reads and resolutions. The dependency hint only
//! locates a candidate: every path, resolution and byte hash is rechecked.
use super::Mode;
use crate::tasks::cache::{Artifact, ArtifactCache, CacheKey, hash, implementation_fingerprint};
use anyhow::{Error, anyhow};
use lightningcss::bundler::{ResolveResult, SourceProvider};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Dependencies {
    files: BTreeMap<PathBuf, (PathBuf, String)>,
    imports: BTreeSet<(PathBuf, String, Destination)>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum Destination {
    File(PathBuf),
    External(String),
}

pub(super) fn compile<P: SourceProvider<Error = std::io::Error>>(
    source: &Path,
    root: &Path,
    provider: &P,
    mode: Mode,
    cache: Option<&ArtifactCache>,
) -> Result<String, Error> {
    compile_inner(source, root, provider, mode, cache, false, || {})
}

pub(super) fn compile_snapshot<P: SourceProvider<Error = std::io::Error>>(
    source: &Path,
    provider: &P,
    cache: Option<&ArtifactCache>,
    before_compile: impl FnOnce(),
) -> Result<String, Error> {
    compile_inner(
        source,
        Path::new("styles"),
        provider,
        Mode::Stylesheet,
        cache,
        true,
        before_compile,
    )
}

fn compile_inner<P: SourceProvider<Error = std::io::Error>>(
    source: &Path,
    root: &Path,
    provider: &P,
    mode: Mode,
    cache: Option<&ArtifactCache>,
    logical_files: bool,
    before_compile: impl FnOnce(),
) -> Result<String, Error> {
    let Some(cache) = cache else {
        log::info!("Compiling CSS {}", source.display());
        before_compile();
        return super::compile_with(source, provider, mode.options()?);
    };
    let canonical_root = if logical_files {
        root.to_owned()
    } else {
        root.canonicalize()?
    };
    // Normalize aliases such as /tmp -> /private/tmp, while retaining a source
    // file's own alias (relative imports are resolved beside that alias).
    let source = source
        .strip_prefix(root)
        .map(|relative| canonical_root.join(relative))
        .unwrap_or_else(|_| source.to_owned());
    let root = canonical_root;
    let identity = logical(&root, &source)
        .and_then(|entry| Ok((entry, mode, implementation_fingerprint()?, logical_files)));
    let Ok(identity) = identity else {
        log::info!("Compiling CSS {}", source.display());
        before_compile();
        return super::compile_with(&source, provider, mode.options()?);
    };
    let hint_key = CacheKey::new("css-dependencies/v2", &identity)?;
    if let Some(hint) = cache.load::<Dependencies>(&hint_key)
        && hint.value.files.contains_key(&identity.0)
        && hint.value.verify(&root, provider, logical_files)
    {
        let key = CacheKey::new("css-output/v2", &(&identity, &hint.value))?;
        if let Some(output) = cache.load::<()>(&key)
            && let Ok(css) = String::from_utf8(output.bytes)
        {
            log::info!("Reusing CSS {}", identity.0.display());
            return Ok(css);
        }
    }
    log::info!("Compiling CSS {}", identity.0.display());
    // Lightning CSS's SourceProvider is Send + Sync and may read imports in
    // parallel. The lock is only for recording, not shared pipeline ownership.
    before_compile();
    let tracked = Tracking {
        logical_files,
        provider,
        root: &root,
        dependencies: Mutex::new(Some(Dependencies::default())),
    };
    let css = super::compile_with(&source, &tracked, mode.options()?)?;
    if let Some(dependencies) = tracked
        .dependencies
        .into_inner()
        .map_err(|_| anyhow!("CSS dependency lock poisoned"))?
    {
        let key = CacheKey::new("css-output/v2", &(&identity, &dependencies))?;
        cache.store(
            &key,
            &Artifact {
                value: (),
                bytes: css.as_bytes().to_vec(),
            },
        );
        cache.store(
            &hint_key,
            &Artifact {
                value: dependencies,
                bytes: Vec::new(),
            },
        );
    }
    Ok(css)
}

impl Dependencies {
    fn verify<P: SourceProvider<Error = std::io::Error>>(
        &self,
        root: &Path,
        provider: &P,
        logical_files: bool,
    ) -> bool {
        self.files.iter().all(|(path, expected)| {
            safe_relative(path)
                && file_record(root, &root.join(path), provider, logical_files)
                    .is_ok_and(|actual| &actual == expected)
        }) && self.imports.iter().all(|(origin, specifier, expected)| {
            safe_relative(origin)
                && self.files.contains_key(origin)
                && provider
                    .resolve(specifier, &root.join(origin))
                    .ok()
                    .and_then(|result| destination(root, &result, logical_files).ok())
                    .as_ref()
                    == Some(expected)
        })
    }
}

struct Tracking<'a, P> {
    logical_files: bool,
    provider: &'a P,
    root: &'a Path,
    dependencies: Mutex<Option<Dependencies>>,
}

impl<P: SourceProvider<Error = std::io::Error>> SourceProvider for Tracking<'_, P> {
    type Error = std::io::Error;
    fn read<'a>(&'a self, path: &Path) -> Result<&'a str, Self::Error> {
        let text = self.provider.read(path)?;
        let record = logical(self.root, path).and_then(|name| {
            let target = target(self.root, path, self.logical_files)?;
            Ok((name, (target, hash(text.as_bytes()))))
        });
        let mut dependencies = self
            .dependencies
            .lock()
            .map_err(|_| std::io::Error::other("CSS dependency lock poisoned"))?;
        match (dependencies.as_mut(), record) {
            (Some(deps), Ok((name, value))) => {
                if let Some(previous) = deps.files.insert(name, value.clone())
                    && previous != value
                {
                    *dependencies = None;
                }
            }
            (_, Err(_)) => *dependencies = None,
            _ => {}
        }
        Ok(text)
    }

    fn resolve(&self, specifier: &str, origin: &Path) -> Result<ResolveResult, Self::Error> {
        let result = self.provider.resolve(specifier, origin)?;
        let record = logical(self.root, origin).and_then(|origin| {
            let target = destination(self.root, &result, self.logical_files)?;
            Ok((origin, specifier.to_owned(), target))
        });
        let mut dependencies = self
            .dependencies
            .lock()
            .map_err(|_| std::io::Error::other("CSS dependency lock poisoned"))?;
        match (dependencies.as_mut(), record) {
            (Some(deps), Ok(edge)) => {
                deps.imports.insert(edge);
            }
            (_, Err(_)) => *dependencies = None,
            _ => {}
        }
        Ok(result)
    }
}

fn file_record<P: SourceProvider<Error = std::io::Error>>(
    root: &Path,
    path: &Path,
    provider: &P,
    logical_files: bool,
) -> Result<(PathBuf, String), Error> {
    let target = target(root, path, logical_files)?;
    Ok((target, hash(provider.read(path)?.as_bytes())))
}

fn destination(
    root: &Path,
    result: &ResolveResult,
    logical_files: bool,
) -> Result<Destination, Error> {
    Ok(match result {
        ResolveResult::File(path) => Destination::File(target(root, path, logical_files)?),
        ResolveResult::External(url) => Destination::External(url.clone()),
    })
}

fn target(root: &Path, path: &Path, logical_files: bool) -> Result<PathBuf, Error> {
    if logical_files {
        logical(root, path)
    } else {
        confined(root, path)
    }
}

fn confined(root: &Path, path: &Path) -> Result<PathBuf, Error> {
    Ok(path.canonicalize()?.strip_prefix(root)?.to_owned())
}

fn logical(root: &Path, path: &Path) -> Result<PathBuf, Error> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                anyhow::ensure!(normalized.pop(), "CSS path escapes root");
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    let relative = normalized.strip_prefix(root)?.to_owned();
    anyhow::ensure!(safe_relative(&relative), "invalid CSS dependency path");
    Ok(relative)
}

fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}
