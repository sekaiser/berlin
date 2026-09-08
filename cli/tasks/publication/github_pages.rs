//! Dedicated gh-pages branch adapter. Uses installed Git and authenticated gh.
//! Never changes repository settings, source branches, or global Git configuration.
use super::{Commit, Observation, PagesRemote};
use crate::tasks::release::{self, WebsiteRelease};
use anyhow::{Context, Error, bail};
use berlin_core::DeploymentTarget;
use serde::Deserialize;
use std::fs;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

pub(super) struct GitHubPages {
    repository: String,
    workspace: TempDir,
}

impl GitHubPages {
    pub fn new(release: &WebsiteRelease) -> Result<Self, Error> {
        let DeploymentTarget::GitHubPages { repository } = &release.manifest.destination;
        Ok(Self {
            repository: repository.clone(),
            workspace: tempfile::tempdir()?,
        })
    }

    fn url(&self) -> String {
        format!("https://github.com/{}.git", self.repository)
    }

    fn api<T: serde::de::DeserializeOwned>(&self, resource: &str) -> Result<T, Error> {
        let response = run(Command::new("gh").args([
            "api",
            "--hostname",
            "github.com",
            "--method",
            "GET",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "X-GitHub-Api-Version: 2022-11-28",
            &format!("repos/{}{}", self.repository, resource),
        ]))?;
        Ok(serde_json::from_str(&response)?)
    }

    fn git(&self, args: &[&str]) -> Result<String, Error> {
        let mut command = Command::new("git");
        command.current_dir(self.workspace.path());
        // Inherited repository/config overrides must not redirect this scratch repo.
        for (name, _) in
            std::env::vars_os().filter(|(name, _)| name.to_string_lossy().starts_with("GIT_"))
        {
            command.env_remove(name);
        }
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_AUTHOR_NAME", "Berlin")
            .env("GIT_AUTHOR_EMAIL", "berlin@localhost")
            .env("GIT_COMMITTER_NAME", "Berlin")
            .env("GIT_COMMITTER_EMAIL", "berlin@localhost")
            .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.autocrlf=false",
                "-c",
                "core.excludesFile=/dev/null",
                "-c",
                "credential.helper=",
                "-c",
                "credential.helper=!gh auth git-credential",
            ])
            .args(args);
        run(&mut command)
    }

    fn commit_snapshot(
        &self,
        release: &WebsiteRelease,
        parent: Option<String>,
    ) -> Result<Commit, Error> {
        // Never check out existing remote files. The dedicated branch owns this tree.
        let snapshot = tempfile::tempdir()?;
        let copy = snapshot.path().join("site");
        release::copy_site(&release.site(), &copy, &release.manifest.files)?;
        for entry in fs::read_dir(&copy)? {
            let entry = entry?;
            fs::rename(entry.path(), self.workspace.path().join(entry.file_name()))?;
        }
        fs::remove_dir(copy)?;
        self.git(&["add", "--force", "--all", "--", "."])?;
        let tree = self.git(&["write-tree"])?;
        let message = format!("Berlin release {}", release.id);
        let mut args = vec!["commit-tree", tree.trim(), "-m", &message];
        if let Some(parent) = &parent {
            args.extend(["-p", parent]);
        }
        let sha = self.git(&args)?.trim().to_owned();
        Ok(Commit { sha, parent })
    }

    fn head(&self) -> Result<Option<String>, Error> {
        let response = self.git(&["ls-remote", "--refs", &self.url(), "refs/heads/gh-pages"])?;
        parse_head(&response)
    }

    fn push_to(&self, commit: &Commit, remote: &str) -> Result<(), Error> {
        let lease = format!(
            "--force-with-lease=refs/heads/gh-pages:{}",
            commit.parent.as_deref().unwrap_or("")
        );
        let reference = format!("{}:refs/heads/gh-pages", commit.sha);
        self.git(&["push", "--porcelain", &lease, remote, &reference])?;
        Ok(())
    }
}

#[derive(Deserialize)]
struct Pages {
    source: Source,
    html_url: String,
    build_type: Option<String>,
}
#[derive(Deserialize)]
struct Source {
    branch: String,
    path: String,
}
#[derive(Deserialize)]
struct Repository {
    default_branch: String,
}
#[derive(Deserialize)]
struct Build {
    commit: String,
    status: String,
}

impl PagesRemote for GitHubPages {
    fn preflight(&mut self, release: &WebsiteRelease) -> Result<(), Error> {
        let repository: Repository = self.api("")?;
        if repository.default_branch == "gh-pages" {
            bail!("refusing to deploy over the repository's default branch");
        }
        let pages: Pages = self.api("/pages")?;
        if pages.source.branch != "gh-pages"
            || pages.source.path != "/"
            || pages.build_type.as_deref() == Some("workflow")
        {
            bail!(
                "configure Pages to deploy from gh-pages at /; Berlin does not change repository settings"
            );
        }
        if pages.html_url.trim_end_matches('/')
            != release.manifest.url.as_str().trim_end_matches('/')
        {
            bail!("Pages URL does not match the reviewed release URL");
        }
        Ok(())
    }

    fn prepare(&mut self, release: &WebsiteRelease) -> Result<Commit, Error> {
        self.git(&["init", "--quiet", "--object-format=sha1"])?;
        let parent = self.head()?;
        if let Some(parent) = &parent {
            self.git(&[
                "fetch",
                "--quiet",
                "--no-tags",
                "--depth=1",
                &self.url(),
                parent,
            ])?;
        }
        self.commit_snapshot(release, parent)
    }

    fn push(&mut self, commit: &Commit) -> Result<(), Error> {
        self.push_to(commit, &self.url())
    }

