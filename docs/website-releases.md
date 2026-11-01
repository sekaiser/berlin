# Website releases and GitHub Pages

For projects migrated with `bln state migrate`, SQLite owns publication and
approval records. JSON storage paths below describe unmigrated projects; see
[lifecycle state and backup/restore](lifecycle-state.md).

Berlin can seal a website build and explicitly publish that exact bundle to an
existing GitHub.com Pages site. This adapter uses a dedicated `gh-pages` branch,
not a custom Actions artifact workflow. It does not configure the repository,
change its default branch, or run on `build`, `plan`, `check`, or `serve`.

## Declare a destination

Attach a destination to the existing website output in your Rhai pipeline:

```rhai
output render_html(website, "_site", presentation)
    .deploy_to(github_pages("owner/notebook"));
```

`presentation` must declare the actual public HTTPS URL in `html_config`.
For example, `https://owner.github.io/notebook` for repository Pages, or your
custom domain. The URL and repository are part of the release identity.
Deployment is typed graph metadata; evaluating the declaration performs no I/O.
Only website outputs accept this destination. The release command currently
requires one website renderer, with all declared output paths inside that
website's output directory.

## Prepare and review locally

```sh
bln release --pipeline site
# Prints a SHA-256 release ID. Save it for the following commands.
bln release-plan RELEASE_ID
```

`release` builds into staging without replacing `_site` or the configured preview
output. It supports the same repeatable `--pipeline-file` arguments as `build`.
Run the separate Org producer first when your pipeline consumes exported Markdown.

The result is stored in:

```text
.berlin/releases/<release-id>/
  manifest.json
  site/
```

The manifest identifies the pipeline, website node, Berlin version, public URL,
destination, and sorted file inventory with byte sizes and SHA-256 hashes. Native
Berlin builds also include a versioned document inventory in the manifest, outside
the published `site/` directory. The release ID hashes the complete manifest,
including that inventory when present. Files are copied, not hard-linked to mutable
build output. Identical results reuse the same ID; altered files cannot be used
under the old ID. This detects modification; it is not a cryptographic signature
or a guarantee against someone who controls the machine.

Berlin requires a nonempty `index.html`, rejects symlinks/special files and Git
control paths, and checks for common preview-only HTML references. It adds
`.nojekyll`, Berlin's licence/notice files under `licenses/berlin`, and a `CNAME`
for custom domains. Conflicting supplied notice/CNAME files fail preparation.
Normal authored downloads, including CSV files, remain allowed.

Without `--production`, Berlin does not rewrite URLs, fingerprint assets, or
generate a sitemap. Templates, CSS, authored links and attachments must already
work under the configured URL prefix. Inspect the sealed `site/` directory before
authorizing publication; additional validators must not alter it. Neither the
basic guards nor production preparation constitute a complete accessibility,
privacy, rights, or secret audit.

`release-plan` rechecks the complete inventory and reports the exact destination
and files. It does not reload the pipeline or contact GitHub. Subsequent edits
and builds cannot change an existing release.

### Native production preparation

For a website with canonical URLs in its templates, prepare production files
inside the same native build that supplies the document revision inventory:

```sh
bln release --production --pipeline-file production.rhai --pipeline site
# Alternatively, install production output locally without requiring a destination:
bln build --production --pipeline-file production.rhai --pipeline site
```

The selected configuration must supply the public HTTPS website URL. Berlin does
not infer it from a repository name or edit your configuration. `--production`
requires exactly one website renderer, with all declared outputs inside its
output directory. Separate Org export remains an explicit preceding step.

After the renderer and asset-copy operations complete, Berlin prepares their
transaction-owned output:

- Normalizes local HTML `href`, `src`, `poster`, `srcset`, and social-image URLs
  against the configured website base. Authored `/path` links are site-relative;
  paths already carrying the site's prefix are not prefixed twice.
- Adds content-hash query versions to local assets. The existing CSS parser
  rewrites stylesheet imports and `url()` references before computing dependent
  hashes, so an image change updates the referring CSS and HTML URLs.
- Checks local destinations and HTML fragments, exact absolute canonical URLs,
  Berlin redirect targets, and common preview references. Redirected fragments
  resolve against the destination page. Cyclic CSS dependencies fail preparation.
- Generates `sitemap.xml` and `robots.txt`, replacing any staged copies, excludes
  redirect aliases from the sitemap, and includes Berlin's notices and `.nojekyll`.
- Rejects `.org`, `.csv`, `.DS_Store`, and the legacy `static/js/live.js` asset in
  this stricter production mode. Ordinary builds/basic sealing retain their
  existing behavior, including intentionally published CSV downloads.

