# Prepare announcements and syndicated drafts

Berlin prepares a channel package from a sealed website release: an article
summary, canonical link, teaser image and alternative text, editable LinkedIn/X
announcements, and full editions for DEV and manual Substack publishing. Preparation is local.
Explicit delivery can publish a LinkedIn announcement or create an unpublished
DEV draft. Builds, previews, releases and approvals never post to social platforms.
See [LinkedIn authorization and delivery](linkedin-publishing.md).

## Capture an edition

Run from your publishing project, or select it with `BERLIN_DIR`. Keep the existing
pipeline and its `.deploy_to(...)` declaration. No social credentials belong in
Rhai or Org frontmatter.

```sh
bln release --pipeline site --production
# Save the printed release ID.
bln announcement prepare RELEASE_ID --document CONTENT_ID \
    --output .berlin/channels/my-article
```

Use a stable content ID, not a title or slug. Add `--updated` for an update
announcement; Berlin does not decide whether a revision deserves a new post.
Existing release comparison remains available through `release-drafts`.

Native releases now capture `articles.json` beside `site/`. The manifest binds
its bytes to the release ID. It contains only mapped public documents and their
static editions, not authoring paths or unpublished documents. GitHub Pages
uploads only `site/`; it does not upload the edition capture. Earlier sealed
releases remain valid but cannot acquire a capture retroactively. Imported
website directories have no semantic edition: prepare a new native release.

The package contains:

| File | Purpose |
| --- | --- |
| `linkedin.txt` | Title, authored description and canonical link |
| `x.txt` | Editable short announcement; may need shortening before approval |
| `article.md` | Static full article, with absolute public links and attribution |
| `devto.json` | DEV request: unpublished article, canonical URL, tags and cover |
| `substack.json` | Editable local edition with publication, audience and email intent |
| `teaser.*`, `teaser-alt.txt` | Copy of the release-owned preview and its alt text |
| `warnings.json` | Adaptations requiring editorial review |
| `manifest.json` | Sealed document/release identity; leave unchanged |

The authored description supplies the summary. Without one, Berlin uses the first
paragraph and warns. It does not generate a new AI summary or silently truncate
text. A teaser must be a release-owned asset of at most 10 MiB; external or missing
images produce a warning instead of a network fetch.

Markdown retains headings, lists, tables, code, figures, footnotes and references.
Interactive components become static content. Embedded HTML media becomes a link
to the original; other HTML is simplified to text. TeX is retained with a warning.
Review those adaptations in DEV's editor. If an edition cannot be projected safely,
the package still includes announcements but omits the DEV files. This is not a
promise of identical rendering across platforms.

## Edit and approve each channel

Edit `linkedin.txt`, `x.txt`, or `devto.json`. Editing `article.md` alone does not
change the DEV request: put the reviewed body into `article.body_markdown` in
`devto.json`. Keep `published: false` and the sealed canonical URL and image.
DEV frontmatter is refused because it can override request fields. At most four
normalized tags are exported. Inspect them before approval.

```sh
bln announcement review .berlin/channels/my-article \
    --document CONTENT_ID --channel linkedin --account personal
bln announcement review .berlin/channels/my-article \
    --document CONTENT_ID --channel linkedin --account personal --approve REVIEW_ID
```

Repeat for `--channel x`, `--channel devto` or `--channel substack`; each has a different review ID.
Substack also needs its formatted preview before approval; see
[Substack publishing](substack-publishing.md).
Approvals bind exact text/request bytes, account, source revision, canonical URL,
teaser URL/alt text and image hash. Changed exports require approval again. Image
or alt-text edits require a new source release/package. Export destinations are
never overwritten. LinkedIn defaults to 3,000 characters; X uses a conservative
280-character budget, weighting non-ASCII characters as two and URLs as 23. These
are local checks, not a guarantee of platform acceptance. Full DEV requests are
bounded to 2 MiB; approval records have a separate 8 MiB serialization budget.

Post LinkedIn/X text and attach the teaser manually, then record the post:

```sh
bln announcement record .berlin/channels/my-article \
    --document CONTENT_ID --channel x --account personal \
    --url https://x.com/YOUR_HANDLE/status/POST_ID --confirm REVIEW_ID
```

This records your assertion, not proof of the remote text, account or image.
See [manual announcement tracking](announcement-workflow.md) for its deduplication
and history rules. X posting remains manual; LinkedIn also supports the explicit
[API workflow](linkedin-publishing.md).

## Create a DEV draft explicitly

Back up existing state and migrate it before the first delivery:

```sh
bln state backup /absolute/path/to/new-backup.sqlite
bln state migrate
```

For a new project with no SQLite state, run `state migrate` directly. Supply a
DEV API key through a local environment variable named `DEVTO_API_KEY`, never as
a command-line argument. `curl` must be installed. Use your actual DEV username
as `--account`; Berlin checks the authenticated identity before submitting.

```sh
bln announcement review .berlin/channels/my-article \
    --document CONTENT_ID --channel devto --account YOUR_DEV_USERNAME
bln announcement review .berlin/channels/my-article \
    --document CONTENT_ID --channel devto --account YOUR_DEV_USERNAME --approve REVIEW_ID
bln announcement deliver .berlin/channels/my-article \
    --document CONTENT_ID --channel devto --account YOUR_DEV_USERNAME --confirm REVIEW_ID
bln announcement deliveries --review REVIEW_ID
```

The adapter uses the [Forem API](https://developers.forem.com/api/v1) and stores a
delivery intent in SQLite before `POST /api/articles`. It saves the returned draft
ID/URL. Repeating a completed delivery returns that receipt without another POST.
Credentials and response bodies are not stored in lifecycle records.

Timeouts, connection loss, server failures, and a crash after recording intent
leave an uncertain outcome. Berlin refuses to submit again blindly. Inspect your
DEV drafts, find the matching draft ID, and reconcile:

```sh
bln announcement reconcile REVIEW_ID --draft DEV_ARTICLE_ID
```

Reconciliation performs GET requests and checks owner, unpublished status, title,
Markdown body, canonical URL, cover URL, tags and any explicit description against
the approval. If DEV
normalizes any of those fields, it refuses rather than guessing equivalence.
It does not verify remotely served image bytes. If no matching draft can be found,
the uncertain attempt stays unresolved; this increment has no force-retry command.
An explicit client rejection permits retry. Inspect the resulting draft in DEV
and publish manually when ready. A draft receipt is not a public-post receipt,
and Berlin does not yet synchronize later remote edits or publication status.

## Substack and other channels

Substack has a first-class [manual edition workflow](substack-publishing.md):
edit `substack.json`, generate a copyable `substack.html` with `announcement preview`,
approve it, then record the manually published post URL. Destination, audience,
email intent and rendered body are included in the approval. There is no remote
Substack publishing adapter. Substack's documented
[AI assistant connection](https://support.substack.com/hc/en-us/articles/50834026608916-How-to-connect-Substack-to-your-AI-Assistant)
is read-only; it does not establish a supported write API. No browser automation
or private publishing endpoints are used.

Keep `.berlin/channels` and the lifecycle database private. Backups include delivery
history, approvals and exact approved article payloads. See
[lifecycle storage](lifecycle-state.md) for backup/restore. Before distributing,
check the original public URL and teaser are live: preparing or approving a package
does not verify website deployment.
