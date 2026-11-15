use super::super::delivery::Event;
use super::{
    connection,
    http::{self, Curl, Request, Response, Transport},
    *,
};
use serde_json::{Value, json};

fn api(
    transport: &mut impl Transport,
    token: &str,
    target: &Target,
    method: &'static str,
    path: &str,
    body: Option<&Value>,
) -> Result<Response, Error> {
    let bytes = body.map(serde_json::to_vec).transpose()?;
    transport.send(Request {
        method,
        url: http::endpoint(path)?,
        bearer: Some(token),
        content_type: "application/json",
        version: Some(&target.api_version),
        body: bytes.as_deref(),
    })
}
pub(super) fn little(text: &str) -> String {
    let mut output = String::new();
    for c in text.chars() {
        if "|{}@[]()<>#\\*_~".contains(c) {
            output.push('\\');
        }
        output.push(c);
    }
    output
}
pub(super) fn payload(snapshot: &Snapshot, image: Option<&str>) -> Result<Value, Error> {
    let target = snapshot
        .linkedin
        .as_ref()
        .context("LinkedIn author is not bound; connect and review again")?;
    if snapshot.channel != Channel::LinkedIn
        || !snapshot.text.contains(snapshot.article_url.as_str())
    {
        bail!("a LinkedIn announcement must include its sealed article URL");
    }
    let mut value = json!({"author":target.author_urn(),"commentary":little(&snapshot.text),
        "visibility":target.visibility,"distribution":{"feedDistribution":"MAIN_FEED","targetEntities":[],"thirdPartyDistributionChannels":[]},
        "lifecycleState":"PUBLISHED","isReshareDisabledByAuthor":false});
    match (&snapshot.image, image) {
        (Some(preview), Some(id)) => {
            super::urn(id, "urn:li:image:")?;
            value["content"] =
                json!({"media":{"id":id,"title":snapshot.document.title,"altText":preview.alt}});
        }
        (None, None) => {}
        _ => bail!("LinkedIn teaser differs from the approval"),
    }
    Ok(value)
}
fn image_bytes(
    project: &Project,
    source: &Source,
    snapshot: &Snapshot,
) -> Result<Option<Vec<u8>>, Error> {
    let Some(image) = &snapshot.image else {
        return Ok(None);
    };
    let filename = image.filename()?;
    if !["png", "jpg", "jpeg", "gif"]
        .iter()
        .any(|ext| filename.to_ascii_lowercase().ends_with(&format!(".{ext}")))
    {
        bail!("LinkedIn image delivery requires a PNG, JPEG or GIF teaser");
    }
    let root = project.root().join(&source.directory);
    let path = root.join(&filename);
    release::ensure_file(&path)?;
    let file = release::ReleaseFile {
        path: filename,
        bytes: std::fs::metadata(path)?.len(),
        sha256: image.sha256.clone(),
    };
    Ok(Some(release::read_verified_file(
        &root,
        &file,
        10 * 1024 * 1024,
    )?))
}
fn prepare_image(
    transport: &mut impl Transport,
    token: &str,
    target: &Target,
    bytes: Option<&[u8]>,
) -> Result<Option<String>, Error> {
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    let response = api(
        transport,
        token,
        target,
        "POST",
        "/rest/images?action=initializeUpload",
        Some(&json!({"initializeUploadRequest":{"owner":target.author_urn()}})),
    )?;
    if response.status != 200 {
        bail!("LinkedIn refused image initialization");
    }
    let image = response.body["value"]["image"]
        .as_str()
        .context("LinkedIn returned no image URN")?
        .to_owned();
    super::urn(&image, "urn:li:image:")?;
    let url = Url::parse(
        response.body["value"]["uploadUrl"]
            .as_str()
            .context("LinkedIn returned no image upload URL")?,
    )?;
    if !http::allowed_url(&url)
        || url.host_str() != Some("www.linkedin.com")
        || !url.path().starts_with("/dms-uploads/")
    {
        bail!("refusing an unexpected LinkedIn image upload destination");
    }
    if response.body["value"]["uploadUrlExpiresAt"]
        .as_u64()
        .is_some_and(|t| super::super::now().is_ok_and(|now| t / 1000 <= now))
    {
        bail!("LinkedIn image upload URL has expired");
    }
    let response = transport.send(Request {
        method: "PUT",
        url,
        bearer: Some(token),
        content_type: "application/octet-stream",
        version: None,
        body: Some(bytes),
    })?;
    if !matches!(response.status, 200 | 201) {
        bail!("LinkedIn did not accept the image upload");
    }
    Ok(Some(image))
}
pub(crate) fn deliver(
    project: &Project,
    source: &Source,
    confirm: &str,
    token_env: Option<&str>,
) -> Result<Event, Error> {
    let _lock = super::super::store::lock(project)?;
    let snapshot = super::super::load_source(project, source)?;
    if snapshot.id()? != confirm || snapshot.channel != Channel::LinkedIn {
        bail!("delivery requires the approved LinkedIn review ID");
    }
    let history = super::super::store::load(project)?;
    if super::super::require_approval(&history, confirm)?
        .approval
        .snapshot
        != snapshot
    {
        bail!("approved LinkedIn payload differs");
    }
    let target = snapshot
        .linkedin
        .as_ref()
        .context("LinkedIn is not connected; authorize and review again")?;
    if connection::target(project, &source.account)?.as_ref() != Some(target) {
        bail!("LinkedIn connection changed; review again");
    }
    let db = crate::tasks::state::Store::open(project.root())?;
    if let Some(last) = db.delivery_events(Some(confirm))?.last() {
        if last.settled() {
            return Ok(last.clone());
        }
        if last.state != "rejected" {
            bail!("previous LinkedIn delivery is uncertain; reconcile before retrying");
        }
    }
    for entry in history
        .iter()
        .filter(|e| e.approval.snapshot.same_revision(&snapshot) && e.approval.review_id != confirm)
    {
        if entry.publication.is_some()
            || db
                .delivery_events(Some(&entry.approval.review_id))?
                .last()
                .is_some_and(|e| e.state != "rejected")
        {
            bail!(
                "this revision/account already has a reported or unresolved delivery: {}",
                entry.approval.review_id
            );
        }
    }
    drop(db);
    let token = connection::credential(project, &source.account, token_env)?;
    let image = image_bytes(project, source, &snapshot)?;
    deliver_with(project, &snapshot, &token, image.as_deref(), &mut Curl)
}
pub(super) fn deliver_with(
    project: &Project,
    snapshot: &Snapshot,
    token: &str,
    bytes: Option<&[u8]>,
    transport: &mut impl Transport,
) -> Result<Event, Error> {
    snapshot.validate()?;
    payload(
        snapshot,
        snapshot.image.as_ref().map(|_| "urn:li:image:preflight"),
    )?;
    let target = snapshot
        .linkedin
        .as_ref()
        .context("missing LinkedIn target")?;
    let id = snapshot.id()?;
    let mut db = crate::tasks::state::Store::open(project.root())?;
    let history = db.history()?;
    if super::super::require_approval(&history, &id)?
        .approval
        .snapshot
        != *snapshot
    {
        bail!("approved LinkedIn payload differs");
    }
    if let Some(last) = db.delivery_events(Some(&id))?.last() {
        if last.settled() {
            return Ok(last.clone());
        }
        if last.state != "rejected" {
            bail!("previous delivery is uncertain; reconcile before retrying");
        }
    }
    connection::verify_target(
        transport,
        token,
        target,
        super::organization::Permission::Create,
    )?;
    if snapshot.image.is_some() != bytes.is_some()
        || bytes.is_some_and(|b| release::digest(b) != snapshot.image.as_ref().unwrap().sha256)
    {
        bail!("LinkedIn image bytes differ from the approval");
    }
    let mut event = Event {
        review: id,
        state: "started".into(),
        at_unix_seconds: super::super::now()?,
        remote_id: None,
        url: None,
        linkedin: Some(Attempt {
            target: target.clone(),
            image_urn: None,
            post_urn: None,
        }),
    };
    if let Some(old) = db.begin_delivery(&event)? {
        return Ok(old);
    }
    let image = match prepare_image(transport, token, target, bytes) {
        Ok(image) => image,
        Err(_) => {
            event.state = "rejected".into();
            event.at_unix_seconds = super::super::now()?;
            db.finish_delivery(&event)?;
            bail!("LinkedIn media preparation failed; no post was submitted");
        }
    };
    let body = payload(snapshot, image.as_deref())?;
    event.state = "submitting".into();
    event.at_unix_seconds = super::super::now()?;
    event.linkedin.as_mut().unwrap().image_urn = image;
    db.finish_delivery(&event)?; // Durable boundary before the non-idempotent POST.
    let response = api(transport, token, target, "POST", "/rest/posts", Some(&body));
    event.state = "unknown".into();
    match response {
        Ok(r) if r.status == 201 => {
            if let Some(id) = r.headers.get("x-restli-id")
                && let Ok(url) = super::post_url(id)
            {
                event.state = "submitted".into();
                event.linkedin.as_mut().unwrap().post_urn = Some(id.clone());
                event.url = Some(url);
            }
        }
        Ok(r) if (400..500).contains(&r.status) && !matches!(r.status, 408 | 409) => {
            event.state = "rejected".into()
        }
        _ => {}
    }
    event.at_unix_seconds = super::super::now()?;
    db.finish_delivery(&event)?;
    match event.state.as_str() {
        "submitted" => Ok(event),
        "rejected" => bail!(
            "LinkedIn rejected the post; check permissions, limits and API version before retrying"
        ),
        _ => bail!(
            "LinkedIn submission outcome is unknown; inspect your profile and reconcile before retrying"
        ),
    }
}
pub(crate) fn reconcile(
    project: &Project,
    review: &str,
    post: &str,
    reported: bool,
    token_env: Option<&str>,
) -> Result<Event, Error> {
    let _lock = super::super::store::lock(project)?;
    let history = super::super::store::load(project)?;
    let snapshot = &super::super::require_approval(&history, review)?
        .approval
        .snapshot;
    let mut db = crate::tasks::state::Store::open(project.root())?;
    let mut event = db
        .delivery_events(Some(review))?
        .pop()
        .context("no LinkedIn submission to reconcile")?;
    if event.linkedin.is_none() {
        bail!("this is not a LinkedIn delivery");
    }
    if event.settled() && event.linkedin.as_ref().unwrap().post_urn.as_deref() == Some(post) {
        return Ok(event);
    }
    if !matches!(event.state.as_str(), "unknown" | "submitting") {
        bail!("no uncertain LinkedIn submission to reconcile");
    }
    let target = snapshot
        .linkedin
        .as_ref()
        .context("missing LinkedIn target")?;
    let url = super::post_url(post)?;
    if !reported {
        let token = connection::credential(project, &snapshot.account, token_env)?;
        verify_remote(snapshot, &event, &token, post, &mut Curl)?;
    }
    event.matches_snapshot(snapshot)?;
    if event.linkedin.as_ref().unwrap().target != *target {
        bail!("LinkedIn member mismatch");
    }
    event.state = if reported { "reported" } else { "reconciled" }.into();
    event.at_unix_seconds = super::super::now()?;
    event.url = Some(url);
    event.linkedin.as_mut().unwrap().post_urn = Some(post.into());
    db.finish_delivery(&event)?;
    Ok(event)
}
pub(super) fn verify_remote(
    snapshot: &Snapshot,
    event: &Event,
    token: &str,
    post: &str,
    transport: &mut impl Transport,
) -> Result<(), Error> {
    super::post_url(post)?;
    let attempt = event
        .linkedin
        .as_ref()
        .context("missing LinkedIn delivery")?;
    connection::verify_target(
        transport,
        token,
        &attempt.target,
        super::organization::Permission::View,
    )?;
    let encoded = percent_encoding::utf8_percent_encode(post, percent_encoding::NON_ALPHANUMERIC);
    let response = api(
        transport,
        token,
        &attempt.target,
        "GET",
        &format!("/rest/posts/{encoded}?viewContext=AUTHOR"),
        None,
    )?;
    if response.status != 200 {
        bail!(
            "LinkedIn read-back unavailable; members require restricted r_member_social and Pages require r_organization_social. Inspect the post and use --reported only after checking it yourself"
        );
    }
    let expected = payload(snapshot, attempt.image_urn.as_deref())?;
    for field in ["author", "commentary", "visibility"] {
        if response.body[field] != expected[field] {
            bail!("LinkedIn post differs from the approved content/member");
        }
    }
    if response.body["id"] != post
        || response.body["lifecycleState"] != "PUBLISHED"
        || response.body["distribution"]["feedDistribution"] != "MAIN_FEED"
    {
        bail!("LinkedIn post has not reached the approved published state");
    }
    if snapshot.image.is_some() {
        for field in ["id", "altText", "title"] {
            if response.body["content"]["media"][field] != expected["content"]["media"][field] {
                bail!("LinkedIn post media differs from the approved upload");
            }
        }
    } else if !response.body["content"].is_null() {
        bail!("unexpected LinkedIn media");
    }
    Ok(())
}
