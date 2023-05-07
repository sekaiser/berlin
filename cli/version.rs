pub const GIT_COMMIT_HASH: &str = env!("GIT_COMMIT_HASH");

pub fn berlin() -> String {
    let semver = env!("CARGO_PKG_VERSION");
    format!("{}+{}", semver, &GIT_COMMIT_HASH[..7])
}

pub fn get_user_agent() -> String {
    format!("Berlin/{}", berlin())
}