HTML is parsed rather than rewritten as arbitrary text: prose, code specimens,
comments, and script bodies remain intact. This is not a general web bundler:
JavaScript-generated URLs, inline CSS, SVG documents, external destinations, and
social-platform previews are not validated or rewritten. HTML `<base href>`
overrides are rejected. Templates must emit absolute canonicals; use Berlin's
`config_site_url` and `page_path` context values. Generated HTML is expected to use
semicolon-terminated character references in URL attributes.

`release --production` seals only after preparation succeeds, keeps the mapped
document inventory from that same execution, and leaves preview output untouched.
`build --production` instead installs the prepared output transactionally, just
like a regular build. Failed preparation does not install output or seal a bundle.
`build --production --dry-run` validates the plan but does not run production file
checks or write files. No mode publishes or executes a post-build command hook.

Two such native releases can be passed directly to `release-diff` and
`release-drafts`. Earlier imports still lack document history: establish a new
native baseline rather than assigning today's source metadata to old bundles.

### Seal a prepared directory

If a separate preparation step rewrites URLs, fingerprints assets, or validates
links, finish that work **before** sealing:

```sh
bln release --pipeline-file production.rhai --pipeline site \
    --from-directory /path/to/prepared-website
bln release-plan RELEASE_ID
```

The selected Rhai program is evaluated and its plan validated to obtain the
website's public URL and deployment target. No pipeline build operations run:
Berlin does not export Org, render pages, compile CSS, or copy declared assets.
The supplied directory is the entire website output, not the project's root.
Relative directory arguments resolve against the project (`BERLIN_DIR`);
absolute paths may point to an external preparation workspace.

Berlin copies those files, verifies their hashes, and adds the same required
Pages support files and notices to its private copy. It never modifies the
supplied directory. Conflicting support files and unsafe paths still fail.
`--from-directory` cannot be combined with `--production`.
Imports have no document inventory: current source files, receipts, or a copied
manifest are not evidence that they describe these prepared bytes. Native builds
and imports therefore have different IDs even when their final files match;
repeating either operation with identical files and manifest values reuses its ID.
The manifest binds the resulting files and the
declared destination; it is not proof that imported files were produced by that
pipeline or passed external checks. Ensure their URLs match the declaration,
then review the final sealed bundle, including any added support files.

There are no arbitrary command hooks in the pipeline or publisher. This is a
handoff for a finished website, not permission to mutate a sealed release.

## Compare releases locally

Before publishing an update, compare its sealed bundle with an earlier one:

```sh
bln release-diff BEFORE_ID AFTER_ID
bln release-diff BEFORE_ID AFTER_ID --json
```

Both IDs must exist under the selected project's `.berlin/releases`. Berlin
verifies both complete bundles before reporting added, modified, removed, and
unchanged file counts. Added, modified, and removed paths are listed in sorted
order within each category. Modification is based on bytes and SHA-256 hashes,
not timestamps; same-size edits are detected. Renames appear as a removal and
an addition at file level. The separate document comparison described below
distinguishes mapped document changes from generated-file changes.

Release metadata is compared separately: destination, public URL, pipeline,
website node, and Berlin version. Metadata-only changes remain visible even
when all files are identical. The JSON report has `schema_version: 2`, `before`
and `after` IDs with metadata, a `metadata_changed` flag, and `files` containing
`added`, `modified`, `removed`, and `unchanged`. Added and removed entries include
path, byte size, and SHA-256; modified entries contain both versions.

The command is read-only and offline. It does not load authoring inputs or Rhai,
accept pipeline file overrides, acquire the build/recovery lock, create receipts,
or consult publication records or GitHub. It does not choose a baseline or assert
that either release is currently live. Differences are a successful result
(exit status zero); missing or altered bundles fail without a partial report.
Comparing a release with itself still verifies its integrity.

### Document revisions

For two native Berlin releases with document inventories, the same command also
reports added, modified, removed, and unchanged documents, matched by their
stable content IDs. A changed title or route remains a modification of the same
document, not a deletion and addition. Changing the content ID creates a different
document. Paths are output-relative; a change to the site's public URL is reported
separately in release metadata.

Each document records its ID, public title, route, and two SHA-256 fingerprints:

- **Content:** the mapped document's kind, blocks (including code and links), and
  explicit relations.
- **Metadata:** its complete typed metadata, including description, tags, authored
  dates, authors, preview references, slugs, and comments setting.

These are derived from the resolved local-document routes in the exact `SiteAssembly` supplied to the selected
renderer, after mappings and draft exclusion. Neither fingerprint includes
provenance paths, raw source hashes, templates, CSS, generated HTML, derived
backlinks, or build timestamps. Moving a source file or changing a YAML comment
does not itself create a document revision. A `lastmod` edit is a metadata change,
not a content change. A theme-only edit may change files while documents remain
unchanged. Documents retain their vector ordering and typed structure when hashed;
this detects structural changes, not equivalent meaning or editorial significance.

