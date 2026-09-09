//! Conservative cache eligibility. Nix store paths identify immutable binaries
//! and packages. Mutable/custom TeX installations still compile, without reuse.
use crate::tasks::cache::{file_hash, hash};
use anyhow::{Context, Error};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(super) fn fingerprint(
    bibliography: Option<&str>,
    figures: bool,
) -> Result<Option<String>, Error> {
    fingerprint_for("pdflatex", bibliography, figures, true)
}

pub(in crate::tasks) fn graphics_fingerprint(engine: &str) -> Result<Option<String>, Error> {
    fingerprint_for(engine, None, true, false)
}

fn fingerprint_for(
    engine: &str,
    bibliography: Option<&str>,
    figures: bool,
    paper: bool,
) -> Result<Option<String>, Error> {
    // Search overrides can introduce dependencies not represented by TEXMF.
    if env::vars_os().any(|(name, _)| {
        let name = name.to_string_lossy();
        ["TEX", "BIB", "BST", "MKTEX", "PANDOC"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
    }) {
        return Ok(None);
    }
    let mut tools = vec![engine, "kpsewhich"];
    if paper {
        tools.push("pandoc");
    }
    if bibliography.is_some() {
        tools.push("bibtex");
    }
    if figures {
        tools.push("dvisvgm");
        tools.push("pdfinfo");
    }
    let mut identities = Vec::new();
    for name in tools {
        let path = executable(name).with_context(|| format!("unable to locate {name}"))?;
        if !path.starts_with("/nix/store") || !path.canonicalize()?.starts_with("/nix/store") {
            return Ok(None);
        }
        identities.push((
            name.to_owned(),
            path.clone(),
            probe(&path, &[if name == "pdfinfo" { "-v" } else { "--version" }])?,
        ));
    }
    let kpsewhich = executable("kpsewhich").context("missing kpsewhich")?;
    let roots = probe(&kpsewhich, &["--var-value=TEXMF"])?;
    if !immutable_search_roots(&roots) {
        return Ok(None);
    }
    let mut resources = Vec::new();
    let mut names = vec![format!("{engine}.fmt")];
    if let Some(style) = bibliography {
        names.push(format!("{style}.bst"));
    }
    if figures {
        names.extend(["standalone.cls".into(), "tikz.sty".into()]);
    }
    for name in names {
        let resolved = probe(
            &kpsewhich,
            &[
                &format!("--progname={engine}"),
                if engine == "lualatex" {
                    "--engine=luahbtex"
                } else {
                    "--engine=pdftex"
                },
                &name,
            ],
        )?;
        let path = Path::new(resolved.trim()).canonicalize()?;
        if !path.starts_with("/nix/store") {
            return Ok(None);
        }
        resources.push(path);
    }
    // Includes uncommitted converter changes in development builds, not merely
    // the package version. Computed once per compilation operation, not per paper.
    let berlin = file_hash(&env::current_exe()?)?;
    let mut environment = env::vars_os()
        .filter(|(name, _)| {
            let name = name.to_string_lossy();
            name.starts_with("LC_")
                || name.starts_with("LD_")
                || name.starts_with("DYLD_")
                || matches!(
                    name.as_ref(),
                    "LANG" | "TZ" | "HOME" | "OSFONTDIR" | "FONTCONFIG_FILE" | "FONTCONFIG_PATH"
                )
        })
        .collect::<Vec<_>>();
    environment.sort();
    Ok(Some(hash(&serde_json::to_vec(&(
        identities,
        roots,
        resources,
        berlin,
        environment,
    ))?)))
}

fn immutable_search_roots(roots: &str) -> bool {
    let mut found = false;
    for root in roots
        .split(['{', '}', ','])
        .map(|part| part.trim().trim_start_matches('!'))
    {
        if root.is_empty() {
            continue;
        }
        let path = Path::new(root);
        if !path.is_absolute() {
            return false;
        }
        if path.starts_with("/nix/store") {
            found = true;
        } else if !empty_search_root(path, 0) {
            return false;
        }
    }
    found
}

// TeX may create empty user-cache directories. Those cannot shadow a package;
// recheck on every invocation so adding even one file disables immutable reuse.
fn empty_search_root(path: &Path, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Ok(metadata) if metadata.is_dir() => fs::read_dir(path).is_ok_and(|entries| {
            entries.take(1025).enumerate().all(|(index, entry)| {
                index < 1024 && entry.is_ok_and(|entry| empty_search_root(&entry.path(), depth + 1))
            })
        }),
        _ => false,
    }
}

fn executable(name: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH")?)
        .map(|root| root.join(name))
        .find(|path| path.is_file())
        .and_then(|path| std::path::absolute(path).ok())
}

fn probe(program: &Path, args: &[&str]) -> Result<String, Error> {
    crate::util::process::run(
        Command::new(program).args(args),
        crate::util::process::Limits {
            timeout: std::time::Duration::from_secs(10),
            output_bytes: 64 * 1024,
        },
    )
}

/// Refuse to cache if the compiler consumed anything outside its source snapshot
/// and immutable installation. Never let a cache hit skip mutable package reads.
pub(in crate::tasks) fn immutable_inputs(work: &Path) -> Result<bool, Error> {
    let work = work.canonicalize()?;
    let mut found = false;
    for entry in fs::read_dir(&work)? {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "fls") {
            continue;
        }
        let recorder = fs::read_to_string(path)?;
        for input in recorder
            .lines()
            .filter_map(|line| line.strip_prefix("INPUT "))
        {
            found = true;
            let path = work.join(input).canonicalize()?;
            if !path.starts_with(&work)
                && !path.starts_with("/nix/store")
                && path != Path::new("/dev/null")
            {
                return Ok(false);
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mutable_search_roots_disable_reuse() {
        let root = tempfile::tempdir().unwrap();
        assert!(immutable_search_roots("{!!/nix/store/example-texmf}"));
        assert!(immutable_search_roots(&format!(
            "{{/nix/store/example,{}}}",
            root.path().display()
        )));
        fs::write(root.path().join("custom.sty"), "override").unwrap();
        assert!(!immutable_search_roots(&format!(
            "{{/nix/store/example,{}}}",
            root.path().display()
        )));
        assert!(!immutable_search_roots("{relative,/nix/store/example}"));
        assert!(!immutable_search_roots(""));
    }
}
