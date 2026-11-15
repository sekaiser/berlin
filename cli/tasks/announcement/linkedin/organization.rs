//! Company-page authorization is checked at connection and again before I/O.
use super::{http::*, *};

#[derive(Clone, Copy)]
pub(super) enum Permission {
    Create,
    View,
}
impl Permission {
    fn action(self) -> &'static str {
        match self {
            Self::Create => "ORGANIC_SHARE_CREATE",
            Self::View => "ORGANIC_SHARE_VIEW_AS_AUTHOR",
        }
    }
}
pub(super) fn id(urn: &str) -> Result<&str, Error> {
    let id = urn
        .strip_prefix("urn:li:organization:")
        .context("use an organization URN such as urn:li:organization:12345")?;
    if id.is_empty()
        || id.starts_with('0')
        || !id.bytes().all(|b| b.is_ascii_digit())
        || id.parse::<u64>().is_err()
    {
        bail!("invalid LinkedIn organization ID");
    }
    Ok(id)
}
fn get(
    transport: &mut impl Transport,
    token: &str,
    target: &Target,
    path: &str,
) -> Result<Response, Error> {
    transport.send(Request {
        method: "GET",
        url: endpoint(path)?,
        bearer: Some(token),
        content_type: "application/json",
        version: Some(&target.api_version),
        body: None,
    })
}
pub(super) fn authorize(
    transport: &mut impl Transport,
    token: &str,
    target: &Target,
    organization: &str,
    permission: Permission,
) -> Result<(), Error> {
    id(organization)?;
    let encode = |value: &str| {
        percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
    };
    let action = permission.action();
    let path = format!(
        "/rest/organizationAuthorizations/(impersonator:{},organization:{},action:(organizationContentAuthorizationAction:(actionType:{action})))",
        encode(&target.member_urn),
        encode(organization)
    );
    let response = get(transport, token, target, &path)?;
    if response.status != 200 {
        bail!(
            "LinkedIn Page authorization unavailable; verify app access, rw_organization_admin, and the member's Page permissions"
        );
    }
    // LinkedIn examples use both Action and ActionType union names. Require
    // one recognized content action, the exact subject/Page, and Approved only.
    let valid_action = response.body["action"].as_object().is_some_and(|fields| {
        fields.len() == 1
            && fields.iter().any(|(kind, value)| {
                matches!(
                    kind.as_str(),
                    "OrganizationContentAuthorizationActionType"
                        | "OrganizationContentAuthorizationAction"
                        | "organizationContentAuthorizationAction"
                ) && value["actionType"] == action
            })
    });
    let approved = response.body["status"].as_object().is_some_and(|fields| {
        fields.len() == 1
            && fields
                .get("com.linkedin.organization.Approved")
                .is_some_and(|value| value.as_object().is_some_and(|v| v.is_empty()))
    });
    if response.body["impersonator"] != target.member_urn
        || response.body["organization"] != organization
        || !valid_action
        || !approved
    {
        bail!("LinkedIn has not authorized this member to perform {action} for the selected Page");
    }
    Ok(())
}
pub(super) fn bind(
    transport: &mut impl Transport,
    token: &str,
    mut target: Target,
    organization: Option<&str>,
) -> Result<Target, Error> {
    let Some(organization) = organization else {
        return Ok(target);
    };
    let id = id(organization)?;
    authorize(transport, token, &target, organization, Permission::Create)?;
    // Project only public identity fields; no Page-management data is needed.
    let response = get(
        transport,
        token,
        &target,
        &format!("/rest/organizations/{id}?projection=(id,localizedName)"),
    )?;
    if response.status != 200 || response.body["id"].as_u64() != Some(id.parse()?) {
        bail!("LinkedIn could not identify the selected Page");
    }
    target.organization = Some(Organization {
        urn: organization.into(),
        name: response.body["localizedName"]
            .as_str()
            .context("LinkedIn Page has no localized name")?
            .into(),
    });
    target.validate()?;
    Ok(target)
}
