# Publish a Substack edition

Berlin prepares a static edition for Substack's editor and records the public
post against an exact source revision. Delivery is manual. It does not use
session cookies, private publishing endpoints, or browser automation.
Substack's documented
[AI assistant connection](https://support.substack.com/hc/en-us/articles/50834026608916-How-to-connect-Substack-to-your-AI-Assistant)
is read-only and cannot publish posts or Notes.

## Prepare and edit the edition

Run from your publishing project, or select it with `BERLIN_DIR`:

```sh
bln release --pipeline site --production
bln announcement prepare RELEASE_ID --document CONTENT_ID \
    --output .berlin/channels/my-article
```

Use a native release with a captured static edition. Earlier channel packages
remain readable; prepare a new package to obtain `substack.json`.
Unpublished documents and authoring paths are excluded from the release capture.
Preparation needs no Substack account or network access.

Edit `substack.json`. It contains the title, subtitle, body Markdown, immutable
`source_url`, publication URL, audience and email intent. Editing `article.md`
alone does not change the Substack edition. The initial `publication_url` is
`null`: set it to your publication's HTTPS root, such as
`https://yourname.substack.com/` or `https://journal.example.com/`. A profile URL
or individual `/p/...` post URL is not a publication root.

The default audience is `everyone` and `send_email` is `false` (web only).
Supported audiences are `everyone`, `free`, `paid`, and `founding`. These are
reviewed intentions: you must select the corresponding settings in Substack's
editor. They do not send email or change your subscribers. The account argument
below is a stable local label, not an authenticated Substack identity.

Keep `source_url` unchanged. The copied body always includes a link to that
original website article. The cover and its alt text remain bound to the source
release; replace them by preparing another source release/package.

## Inspect the formatted preview

```sh
bln announcement preview .berlin/channels/my-article \
    --document CONTENT_ID --channel substack --account newsletter
```

Open the returned `substack.html` file in your browser. It shows the title,
subtitle, cover, publication, audience, email intent, adaptations and review ID.
The **Copy formatted body** button copies HTML and plain text, excluding the
preview controls and metadata. Where rich clipboard access is unavailable, it
tries a local selection copy, then leaves the body selected for manual copying.
Opening the preview may load its public image URLs; Berlin does not download or
upload body images during preparation.

The HTML is a static interpretation of the captured Markdown. Paragraphs,
headings, emphasis, lists, code, images, captions, tables and footnotes remain
available. Raw HTML is omitted with a warning. Relative links become absolute;
section links point to the original article because Substack can rewrite anchors.
Interactive components and embeds need the adaptations listed in the package.
Check equations and SVG figures in the destination editor as well.

The preview uses an independent reading layout, no website scripts, and no
external script or stylesheet. Its CSP binds its embedded script and stylesheet.
It is a local review artifact; it is not added to the website.

## Approve, publish, and record

For an existing SQLite project, back up and migrate before the first use of the
new channel. Schema version 9 preserves existing approvals and publication URLs:

```sh
bln state backup /absolute/path/to/new-backup.sqlite
bln state migrate
```

For a new project without state, run `state migrate` directly.

```sh
bln announcement review .berlin/channels/my-article \
    --document CONTENT_ID --channel substack --account newsletter
bln announcement review .berlin/channels/my-article \
    --document CONTENT_ID --channel substack --account newsletter \
    --approve REVIEW_ID
```

Approval binds the exact JSON bytes, rendered body HTML, source revision,
publication, audience, email intent and captured cover. A destination is required
before approval. After edits, regenerate and inspect the preview: a stale,
missing or edited preview cannot be approved or recorded as published. The local
budgets are 512 KiB for the draft JSON, 2 MiB for body HTML and 4 MiB for the
complete preview; these are Berlin limits, not Substack platform limits.

Create a post in Substack yourself. Enter the title and subtitle separately,
paste the copied body, upload the cover and alt text, and check the actual
formatting. Select the audience and email delivery explicitly. Substack's
[publishing instructions](https://support.substack.com/hc/en-us/articles/360037831771-How-do-I-publish-a-new-post-on-Substack)
describe web-only publication and sending to subscribers. Berlin's `send_email`
setting does not configure that editor.

Once published, record the public `/p/slug` URL:

```sh
bln announcement record .berlin/channels/my-article \
    --document CONTENT_ID --channel substack --account newsletter \
    --url https://yourname.substack.com/p/your-article --confirm REVIEW_ID
bln announcement status .berlin/channels/my-article \
    --account newsletter --document CONTENT_ID
```

The URL must use the approved publication's origin, including when using a custom
domain. Tracking queries and fragments are removed. The record is
`user_reported`: it does not prove the remote body, audience, email delivery or
authenticated author. Repeated identical reports are idempotent; another URL or
an already reported article revision cannot silently replace an existing record.
Backups and the article's lifecycle history include the approval and report.

`announcement deliver --channel substack` refuses API delivery. It creates no
remote draft, email or delivery attempt. Later Substack edits do not synchronize
back to the authoring source or automatically update the recorded edition.
