use std::{fs, process::Command};

fn git_output(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git").args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn watch_git_revision() {
    println!("cargo:rerun-if-changed=build.rs");

    let Some(head_path) = git_output(&["rev-parse", "--git-path", "HEAD"]) else {
        return;
    };
    println!("cargo:rerun-if-changed={head_path}");

    let Ok(head) = fs::read_to_string(head_path) else {
        return;
    };
    let Some(reference) = head.trim().strip_prefix("ref: ") else {
        return;
    };
    if let Some(reference_path) = git_output(&["rev-parse", "--git-path", reference]) {
        println!("cargo:rerun-if-changed={reference_path}");
    }
}

fn git_commit_hash() -> String {
    git_output(&["rev-parse", "HEAD"]).unwrap_or_else(|| "UNKNOWN".to_owned())
}

fn main() {
    watch_git_revision();
    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", git_commit_hash());
}