The `documents` JSON member has `status: "compared"` and `changes` with `added`,
`modified`, `removed`, and `unchanged`. Lists are sorted by document ID. Modified
entries include before/after revisions and independent `content_changed`,
`metadata_changed`, and `route_changed` flags. No full prose or source paths are
stored in the inventory. It is sealed local state, not an extra public site asset.

Native releases use manifest schema 2 with fingerprint version 1. Schema-1
releases remain readable under their original IDs, with no rewrite or migration;
prepared-directory imports still use schema 1. When either inventory is absent,
`documents.status` is `"unavailable"`, with `before_available` and `after_available`
flags rather than misleading zero-change counts. An empty inventory is different:
it is a known publication containing no documents. Berlin never reloads today's
authoring inputs to fill in missing release history.

This does not attribute image/download byte changes, reading-feed changes, or
updates in linked documents to individual articles. Those remain visible through
the file comparison when they change the output. The comparison does not decide
whether a revision merits an announcement or alter publication dates.
Parser/model serialization changes may affect fingerprints; fingerprint versions
must change when their serialization contract changes. Unsupported versions fail
verification rather than silently comparing incompatible hashes.

## Prepare announcement drafts for review

Use the same pair of releases to prepare local LinkedIn announcement starting
points, without running the pipeline or contacting a platform:

```sh
bln release-drafts BEFORE_ID AFTER_ID
bln release-drafts BEFORE_ID AFTER_ID --json
# Optional: export text files into a new review directory.
bln release-drafts BEFORE_ID AFTER_ID --output .berlin/review-update
```

Both releases must have document inventories and the same website URL and
deployment destination. Missing history is an error, not an empty successful
plan. Website migrations can be inspected with `release-diff`, but are not
announcement inputs. The command does not accept pipeline file overrides.

Candidates are new document IDs and documents whose content fingerprint changed,
sorted by ID. Metadata-only changes (including title, tags, and `lastmod`),
route-only changes, removals, and unchanged documents are skipped and counted.
Theme and asset changes alone generate no announcements. A content revision that
also changes metadata or its route uses the new title and route. URLs retain the
sealed website's path prefix and encode literal output-path components.

Each draft contains only `New: <title>` or `Updated: <title>` followed by the
document's URL. These are **starting points, not summaries**: the inventory cannot
explain what changed, and structural changes do not necessarily merit a post.
Add your explanation, review the wording and diagnostics, and verify the intended
release and URL are live before posting manually. Preparing a draft does not
approve it or establish that either release is live. Existing publication records
are not consulted by `release-drafts`. The separate
[manual announcement workflow](announcement-workflow.md) approves your edited
text and records user-reported post URLs. Neither workflow calls the LinkedIn
API or schedules posts; local records do not verify delivery or prevent manual
posts made outside Berlin.

The version-1 JSON plan records both release identities and metadata, the channel,
document revisions and URLs, generated drafts with character counts/diagnostics,
skip counts, `review_required: true`, and `live_status_checked: false`. It contains
no article bodies or source paths. No current authoring files are loaded to fill
in a summary. Berlin's existing LinkedIn renderer supplies the text diagnostics;
over-limit text is retained for review, not silently truncated.

Without `--output`, this operation only prints the plan. With `--output`, relative
paths resolve against the project (`BERLIN_DIR`). The parent must already exist
and the destination must not exist, even as an empty directory or symlink. The
export creates one `.txt` per candidate and a `manifest.json`; a zero-candidate
export contains only the manifest. Filename collisions fail before creating the
directory. Export into this project's release, publication, announcement, or Git
state is refused. Existing review directories and hand-edited drafts are never
replaced.

The manifest records the generated starting points, not subsequent edits to text
files, approval, or publication. It is written last. An I/O failure leaves the new
directory for inspection and reports an incomplete export; Berlin does not delete
its contents or retry over it. Choose a fresh directory if you need another export.

This command consumes sealed release artifacts. It does not change the existing
`render_text` compositions, which project complete structural excerpts into local
drafts rather than announcing changes between releases.

## Publish explicitly

Prerequisites:

- Install Git and the GitHub CLI (`gh`), and authenticate `gh` for `github.com`.
- Use an existing Pages site configured to **Deploy from a branch**, selecting
  `gh-pages` and `/ (root)`. Keep the repository's default branch separate.
- Use credentials permitted to push that branch and read Pages configuration
  and build status. Branch-protection rules still apply.
- The Pages URL must match the sealed release URL. Configure custom domains and
  HTTPS in GitHub first.

```sh
bln publish RELEASE_ID --confirm RELEASE_ID
bln publication RELEASE_ID
bln publication RELEASE_ID --refresh
```

