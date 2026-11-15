//! Bounded HTTPS calls to documented LinkedIn endpoints. No redirect following.
use super::*;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    process::{Command, Stdio},
};

pub(super) struct Request<'a> {
    pub method: &'a str,
    pub url: Url,
    pub bearer: Option<&'a str>,
    pub content_type: &'a str,
    pub version: Option<&'a str>,
    pub body: Option<&'a [u8]>,
}
pub(super) struct Response {
    pub status: u16,
    pub body: Value,
    pub headers: BTreeMap<String, String>,
}
pub(super) trait Transport {
    fn send(&mut self, request: Request<'_>) -> Result<Response, Error>;
}
pub(super) struct Curl;
pub(super) fn endpoint(path: &str) -> Result<Url, Error> {
    Ok(Url::parse(&format!("https://api.linkedin.com{path}"))?)
}
pub(super) fn allowed_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
        && match url.host_str() {
            Some("api.linkedin.com") => {
                url.path() == "/v2/userinfo"
                    || url.path() == "/rest/posts"
                    || url.path().starts_with("/rest/posts/")
                    || url.path() == "/rest/images"
                    || url.path().starts_with("/rest/organizationAuthorizations/")
                    || url.path().starts_with("/rest/organizations/")
            }
            Some("www.linkedin.com") => {
                url.path() == "/oauth/v2/accessToken" || url.path().starts_with("/dms-uploads/")
            }
            _ => false,
        }
}
fn quote(value: &str) -> Result<String, Error> {
    if value.chars().any(char::is_control) {
        bail!("invalid HTTP configuration value");
    }
    Ok(value.replace('\\', "\\\\").replace('"', "\\\""))
}
impl Transport for Curl {
    fn send(&mut self, request: Request<'_>) -> Result<Response, Error> {
        if !allowed_url(&request.url) || !matches!(request.method, "GET" | "POST" | "PUT") {
            bail!("refusing an unsupported LinkedIn endpoint");
        }
        let mut config = format!(
            "url = \"{}\"\nheader = \"Content-Type: {}\"\n",
            quote(request.url.as_str())?,
            quote(request.content_type)?
        );
        if let Some(token) = request.bearer {
            if token.is_empty() || token.len() > 16384 {
                bail!("invalid LinkedIn credential");
            }
            config.push_str(&format!(
                "header = \"Authorization: Bearer {}\"\n",
                quote(token)?
            ));
        }
        if let Some(v) = request.version {
            super::version(v)?;
            config.push_str(&format!("header = \"Linkedin-Version: {v}\"\nheader = \"X-Restli-Protocol-Version: 2.0.0\"\n"));
        }
        let mut payload = tempfile::NamedTempFile::new()?;
        if let Some(body) = request.body {
            if body.len() > 10 * 1024 * 1024 {
                bail!("LinkedIn request exceeds 10 MiB");
            }
            payload.write_all(body)?;
            payload.flush()?;
        }
        let output = tempfile::NamedTempFile::new()?;
        let headers = tempfile::NamedTempFile::new()?;
        let mut command = Command::new("curl");
        command
            .args([
                "-q",
                "--silent",
                "--proto",
                "=https",
                "--max-time",
                "30",
                "--connect-timeout",
                "10",
                "--max-filesize",
                "8388608",
                "--request",
                request.method,
                "--config",
                "-",
                "--output",
            ])
            .arg(output.path())
            .arg("--dump-header")
            .arg(headers.path())
            .args(["--write-out", "%{http_code}"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if request.body.is_some() {
            command
                .arg("--data-binary")
                .arg(format!("@{}", payload.path().display()));
        }
        let mut child = command.spawn().context("LinkedIn delivery requires curl")?;
        child
            .stdin
            .take()
            .context("missing curl stdin")?
            .write_all(config.as_bytes())?;
        let result = child.wait_with_output()?;
        if !result.status.success() {
            bail!("LinkedIn request did not return a complete response");
        }
        let status = String::from_utf8(result.stdout)?.parse()?;
        let mut bytes = vec![];
        File::open(output.path())?
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 8 * 1024 * 1024 {
            bail!("LinkedIn response exceeds size limit");
        }
        let mut head = String::new();
        File::open(headers.path())?
            .take(64 * 1024 + 1)
            .read_to_string(&mut head)?;
        if head.len() > 64 * 1024 {
            bail!("LinkedIn headers exceed size limit");
        }
        // Each HTTP status starts a fresh response block (e.g. 100 Continue).
        let mut values = BTreeMap::new();
        for line in head.lines() {
            if line.starts_with("HTTP/") {
                values.clear();
            } else if let Some((k, v)) = line.split_once(':') {
                values.insert(k.to_ascii_lowercase(), v.trim().to_owned());
            }
        }
        Ok(Response {
            status,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            headers: values,
        })
    }
}
