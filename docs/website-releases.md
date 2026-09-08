# Website releases and GitHub Pages

Berlin can seal a website build and explicitly publish that exact bundle to an
existing GitHub.com Pages site. This adapter uses a dedicated `gh-pages` branch,
not a custom Actions artifact workflow. It does not configure the repository,
change its default branch, or run on `build`, `plan`, `check`, or `serve`.

## Declare a destination

Attach a destination to the existing website output in your Rhai pipeline:

```rhai
output render_website(website, "_site", presentation)
    .deploy_to(github_pages("owner/notebook"));
```

`presentation` must declare the actual public HTTPS URL in `website_config`.
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
destination, and sorted file inventory with byte sizes and SHA-256 hashes. The
release ID hashes this manifest. Files are copied, not hard-linked to mutable
build output. Identical results reuse the same ID; altered files cannot be used
under the old ID. This detects modification; it is not a cryptographic signature
or a guarantee against someone who controls the machine.

Berlin requires a nonempty `index.html`, rejects symlinks/special files and Git
control paths, and checks for common preview-only HTML references. It adds
`.nojekyll`, Berlin's licence/notice files under `licenses/berlin`, and a `CNAME`
for custom domains. Conflicting supplied notice/CNAME files fail preparation.
Normal authored downloads, including CSV files, remain allowed.

These checks do **not** constitute a complete link, accessibility, privacy, or
secret audit. Berlin does not rewrite URLs, fingerprint assets, generate a
sitemap, or migrate site-specific production helpers in this step. Templates,
CSS, authored links and attachments must already work under the configured URL
prefix. Inspect the sealed `site/` directory and run your site's validation on
that exact bundle before authorizing publication; validators must not alter it.

`release-plan` rechecks the complete inventory and reports the exact destination
and files. It does not reload the pipeline or contact GitHub. Subsequent edits
and builds cannot change an existing release.

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
supplied directory. Conflicting support files and unsafe paths still fail. A
normal build and an imported directory with identical final bytes and release
metadata produce the same ID. The manifest binds the resulting files and the
declared destination; it is not proof that imported files were produced by that
pipeline or passed external checks. Ensure their URLs match the declaration,
then review the final sealed bundle, including any added support files.

There are no arbitrary command hooks in the pipeline or publisher. This is a
handoff for a finished website, not permission to mutate a sealed release.

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

The adapter uses GitHub's [branch publishing workflow](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site)
and [Pages configuration/build APIs](https://docs.github.com/en/rest/pages/pages).
A push made with the Actions `GITHUB_TOKEN` does not trigger a branch-based Pages
build; do not use that token for this adapter. Custom Actions artifact deployment
is a separate integration and is not implemented here.

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

After an interrupted push, refresh first. Repeating `publish` reconciles the
recorded commit before sending anything: an already-uploaded commit is not
pushed again. A retry can reconstruct only the same commit against the same
parent. Git author/committer timestamps are fixed for this reconstruction, not
intended to describe the publication time.

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