    fn observe(&mut self, commit: &Commit) -> Result<Observation, Error> {
        let head = self.head()?;
        if head.as_deref() == Some(commit.sha.as_str()) {
            let build: Build = self.api("/pages/builds/latest")?;
            return Ok(if build.commit == commit.sha {
                match build.status.as_str() {
                    "built" => Observation::Live,
                    "errored" => Observation::Failed,
                    _ => Observation::Uploaded,
                }
            } else {
                Observation::Uploaded
            });
        }
        Ok(if head == commit.parent {
            Observation::NotUploaded
        } else {
            Observation::Conflict
        })
    }
}

fn parse_head(response: &str) -> Result<Option<String>, Error> {
    if response.trim().is_empty() {
        return Ok(None);
    }
    let fields = response.split_whitespace().collect::<Vec<_>>();
    let [sha, "refs/heads/gh-pages"] = fields.as_slice() else {
        bail!("unexpected Pages branch response");
    };
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid Pages commit");
    }
    Ok(Some((*sha).into()))
}

fn run(command: &mut Command) -> Result<String, Error> {
    // Spool output to files so waiting cannot deadlock on a full stdout/stderr pipe.
    let stdout = tempfile::NamedTempFile::new()?;
    let stderr = tempfile::NamedTempFile::new()?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout.reopen()?)
        .stderr(stderr.reopen()?)
        .spawn()
        .context("unable to start Git/gh; install both and authenticate gh")?;
    let timer = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                // Do not persist subprocess diagnostics that might include credentials.
                bail!(
                    "Git/gh command failed ({status}); check authentication, Pages settings, and branch permissions"
                );
            }
            return Ok(fs::read_to_string(stdout.path())?);
        }
        if timer.elapsed() >= Duration::from_secs(45) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Git/gh command timed out; remote outcome may be unknown");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::release::Manifest;

    #[test]
    fn git_snapshot_preserves_exact_bytes_and_reconstructs_the_same_commit() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("site")).unwrap();
        fs::write(root.path().join("site/index.html"), "Hello\r\nworld\n").unwrap();
        fs::write(root.path().join("site/.nojekyll"), "").unwrap();
        // Even a name matching an internal staging convention is ordinary content.
        fs::write(root.path().join("site/snapshot"), "asset").unwrap();
        let release = WebsiteRelease {
            id: "1".repeat(64),
            directory: root.path().to_owned(),
            manifest: Manifest {
                schema_version: 1,
                berlin_version: "test".into(),
                pipeline: "site".into(),
                website_node: "website".into(),
                url: "https://example.com".parse().unwrap(),
                destination: DeploymentTarget::GitHubPages {
                    repository: "owner/site".into(),
                },
                files: release::inventory(&root.path().join("site")).unwrap(),
            },
        };
        let mut commits = Vec::new();
        for _ in 0..2 {
            let remote = GitHubPages::new(&release).unwrap();
            remote
                .git(&["init", "--quiet", "--object-format=sha1"])
                .unwrap();
            let commit = remote.commit_snapshot(&release, None).unwrap();
            assert_eq!(
                remote
                    .git(&["show", &format!("{}:index.html", commit.sha)])
                    .unwrap(),
                "Hello\r\nworld\n"
            );
            assert_eq!(
                remote
                    .git(&["ls-tree", "-r", "--name-only", &commit.sha])
                    .unwrap(),
                ".nojekyll\nindex.html\nsnapshot\n"
            );
            commits.push(commit.sha);
        }
        assert_eq!(commits[0], commits[1]);
    }

    #[test]
    fn remote_ref_parser_rejects_ambiguous_responses() {
        assert_eq!(parse_head("").unwrap(), None);
        let response = format!("{}\trefs/heads/gh-pages\n", "a".repeat(40));
        assert_eq!(parse_head(&response).unwrap(), Some("a".repeat(40)));
        assert!(parse_head(&(response.clone() + response.as_str())).is_err());
        assert!(parse_head(&response.replace("gh-pages", "main")).is_err());
        assert!(parse_head("invalid\trefs/heads/gh-pages").is_err());
    }

    #[test]
    fn git_push_preserves_history_and_rejects_a_stale_parent() {
        let remote = GitHubPages {
            repository: "unused/local-test".into(),
            workspace: tempfile::tempdir().unwrap(),
        };
        let bare = tempfile::tempdir().unwrap();
        let destination = bare.path().to_str().unwrap();
        remote
            .git(&["init", "--quiet", "--object-format=sha1"])
            .unwrap();
        remote
            .git(&[
                "init",
                "--bare",
                "--quiet",
                "--object-format=sha1",
                destination,
            ])
            .unwrap();
        let tree = remote.git(&["write-tree"]).unwrap();
        let initial = Commit {
            sha: remote
                .git(&["commit-tree", tree.trim(), "-m", "initial"])
                .unwrap()
                .trim()
                .into(),
            parent: None,
        };
        remote.push_to(&initial, destination).unwrap();
        let next = |message| Commit {
            sha: remote
                .git(&[
                    "commit-tree",
                    tree.trim(),
                    "-p",
                    &initial.sha,
                    "-m",
                    message,
                ])
                .unwrap()
                .trim()
                .into(),
            parent: Some(initial.sha.clone()),
        };
        let accepted = next("accepted");
        let stale = next("stale");
        remote.push_to(&accepted, destination).unwrap();
        assert!(remote.push_to(&stale, destination).is_err());
        assert_eq!(
            parse_head(
                &remote
                    .git(&["ls-remote", "--refs", destination, "refs/heads/gh-pages"])
                    .unwrap()
            )
            .unwrap(),
            Some(accepted.sha.clone())
        );
        assert_eq!(
            remote
                .git(&["show", "--no-patch", "--format=%P", &accepted.sha])
                .unwrap()
                .trim(),
            initial.sha
        );
    }
}
