# Review and track manual announcements

Berlin can approve exact LinkedIn announcement text and record your report of
posting it manually. These commands run locally: they do not authenticate to
LinkedIn, publish, schedule posts, fetch URLs, or verify remote delivery.

This workflow starts with [release announcement drafts](website-releases.md#prepare-announcement-drafts-for-review),
not the full-document drafts produced by Rhai's `render_linkedin` operation.
Two verified native releases of the same website provide the document identities
and revision fingerprints. You decide whether a change merits an announcement.

## Review the wording

Run from your publishing project, or select it through `BERLIN_DIR`:

```sh
bln release-drafts BEFORE_ID AFTER_ID --output .berlin/review-update
```

Edit the exported `.txt` file for the article. The generated title and URL are
starting points, not summaries; add your own explanation. Leave `manifest.json`
unchanged. Berlin verifies its release pair before trusting document IDs, URLs,
or filenames. The manifest describes the original export, not your edited text.

Choose a stable local account name, such as `personal`. This name distinguishes
your records; it is not a LinkedIn account connection or an authenticated identity.
Names accept lowercase ASCII letters, digits, hyphens and underscores, up to 64
characters. Reuse the same name for the same account.

```sh
bln announcement review .berlin/review-update \
    --document CONTENT_ID --account personal
```

This is read-only. It shows the actual text file, character count, release IDs,
content fingerprint, review ID, and any matching saved approval or reported post.
Use the stable content ID, not the title or filename. Add `--json` for structured
output, including the exact text and its SHA-256 hash.

The review ID binds the channel, account name, release pair, website URL and
destination, document revision, article URL, and exact UTF-8 text bytes. Newlines
and trailing whitespace count; Berlin does not silently trim or normalize them.
Moving a review directory does not change the ID.

## Approve the exact snapshot

Read the text and check its source context, then explicitly approve the review ID
printed by the preceding command:

```sh
bln announcement review .berlin/review-update \
    --document CONTENT_ID --account personal --approve REVIEW_ID
```

Approval saves the exact text and revision context, not permission to post any
later edits. Berlin refuses empty text or text exceeding its default limit of
3,000 Unicode characters. It does not truncate drafts. Unsupported control
characters and oversized files are rejected; this is not a guarantee of platform
acceptance.

If the text, account, or release context changes, the old review ID no longer
matches. Preview and approve the new snapshot. Older approvals remain in history;
they are not overwritten. Approving an unchanged snapshot again is idempotent.

## Post manually, then record your report

First verify that the intended article URL is live. Publish the approved text
yourself in LinkedIn, then copy the resulting post URL:

```sh
bln announcement record .berlin/review-update \
    --document CONTENT_ID --account personal \
    --url https://www.linkedin.com/posts/YOUR_POST \
    --confirm REVIEW_ID
```

This command rereads the draft and requires a matching saved approval. It records
your assertion that those bytes were posted. If you edited the text while posting,
update the local file and review and approve that version before recording it.

Berlin only checks URL syntax: HTTPS on `linkedin.com` or `www.linkedin.com`, a
nonempty path, and no credentials or nondefault port. It normalizes the hostname,
removes query/fragment tracking and trailing slashes. It does **not** establish
that the URL is a post, exists, belongs to the named account, or contains the
approved text. The record has `evidence: "user_reported"`; its timestamp is when
the report was recorded, not a verified publication time.

Repeating the same review ID and normalized URL returns the original record.
Berlin refuses to overwrite its URL or bind one post URL to another review. It
also refuses a second reported announcement of the same document content for
the same website/destination and account, even after rewording the draft or
creating a theme-only release. A genuinely different content fingerprint can
have a new report. This initial workflow has no repost override, correction,
deletion, remote reconciliation, or revocation command. These checks protect
local history; they cannot prevent duplicate posts made outside Berlin.

## Inspect status

```sh
# Saved history only; source files, exports and releases need not remain present.
bln announcement status
bln announcement status --account personal --document CONTENT_ID --json

# Also compare the current exported drafts with saved approvals and reports.
bln announcement status .berlin/review-update --account personal --json
```

Current snapshots are `review_required`, `approved`, or `reported_published`.
`same_revision_reports` points to earlier reports for that document content and
account, including when the current wording requires a new review. History retains
the exact approved snapshots and optional reported-publication records. The JSON
status explicitly sets `remote_verified` and `website_live_checked` to `false`.

Without a review directory, status describes only saved snapshots; it does not
claim they match your latest files. With a directory, Berlin verifies both sealed
releases again. None of these commands reload Rhai or authoring inputs, accept
pipeline-file overrides, or infer which release is currently live.

## Local state and durability

```text
.berlin/announcements/
  .lock
  approvals/<review-id>.json
  reported/<review-id>.json
```

Treat this as private operational state, not generated website output or a
disposable cache. Back it up with other local publishing records. It includes
the reviewed social text, local account labels, release/revision context and
reported URLs, but no article source bodies or source paths. Lost records cannot
be reconstructed from LinkedIn by this workflow.

Writers use a separate exclusive lock, synced temporary files and no-clobber
installation. They do not acquire the build/recovery lock, alter preview output,
or touch GitHub Pages publication records. Read-only review and status create no
state. A competing writer fails with a retry message. Malformed or conflicting
records fail closed; existing history is never replaced to recover from an error.
Uncommitted `.pending-*` files are ignored. Hash checks detect accidental changes;
they are not signatures, a tamper-proof audit log, or protection against someone
who controls the local files.