`publish` replaces the **entire tree** on `gh-pages`, including removing old files
not present in the release. This branch must contain only generated website
output. Commit history is retained; the current branch head is the new commit's
parent. A lease prevents overwriting a branch that changes during publication.
No source files, local Git working tree, or global Git configuration are changed.

Confirmation binds the exact artifact and destination. Sealed-release commands
do not accept pipeline file overrides. They neither rebuild nor retarget a
release. Credentials are provided by `gh`, not by the Rhai script or manifest.
Check `gh auth status --hostname github.com` before publishing when using multiple
accounts. The active account must have the required access; Berlin does not
automatically select the repository owner or switch your active account.
`GH_TOKEN` and `GITHUB_TOKEN`, when set, override stored credentials according to
the [GitHub CLI environment rules](https://cli.github.com/manual/gh_help_environment).
Keep tokens outside scripts, manifests, and publication records.

Command errors identify the failed Git operation or GitHub resource without
echoing raw subprocess output, which may contain credentials.

The adapter uses GitHub's [branch publishing workflow](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site)
and [Pages configuration/build APIs](https://docs.github.com/en/rest/pages/pages).
A push made with the Actions `GITHUB_TOKEN` does not trigger a branch-based Pages
build; do not use that token for this adapter. Custom Actions artifact deployment
is a separate integration and is not implemented here.

## Publish an update

Update the authoring inputs and rerun any required producer, such as the separate
Org export pipeline. Seal and review the finished website using the same
preparation workflow as the first release, then publish its release ID explicitly.
Use `release-diff BEFORE_ID AFTER_ID` to review changes between the saved bundles;
the comparison does not authorize publication or replace `--confirm`.
Each changed release has its own publication record; its Git commit follows the
current `gh-pages` head. Files absent from the new release are removed from the
published tree, but remain in Git history.

Repeating `publish` for the current release checks its existing operation rather
than creating another commit or push. A pending or failed Pages build is not a
reason to upload the same commit again. Reconcile it with `publication --refresh`.

After a newer release is uploaded, publishing an older recorded release reports
`conflict`: it does not silently roll the website back. Preserve the old release
and receipt for inspection; do not delete the receipt to force another push.

## State and interrupted operations

Publication records are stored in `.berlin/publications/<release-id>.json`.
They are not best-effort build receipts. Berlin synchronizes intent to disk
before pushing and persists the resulting state, its update time, and the first
time a matching live build was observed:

| State | Meaning |
| --- | --- |
| `prepared` | Exact commit recorded; upload is not confirmed. |
| `unknown` | A push failed or timed out; it may have succeeded remotely. |
| `uploaded` | Commit accepted, but a matching successful Pages build is not yet confirmed. |
| `live` | GitHub reports a successful Pages build for the expected commit, still at the branch head. |
| `failed` | GitHub reports a failed Pages build for that commit. |
| `conflict` | Branch state no longer matches this operation; no automatic overwrite. |

`uploaded` is not a claim that the website is live. `live` reflects GitHub's build
status, not an independent CDN/HTTP content check. `publication` shows recorded
state; `--refresh` performs read-only remote checks and updates the local record.
If the branch confirms the expected commit but the Pages build query fails,
Berlin records `uploaded` and warns that build status is unavailable. A stale
build for another commit cannot establish `live` or `failed` for this release.
If the branch itself cannot be queried, refresh fails without replacing the
previously recorded state. After a successful push, that state is `uploaded`.

After an interrupted push, refresh first. Repeating `publish` reconciles the
recorded commit before sending anything: an already-uploaded commit is not
pushed again. A retry can reconstruct only the same commit against the same
parent. Git author/committer timestamps are fixed for this reconstruction, not
intended to describe the publication time.

Only `prepared` or `unknown` operations whose branch is still at the recorded
parent may retry the exact commit. Once upload is acknowledged or observed,
seeing that old parent again is a `conflict`, not permission to push again.
This applies equally immediately after a push, during a retry, and on refresh.
If the parent changes during recovery preparation, Berlin records the conflict
and does not push the newly prepared commit.

A changed branch is reported rather than automatically rebased or overwritten.
If a Pages build fails, inspect it in GitHub and refresh Berlin's record after
resolving it. There is no automatic deletion, rollback, retry scheduler, or
cross-channel transaction.

Keep `.berlin/releases` and `.berlin/publications` together in backups. Unlike
generated Markdown or latest-build receipts, they are durable release state.
Do not delete the ledger to retry an uncertain operation: that discards its
duplicate-prevention information. Use one authoritative publishing machine or
persisted CI workspace; concurrent independent publishers/state merging are not
supported. Local project locking serializes Berlin operations on that machine.
