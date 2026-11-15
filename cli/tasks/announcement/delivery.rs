//! DEV draft delivery only. Credentials and remote response bodies never enter state.
use super::*;
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    process::{Command, Stdio},
};
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Event {
    pub review: String,
    pub state: String,
    pub at_unix_seconds: u64,
    pub remote_id: Option<u64>,
    pub url: Option<Url>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linkedin: Option<super::linkedin::Attempt>,
}
impl Event {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        release::validate_id(&self.review)?;
        if let Some(attempt) = &self.linkedin {
            attempt.target.validate()?;
            if self.remote_id.is_some()
                || !matches!(
                    self.state.as_str(),
                    "started"
                        | "submitting"
                        | "unknown"
                        | "rejected"
                        | "submitted"
                        | "reconciled"
                        | "reported"
                )
            {
                bail!("invalid LinkedIn delivery state");
            }
            if let Some(image) = &attempt.image_urn {
                super::linkedin::urn(image, "urn:li:image:")?;
            }
            let complete = self.settled();
            if complete != attempt.post_urn.is_some() || complete != self.url.is_some() {
                bail!("invalid LinkedIn delivery outcome");
            }
            if let Some(id) = &attempt.post_urn
                && self.url.as_ref() != Some(&super::linkedin::post_url(id)?)
            {
                bail!("LinkedIn post URL/URN mismatch");
            }
            return Ok(());
        }
        if !matches!(
            self.state.as_str(),
            "started" | "unknown" | "rejected" | "draft_created" | "reconciled"
        ) {
            bail!("unknown delivery state");
        }
        let complete = matches!(self.state.as_str(), "draft_created" | "reconciled");
        if complete != self.remote_id.is_some()
            || self.remote_id == Some(0)
            || complete != self.url.is_some()
        {
            bail!("invalid DEV delivery outcome");
        }
        if let Some(url) = &self.url
            && (url.scheme() != "https"
                || url.host_str() != Some("dev.to")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.port().is_some()
                || url.query().is_some()
                || url.fragment().is_some())
        {
            bail!("invalid DEV draft URL");
        }
        Ok(())
    }
    pub(crate) fn settled(&self) -> bool {
        matches!(
            self.state.as_str(),
            "draft_created" | "submitted" | "reconciled" | "reported"
        )
    }
    pub(crate) fn matches_snapshot(&self, snapshot: &Snapshot) -> Result<(), Error> {
        snapshot.validate()?;
        if self.review != snapshot.id()? {
            bail!("delivery approval mismatch");
        }
        match &self.linkedin {
            Some(a)
                if snapshot.channel == Channel::LinkedIn
                    && snapshot.linkedin.as_ref() == Some(&a.target) =>
            {
                if (snapshot.image.is_none() && a.image_urn.is_some())
                    || (matches!(
                        self.state.as_str(),
                        "submitting" | "unknown" | "submitted" | "reconciled" | "reported"
                    ) && snapshot.image.is_some() != a.image_urn.is_some())
                {
                    bail!("LinkedIn media differs from the approval");
                }
            }
            None if snapshot.channel == Channel::DevTo => {}
            _ => bail!("delivery channel or member differs from the approval"),
        }
        Ok(())
    }
    pub(crate) fn follows(&self, previous: Option<&Self>) -> bool {
        let Some(old) = previous else {
            return self.state == "started";
        };
        if old.review != self.review
            || old.linkedin.as_ref().map(|a| &a.target) != self.linkedin.as_ref().map(|a| &a.target)
        {
            return false;
        }
        if old.state == "rejected" {
            return self.state == "started";
        }
        if let Some(current) = &self.linkedin {
            let valid = matches!(
                (old.state.as_str(), self.state.as_str()),
                ("started", "submitting" | "rejected")
                    | (
                        "submitting",
                        "submitted" | "unknown" | "rejected" | "reconciled" | "reported"
                    )
                    | ("unknown", "reconciled" | "reported")
            );
            valid
                && (old.state == "started"
                    || old
                        .linkedin
                        .as_ref()
                        .is_some_and(|old| old.image_urn == current.image_urn))
        } else {
            matches!(
                (old.state.as_str(), self.state.as_str()),
                (
                    "started",
                    "unknown" | "rejected" | "draft_created" | "reconciled"
                ) | ("unknown", "reconciled")
            )
        }
    }
    pub(crate) fn publication(&self) -> Option<super::Publication> {
        if self.linkedin.is_none() || !self.settled() {
            return None;
        }
        let evidence = match self.state.as_str() {
            "reported" => super::Evidence::UserReported,
            "reconciled" => super::Evidence::ApiVerified,
            _ => super::Evidence::ApiAcknowledged,
        };
        Some(super::Publication {
            schema_version: if self.state == "reported" { 1 } else { 2 },
            review_id: self.review.clone(),
            post_url: self.url.clone().unwrap(),
            evidence,
            recorded_at_unix_seconds: self.at_unix_seconds,
        })
    }
}
pub(super) struct Response {
    pub status: u16,
    pub body: Value,
}
pub(super) trait Transport {
    fn request(&mut self, method: &str, path: &str, body: Option<&str>) -> Result<Response, Error>;
}
struct Curl {
    token: String,
}
impl Curl {
    fn from_env(name: &str) -> Result<Self, Error> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            bail!("invalid token environment variable name");
        }
        let token = std::env::var(name).context("DEV API key environment variable is missing")?;
        if token.is_empty() || token.len() > 4096 || token.chars().any(char::is_control) {
            bail!("invalid DEV API key");
        }
        Ok(Self { token })
    }
}
impl Transport for Curl {
    fn request(&mut self, method: &str, path: &str, body: Option<&str>) -> Result<Response, Error> {
        if !path.starts_with("/api/")
            || !path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_".contains(&b))
        {
            bail!("invalid DEV endpoint");
        }
        let mut input = tempfile::NamedTempFile::new()?;
        if let Some(body) = body {
            input.write_all(body.as_bytes())?;
            input.flush()?;
        }
        let output = tempfile::NamedTempFile::new()?;
        let mut command = Command::new("curl");
        command
            .args([
                "-q",
                "--silent",
                "--show-error",
                "--proto",
                "=https",
                "--max-time",
                "30",
                "--connect-timeout",
                "10",
                "--max-filesize",
                "2097152",
                "--request",
                method,
                "--url",
                &format!("https://dev.to{path}"),
                "--header",
                "Accept: application/vnd.forem.api-v1+json",
                "--header",
                "Content-Type: application/json",
                "--output",
            ])
            .arg(output.path())
            .args(["--write-out", "%{http_code}", "--config", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if body.is_some() {
            command
                .arg("--data-binary")
                .arg(format!("@{}", input.path().display()));
        }
        let mut child = command.spawn().context("DEV delivery requires curl")?;
        let token = self.token.replace('\\', "\\\\").replace('"', "\\\"");
        child
            .stdin
            .take()
            .context("missing curl stdin")?
            .write_all(format!("header = \"api-key: {token}\"\n").as_bytes())?;
        let response = child.wait_with_output()?;
        if !response.status.success() {
            bail!("DEV request did not return a complete response");
        }
        let status = String::from_utf8(response.stdout)?.parse()?;
        let mut bytes = Vec::new();
        fs::File::open(output.path())?
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 2 * 1024 * 1024 {
            bail!("DEV response exceeds size limit");
        }
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Ok(Response { status, body })
    }
}
fn owner(transport: &mut impl Transport, snapshot: &Snapshot) -> Result<(), Error> {
    let response = transport.request("GET", "/api/users/me", None)?;
    if response.status != 200
        || response.body["username"].as_str() != Some(snapshot.account.as_str())
    {
        bail!("DEV authenticated username must match the approved --account");
    }
    Ok(())
}
fn draft(snapshot: &Snapshot, id: u64, url: &str) -> Result<Event, Error> {
    let event = Event {
        review: snapshot.id()?,
        state: "draft_created".into(),
        at_unix_seconds: super::now()?,
        remote_id: Some(id),
        url: Some(Url::parse(url)?),
        linkedin: None,
    };
    event.validate()?;
    Ok(event)
}
pub(crate) fn deliver(
    project: &Project,
    source: &Source,
    confirm: &str,
    token_env: &str,
) -> Result<Event, Error> {
    let snapshot = super::load_source(project, source)?;
    if snapshot.channel != Channel::DevTo || snapshot.id()? != confirm {
        bail!("delivery requires a matching approved DEV review ID");
    }
    let history = super::store::load(project)?;
    let approved = super::require_approval(&history, confirm)?;
    if approved.approval.snapshot != snapshot {
        bail!("approved payload differs");
    }
    let mut transport = Curl::from_env(token_env)?;
    deliver_with(project, &snapshot, &mut transport)
}
pub(super) fn deliver_with(
    project: &Project,
    snapshot: &Snapshot,
    transport: &mut impl Transport,
) -> Result<Event, Error> {
    snapshot.validate()?;
    if snapshot.channel != Channel::DevTo {
        bail!("only DEV draft delivery is implemented");
    }
    let id = snapshot.id()?;
    let mut db = crate::tasks::state::Store::open(project.root())?;
    if let Some(last) = db.delivery_events(Some(&id))?.last() {
        if matches!(last.state.as_str(), "draft_created" | "reconciled") {
            return Ok(last.clone());
        }
        if last.state != "rejected" {
            bail!("previous delivery is uncertain; reconcile it before retrying");
        }
    }
    owner(transport, snapshot)?;
    let started = Event {
        review: id.clone(),
        state: "started".into(),
        at_unix_seconds: super::now()?,
        remote_id: None,
        url: None,
        linkedin: None,
    };
    if let Some(existing) = db.begin_delivery(&started)? {
        return Ok(existing);
    }
    let response = transport.request("POST", "/api/articles", Some(&snapshot.text));
    let mut event = match response {
        Ok(r)
            if r.status == 201
                && r.body["id"].as_u64().is_some_and(|id| id > 0)
                && r.body.get("published_at") == Some(&Value::Null)
                && r.body["user"]["username"].as_str() == Some(snapshot.account.as_str())
                && r.body["url"].as_str().is_some() =>
        {
            draft(
                snapshot,
                r.body["id"].as_u64().unwrap(),
                r.body["url"].as_str().unwrap(),
            )
            .unwrap_or_else(|_| Event {
                state: "unknown".into(),
                ..started.clone()
            })
        }
        Ok(r) if (400..500).contains(&r.status) && !matches!(r.status, 408 | 409) => Event {
            state: "rejected".into(),
            ..started
        },
        _ => Event {
            state: "unknown".into(),
            ..started
        },
    };
    event.at_unix_seconds = super::now()?;
    db.finish_delivery(&event)?;
    if event.state == "unknown" {
        bail!("DEV delivery outcome is unknown; inspect drafts and reconcile before retrying");
    }
    if event.state == "rejected" {
        bail!("DEV rejected this draft; review payload, credentials and platform limits");
    }
    Ok(event)
}
pub(crate) fn reconcile(
    project: &Project,
    review: &str,
    id: u64,
    token_env: &str,
) -> Result<Event, Error> {
    release::validate_id(review)?;
    let history = super::store::load(project)?;
    let snapshot = &super::require_approval(&history, review)?.approval.snapshot;
    if snapshot.channel != Channel::DevTo {
        bail!("only DEV draft reconciliation is implemented");
    }
    let mut transport = Curl::from_env(token_env)?;
    reconcile_with(project, snapshot, id, &mut transport)
}
pub(super) fn reconcile_with(
    project: &Project,
    snapshot: &Snapshot,
    id: u64,
    transport: &mut impl Transport,
) -> Result<Event, Error> {
    snapshot.validate()?;
    if snapshot.channel != Channel::DevTo || id == 0 {
        bail!("reconciliation requires an approved DEV draft");
    }
    let mut db = crate::tasks::state::Store::open(project.root())?;
    let history = db.history()?;
    if super::require_approval(&history, &snapshot.id()?)?
        .approval
        .snapshot
        != *snapshot
    {
        bail!("approved payload differs");
    }
    let events = db.delivery_events(Some(&snapshot.id()?))?;
    let last = events
        .last()
        .context("no DEV delivery intent to reconcile")?;
    if matches!(last.state.as_str(), "draft_created" | "reconciled") && last.remote_id == Some(id) {
        return Ok(last.clone());
    }
    if !matches!(last.state.as_str(), "started" | "unknown") {
        bail!("no uncertain DEV delivery to reconcile");
    }
    owner(transport, snapshot)?;
    let response = transport.request("GET", &format!("/api/articles/{id}"), None)?;
    let expected: Value = serde_json::from_str(&snapshot.text)?;
    let body = &response.body;
    if response.status != 200
        || body["id"].as_u64() != Some(id)
        || body["user"]["username"].as_str() != Some(snapshot.account.as_str())
        || body.get("published_at") != Some(&Value::Null)
        || body["title"] != expected["article"]["title"]
        || body["body_markdown"] != expected["article"]["body_markdown"]
        || body["canonical_url"] != expected["article"]["canonical_url"]
        || body.get("cover_image") != Some(&expected["article"]["main_image"])
        || remote_tags(body) != Some(expected_tags(&expected))
        || expected["article"]
            .get("description")
            .is_some_and(|description| body.get("description") != Some(description))
    {
        bail!(
            "remote DEV draft does not match the approved author, content, canonical URL, cover image and tags"
        );
    }
    let mut event = draft(
        snapshot,
        id,
        body["url"]
            .as_str()
            .context("DEV draft response has no URL")?,
    )?;
    event.state = "reconciled".into();
    db.finish_delivery(&event)?;
    Ok(event)
}
fn remote_tags(body: &Value) -> Option<Vec<String>> {
    let mut tags = body["tag_list"]
        .as_array()?
        .iter()
        .map(|v| v.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    tags.sort();
    Some(tags)
}
fn expected_tags(body: &Value) -> Vec<String> {
    let mut tags = body["article"]["tags"]
        .as_str()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    tags.sort();
    tags
}
