//! OAuth credentials are private files, separate from publication evidence.
use super::{
    http::{self, Curl, Request, Transport},
    *,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Connection {
    schema_version: u8,
    account: String,
    target: Target,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Credentials {
    access_token: String,
    expires_at: Option<u64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    client_id: String,
    redirect_uri: Url,
    state: String,
    created_at: u64,
    api_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    organization: Option<String>,
}
fn directory(project: &Project, account: &str) -> Result<PathBuf, Error> {
    super::super::validate_account(account)?;
    let mut path = project.root().join(".berlin");
    for part in ["", "connections", "linkedin", account] {
        if !part.is_empty() {
            path.push(part);
        }
        if fs::symlink_metadata(&path).is_ok() {
            release::ensure_directory(&path)?;
        }
    }
    Ok(path)
}
fn create_directory(project: &Project, account: &str) -> Result<PathBuf, Error> {
    let path = directory(project, account)?;
    let parent = release::state_directory(project.root(), "connections")?;
    for p in [parent.join("linkedin"), path.clone()] {
        match fs::create_dir(&p) {
            Ok(()) => release::sync_directory(p.parent().unwrap())?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        release::ensure_directory(&p)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(path)
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, Error> {
    Ok(serde_json::from_str(&super::super::review::read_text(
        path,
        64 * 1024,
    )?)?)
}
pub(crate) fn target(project: &Project, account: &str) -> Result<Option<Target>, Error> {
    let path = directory(project, account)?.join("connection.json");
    if !path.try_exists()? {
        return Ok(None);
    }
    let c: Connection = read(&path)?;
    if c.schema_version != 1 || c.account != account {
        bail!("invalid LinkedIn connection");
    }
    c.target.validate()?;
    Ok(Some(c.target))
}
pub(super) fn credential(
    project: &Project,
    account: &str,
    env: Option<&str>,
) -> Result<String, Error> {
    if let Some(name) = env {
        return secret_env(name);
    }
    let file = directory(project, account)?.join("credentials.json");
    release::ensure_file(&file)
        .context("LinkedIn is not authorized; run 'announcement linkedin auth-start'")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&file)?.permissions().mode() & 0o077 != 0 {
            bail!("LinkedIn credentials require mode 600");
        }
    }
    let c: Credentials = read(&file)?;
    if c.expires_at
        .is_some_and(|time| super::super::now().is_ok_and(|now| now >= time))
    {
        bail!("LinkedIn authorization expired; authorize again");
    }
    validate_token(&c.access_token)?;
    Ok(c.access_token)
}
fn validate_token(token: &str) -> Result<(), Error> {
    if token.is_empty() || token.len() > 16384 || token.chars().any(char::is_control) {
        bail!("invalid LinkedIn credential");
    }
    Ok(())
}
fn secret_env(name: &str) -> Result<String, Error> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
    {
        bail!("invalid credential environment variable name");
    }
    let value = std::env::var(name)
        .with_context(|| format!("set {name} locally; do not share the secret in chat"))?;
    validate_token(&value)?;
    Ok(value)
}
pub(super) fn whoami(
    transport: &mut impl Transport,
    token: &str,
    api_version: &str,
) -> Result<Target, Error> {
    let response = transport.send(Request {
        method: "GET",
        url: http::endpoint("/v2/userinfo")?,
        bearer: Some(token),
        content_type: "application/json",
        version: None,
        body: None,
    })?;
    if response.status != 200 {
        bail!("LinkedIn could not identify this token; enable openid/profile and authorize again");
    }
    let subject = response.body["sub"]
        .as_str()
        .context("LinkedIn userinfo has no subject")?;
    let t = Target {
        member_urn: format!("urn:li:person:{subject}"),
        name: response.body["name"]
            .as_str()
            .context("LinkedIn userinfo has no name")?
            .into(),
        api_version: api_version.into(),
        visibility: "PUBLIC".into(),
        organization: None,
    };
    t.validate()?;
    Ok(t)
}
pub(super) fn verify_target(
    transport: &mut impl Transport,
    token: &str,
    target: &Target,
    permission: super::organization::Permission,
) -> Result<(), Error> {
    let member = whoami(transport, token, &target.api_version)?;
    if member.member_urn != target.member_urn {
        bail!("authenticated LinkedIn member differs from the approval");
    }
    if let Some(organization) = &target.organization {
        super::organization::authorize(transport, token, target, &organization.urn, permission)?;
    }
    Ok(())
}
fn save(
    project: &Project,
    account: &str,
    target: Target,
    credentials: Credentials,
) -> Result<Value, Error> {
    target.validate()?;
    if self::target(project, account)?.is_some_and(|old| !old.same_binding(&target)) {
        bail!(
            "this account label belongs to another LinkedIn member or author; use a different label"
        );
    }
    let directory = create_directory(project, account)?;
    release::atomic_json(&directory.join("credentials.json"), &credentials)?;
    release::atomic_json(
        &directory.join("connection.json"),
        &Connection {
            schema_version: 1,
            account: account.into(),
            target: target.clone(),
        },
    )?;
    Ok(json!({"account":account,"member":target,"authorized":true}))
}
pub(crate) fn connect(
    project: &Project,
    account: &str,
    token_env: &str,
    confirm: Option<&str>,
    api_version: &str,
    organization: Option<&str>,
) -> Result<Value, Error> {
    if let Some(organization) = organization {
        super::organization::id(organization)?;
    }
    let token = secret_env(token_env)?;
    let member = whoami(&mut Curl, &token, api_version)?;
    let member = super::organization::bind(&mut Curl, &token, member, organization)?;
    if confirm != Some(member.member_urn.as_str()) {
        if confirm.is_some() {
            bail!("confirmed member differs from the authenticated member");
        }
        return Ok(
            json!({"account":account,"member":member,"authorized":false,"next":"Verify the member and selected author, then repeat with the same --organization (if any) and --confirm-member MEMBER_URN"}),
        );
    }
    let _lock = super::super::store::lock(project)?;
    save(
        project,
        account,
        member,
        Credentials {
            access_token: token,
            expires_at: None,
        },
    )
}
pub(crate) fn auth_start(
    project: &Project,
    account: &str,
    client_id: &str,
    redirect: &Url,
    api_version: &str,
    organization: Option<&str>,
) -> Result<Value, Error> {
    super::version(api_version)?;
    if let Some(organization) = organization {
        super::organization::id(organization)?;
    }
    if client_id.is_empty()
        || client_id.len() > 256
        || !client_id.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        bail!("invalid LinkedIn client ID");
    }
    if redirect.scheme() != "https"
        || !redirect.username().is_empty()
        || redirect.password().is_some()
        || redirect.query().is_some()
        || redirect.fragment().is_some()
        || redirect.host_str().is_none()
    {
        bail!("use the registered absolute HTTPS callback URL");
    }
    let _lock = super::super::store::lock(project)?;
    let directory = create_directory(project, account)?;
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("secure OAuth state generation failed"))?;
    let state = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let pending = Pending {
        client_id: client_id.into(),
        redirect_uri: redirect.clone(),
        state: state.clone(),
        created_at: super::super::now()?,
        api_version: api_version.into(),
        organization: organization.map(str::to_owned),
    };
    release::atomic_json(&directory.join("pending.json"), &pending)?;
    let mut url = Url::parse("https://www.linkedin.com/oauth/v2/authorization")?;
    url.query_pairs_mut().extend_pairs([
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", redirect.as_str()),
        ("state", &state),
        ("scope", scopes(organization)),
    ]);
    Ok(
        json!({"account":account,"organization":organization,"authorization_url":url,"redirect_uri":redirect,"expires_in_seconds":900,
        "next":"Open the authorization URL yourself, then save the callback response locally and run auth-finish."}),
    )
}
fn scopes(organization: Option<&str>) -> &'static str {
    if organization.is_some() {
        "openid profile w_organization_social r_organization_social rw_organization_admin"
    } else {
        "openid profile w_member_social"
    }
}
fn callback(pending: &Pending, url: &Url, now: u64) -> Result<String, Error> {
    if now < pending.created_at || now - pending.created_at > 900 {
        bail!("OAuth request expired; start again");
    }
    if url.origin() != pending.redirect_uri.origin()
        || url.path() != pending.redirect_uri.path()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("OAuth callback does not match the registered redirect");
    }
    let mut fields = std::collections::BTreeMap::new();
    for (k, v) in url.query_pairs() {
        if fields.insert(k.into_owned(), v.into_owned()).is_some() {
            bail!("duplicate OAuth callback parameter");
        }
    }
    if fields.get("state") != Some(&pending.state) {
        bail!("OAuth state mismatch; start again");
    }
    if fields.contains_key("error") {
        bail!("LinkedIn authorization was declined; start again");
    }
    let code = fields
        .remove("code")
        .context("OAuth callback has no code")?;
    if code.is_empty() || code.len() > 16384 || code.chars().any(char::is_control) {
        bail!("invalid OAuth authorization code");
    }
    Ok(code)
}
pub(crate) fn auth_finish(
    project: &Project,
    account: &str,
    callback_file: &Path,
    secret_env_name: &str,
) -> Result<Value, Error> {
    let _lock = super::super::store::lock(project)?;
    let directory = directory(project, account)?;
    let pending: Pending = read(&directory.join("pending.json"))?;
    let url = Url::parse(
        super::super::review::read_text(&project.root().join(callback_file), 64 * 1024)?.trim(),
    )?;
    let code = callback(&pending, &url, super::super::now()?)?;
    let secret = secret_env(secret_env_name)?;
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("client_id", pending.client_id.as_str()),
            ("client_secret", secret.as_str()),
            ("redirect_uri", pending.redirect_uri.as_str()),
        ])
        .finish();
    // A failed or interrupted exchange requires a fresh login, never a replay of
    // a possibly consumed authorization code.
    fs::remove_file(directory.join("pending.json"))?;
    release::sync_directory(&directory)?;
    let response = Curl.send(Request {
        method: "POST",
        url: Url::parse("https://www.linkedin.com/oauth/v2/accessToken")?,
        bearer: None,
        content_type: "application/x-www-form-urlencoded",
        version: None,
        body: Some(body.as_bytes()),
    })?;
    if response.status != 200 {
        bail!("LinkedIn token exchange failed; verify app settings and start authorization again");
    }
    let token = response.body["access_token"]
        .as_str()
        .context("LinkedIn returned no access token")?
        .to_owned();
    validate_token(&token)?;
    let ttl = response.body["expires_in"]
        .as_u64()
        .filter(|s| *s > 0 && *s <= 366 * 86400)
        .context("invalid LinkedIn token expiry")?;
    let member = whoami(&mut Curl, &token, &pending.api_version)?;
    let member =
        super::organization::bind(&mut Curl, &token, member, pending.organization.as_deref())?;
    save(
        project,
        account,
        member,
        Credentials {
            access_token: token,
            expires_at: Some(super::super::now()? + ttl),
        },
    )
}
pub(crate) fn callback_page(project: &Project, url: &Url, output: &Path) -> Result<Value, Error> {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    use std::io::Write;
    if url.scheme() != "https"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("callback page requires its canonical HTTPS URL");
    }
    if output.as_os_str().is_empty()
        || output
            .components()
            .any(|p| !matches!(p, std::path::Component::Normal(_)))
        || output
            .components()
            .any(|p| matches!(p.as_os_str().to_str(), Some(".git" | ".berlin")))
    {
        bail!("callback output must be a confined source asset path");
    }
    let template = include_str!("callback.html");
    let digest = |tag: &str| {
        let text = template
            .split_once(&format!("<{tag}>"))
            .unwrap()
            .1
            .split_once(&format!("</{tag}>"))
            .unwrap()
            .0;
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(text.as_bytes()))
    };
    let html = template
        .replace("@@SCRIPT_SHA@@", &digest("script"))
        .replace("@@STYLE_SHA@@", &digest("style"))
        .replace(
            "@@CANONICAL@@",
            &html_escape::encode_double_quoted_attribute(url.as_str()),
        );
    let mut parent = project.root().to_owned();
    for component in output
        .parent()
        .context("callback output needs a parent")?
        .components()
    {
        parent.push(component);
        match fs::create_dir(&parent) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        release::ensure_directory(&parent)?;
    }
    let file = project.root().join(output);
    let mut temp = tempfile::NamedTempFile::new_in(&parent)?;
    temp.write_all(html.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(&file)
        .map_err(|e| e.error)
        .context("callback page already exists; inspect it instead of overwriting it")?;
    release::sync_directory(&parent)?;
    Ok(json!({"path":file,"canonical_url":url,"remote_io":false}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_requires_same_origin_path_state_unique_fields_and_live_request() {
        let p = Pending {
            client_id: "client".into(),
            redirect_uri: Url::parse("https://example.test/callback").unwrap(),
            state: "random".into(),
            created_at: 100,
            api_version: API_VERSION.into(),
            organization: None,
        };
        let valid = Url::parse("https://example.test/callback?code=secret&state=random").unwrap();
        assert_eq!(callback(&p, &valid, 200).unwrap(), "secret");
        for u in [
            "https://other.test/callback?code=secret&state=random",
            "https://example.test/other?code=secret&state=random",
            "https://example.test/callback?code=secret&state=wrong",
            "https://example.test/callback?code=secret&code=other&state=random",
            "https://example.test/callback?error=declined&state=random",
        ] {
            assert!(callback(&p, &Url::parse(u).unwrap(), 200).is_err());
        }
        assert!(callback(&p, &valid, 1001).is_err());
    }
    #[test]
    fn saved_account_cannot_switch_page_member_or_author_kind() {
        let root = tempfile::tempdir().unwrap();
        let project = Project::new(root.path().into(), vec![]);
        let original = Target {
            member_urn: "urn:li:person:member".into(),
            name: "Member".into(),
            api_version: API_VERSION.into(),
            visibility: "PUBLIC".into(),
            organization: Some(Organization {
                urn: "urn:li:organization:12345".into(),
                name: "Company".into(),
            }),
        };
        let credentials = || Credentials {
            access_token: "test-token".into(),
            expires_at: None,
        };
        save(&project, "company", original.clone(), credentials()).unwrap();
        let credential_path = directory(&project, "company")
            .unwrap()
            .join("credentials.json");
        let original_bytes = fs::read(&credential_path).unwrap();
        for case in 0..3 {
            let mut changed = original.clone();
            match case {
                0 => {
                    changed.organization.as_mut().unwrap().urn = "urn:li:organization:67890".into()
                }
                1 => changed.member_urn = "urn:li:person:other".into(),
                _ => changed.organization = None,
            }
            assert!(save(&project, "company", changed, credentials()).is_err());
            assert_eq!(
                self::target(&project, "company").unwrap(),
                Some(original.clone())
            );
            assert_eq!(fs::read(&credential_path).unwrap(), original_bytes);
        }
        // Display-name updates do not silently change the posting identity, but
        // their changed target still requires a new publication approval.
        let mut renamed = original;
        renamed.organization.as_mut().unwrap().name = "Renamed Company".into();
        save(&project, "company", renamed.clone(), credentials()).unwrap();
        assert_eq!(self::target(&project, "company").unwrap(), Some(renamed));
    }
}
