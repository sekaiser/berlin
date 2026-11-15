# Publish a LinkedIn announcement

Berlin can publish an approved announcement as a personal member or a company
Page, with the captured teaser and alt text. It uses the documented
[Posts API](https://learn.microsoft.com/en-us/linkedin/marketing/community-management/shares/posts-api)
and [Images API](https://learn.microsoft.com/en-us/linkedin/marketing/community-management/shares/images-api).
Delivery publishes a real post; LinkedIn's creation API offers no draft mode.
Builds and reviews remain local.

API delivery is optional. You can copy the prepared text and image into LinkedIn
yourself and [record the post URL](announcement-workflow.md) afterward. Manual
posting requires no developer app or OAuth connection.

## Authorize your application

Create an app from the [LinkedIn Developer homepage](https://developer.linkedin.com/)
using **My apps → Create app**. LinkedIn's app setup requires a
[Page association](https://www.linkedin.com/help/linkedin/answer/a548360);
owning a Page alone does not grant API product access. Enable **Sign In with
LinkedIn using OpenID Connect** for member identification. For personal posting,
enable **Share on LinkedIn**; Berlin requests `openid profile w_member_social`.
Company posting needs approved access to the **Community Management API** and
the organization permissions described below. LinkedIn controls app verification,
product approval and scope availability. Berlin never requests email access.

The standard authorization-code flow needs an exact registered HTTPS callback
and your app's client secret. Native PKCE requires separate LinkedIn enablement;
Berlin does not assume it is available. See
[LinkedIn's OAuth instructions](https://learn.microsoft.com/en-us/linkedin/shared/authentication/authorization-code-flow).

For a static website, generate and deploy a standalone callback page:

~~~sh
# Use your own actual public URL.
bln announcement linkedin callback-page \
    --url https://example.com/static/auth/linkedin.html \
    --output static/auth/linkedin.html
~~~

Deploy it through the usual sealed website workflow, then register that exact
URL in the app's Auth settings. The page uses no external resources or network
calls. Its script clears the temporary response from the address bar and offers
a local download. CSP hashes restrict scripts/styles. It is an explicit noindex
utility page, excluded from the sitemap. The host still receives the initial
callback request.

~~~sh
bln announcement linkedin auth-start --account personal \
    --client-id YOUR_CLIENT_ID \
    --redirect-uri https://example.com/static/auth/linkedin.html
~~~

Open the printed authorization URL yourself, select the correct account, then
save the response using the callback page's button. In your own terminal, set
`LINKEDIN_CLIENT_SECRET` without putting its value in shell history or chat:

~~~sh
bln announcement linkedin auth-finish --account personal \
    --callback-file /absolute/path/to/linkedin-callback.txt
bln announcement linkedin status --account personal
~~~

Berlin checks the redirect, random state, duplicate parameters and a 15-minute
request lifetime. Each token exchange consumes the pending request; an interrupted
exchange needs fresh authorization. The authenticated `/v2/userinfo` response
identifies the member; an unverified ID token is not trusted. Check the returned
name and member URN. Delete the downloaded response after successful authorization.

The client secret is not saved. Access tokens and expiry are stored in mode-600
files under `.berlin/connections/linkedin/<account>/`, inside private directories.
They are separate from SQLite, release captures and approvals. Keep `.berlin`
ignored by Git and private. Database backups do not contain credentials; authorize
again after moving/restoring a project, or restore credentials separately.
Expired tokens need authorization again; no refresh-token workflow is assumed.

An existing suitably scoped token can also be inspected and explicitly bound:

~~~sh
# LINKEDIN_ACCESS_TOKEN must already be set locally.
bln announcement linkedin connect --account personal
bln announcement linkedin connect --account personal \
    --confirm-member urn:li:person:MEMBER_ID
~~~

An account label cannot switch members or posting authors. Use another label
for a different member, a different Page, or a change between personal and Page
posting. The default API version is `202609`; `--api-version YYYYMM` during
authorization/connection changes it deliberately.

## Post as a company Page

Use the Page's numeric organization URN, not its vanity URL or your member URN:

~~~sh
bln announcement linkedin auth-start --account company \
    --organization urn:li:organization:12345 \
    --client-id YOUR_CLIENT_ID \
    --redirect-uri https://example.com/static/auth/linkedin.html
bln announcement linkedin auth-finish --account company \
    --callback-file /absolute/path/to/linkedin-callback.txt
bln announcement linkedin status --account company
~~~

The selected Page is retained in the pending OAuth request. This flow requests
`openid profile w_organization_social r_organization_social rw_organization_admin`.
The write permission publishes as the Page; the read permission supports post
reconciliation. The administrative permission is required by LinkedIn's
[Organization Authorizations API](https://learn.microsoft.com/en-us/linkedin/marketing/community-management/organizations/organization-authorizations/organization-authorizations).
Berlin uses that endpoint to check `ORGANIC_SHARE_CREATE` instead of guessing
access from a role name. It does not modify the Page or its administrators.

Connection setup checks the authenticated member's permission for the exact
Page, then retrieves its ID and display name. The connection retains both the
member who authorized it and the Page that will author posts. Check both in the
status output. A denied, malformed or unavailable authorization check refuses
the connection; there is no fallback to personal posting.

Existing organization-scoped tokens can use the same selection:

~~~sh
bln announcement linkedin connect --account company \
    --organization urn:li:organization:12345
# Inspect the displayed member and Page, then repeat the same arguments:
bln announcement linkedin connect --account company \
    --organization urn:li:organization:12345 \
    --confirm-member urn:li:person:MEMBER_ID
~~~

Use `--account company` throughout review and delivery. The approval includes
the member, Page ID and Page name. Both post author and image owner use the
Page's organization URN. Delivery checks the member and Page permission again
before uploading or starting a delivery record, so a revoked permission refuses
the operation. LinkedIn still authorizes the actual write; the preliminary check
cannot eliminate a subsequent permission change.

## Review and deliver

Prepare a [channel package](channel-publishing.md), edit `linkedin.txt`, and
review after connecting:

~~~sh
bln announcement review .berlin/channels/my-article \
    --document CONTENT_ID --channel linkedin --account personal
bln announcement review .berlin/channels/my-article \
    --document CONTENT_ID --channel linkedin --account personal --approve REVIEW_ID
bln announcement deliver .berlin/channels/my-article \
    --document CONTENT_ID --channel linkedin --account personal --confirm REVIEW_ID
~~~

Approval binds the member, optional Page, API version and public audience as well as text, source
revision and teaser identity. Earlier manual approvals remain readable but do not
authorize API posting. Edited packages or changed connections need another review.

Delivery checks the token's member before uploading. PNG, JPEG and GIF teasers up
to 10 MiB are supported. Verified bytes are uploaded only to LinkedIn's documented
HTTPS upload origin, without redirects. The resulting image URN and approved alt
text accompany the post; plain text is escaped for LinkedIn's `little` format.

SQLite records preparation and commits `submitting` before the non-idempotent
post request. A successful `x-restli-id` response records its post URN/feed URL
and an `api_acknowledged` publication together in one transaction. Acceptance
does not establish anonymous visibility or completed image processing.
Repeating that review returns its receipt without another post. Previously
reported revisions and unresolved attempts also prevent reposts under edited wording.

## Resolve an uncertain submission

~~~sh
bln announcement deliveries --review REVIEW_ID
bln announcement linkedin reconcile REVIEW_ID \
    --post-urn urn:li:share:POST_ID
~~~

Timeouts, lost responses and interrupted processes remain unresolved instead of
being retried blindly. Check your LinkedIn feed first. API read-back compares
member, author, commentary, audience, published state and uploaded media fields.
LinkedIn restricts `r_member_social`; basic posting permission does not generally
allow this read-back. Company read-back requires `r_organization_social` and
a fresh `ORGANIC_SHARE_VIEW_AS_AUTHOR` check. When unavailable, verify the post
yourself and record that:

~~~sh
bln announcement linkedin reconcile REVIEW_ID \
    --post-urn urn:li:share:POST_ID --reported
~~~

This is `user_reported` evidence, not API verification, and settles the attempt
without another POST. If no matching post is found, leave the attempt unresolved;
there is no force-retry or automatic deletion. A crash during preparation can
leave an unused uploaded asset. Berlin does not delete those assets or infer
that a request with an unknown outcome had no effect.

Product access, scopes, API versions, media processing and account limits can
cause rejection. Local validation does not guarantee platform acceptance.
No computer-use automation or private LinkedIn endpoints are used.
