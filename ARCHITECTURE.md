# Berlin architecture

Berlin executes local publishing pipelines defined in `berlin.pipeline.rhai` by
default. Repeatable `--pipeline-file` arguments select an ordered set of files
instead; the CLI compiles them as one Rhai program. Project context carries this
selection through execution, source-root resolution, authoring checks, receipts,
and preview watching. Script selection does not change the project root.
It produces static websites and LinkedIn draft files. Explicit release/publish
commands can deploy a sealed website to an existing GitHub Pages publishing
branch. Social-platform API publishing is not implemented.

## Modules

| Location | Responsibility |
| --- | --- |
| [bln-document](crates/bln-document/) | Document types, metadata, provenance, and semantic validation. |
| [markdown](crates/markdown/) | Markdown parsing, supported Hugo shortcodes, and HTML rendering of Markdown. |
| [bln-content](crates/bln-content/) | Validated document/feed collections, publication ordering, and tag indexes. |
| [bln-core](crates/bln-core/) | Pipeline nodes, operation signatures, graph validation, and website settings. |
| [pipeline-dsl](crates/pipeline-dsl/) | Rhai pipeline construction and document-mapping API. |
| [bln-document-html](crates/bln-document-html/) | Semantic documents rendered as HTML, including highlighted code listings. |
| [bln-document-linkedin](crates/bln-document-linkedin/) | Semantic documents rendered as text drafts with channel diagnostics. |
| [cli](cli/) | Project loading, operation execution, templates, output transactions, receipts, and preview server. |

## Content flow

```text
Org ──ox-hugo──> Markdown ──parser──> DocumentCollection ──> LinkedIn drafts
                                           │
CSV ──feed parser──> Feed ─────────────────┴──> WebsiteAssembly ──> HTML pages
```

Org export is optional. The batch exporter in
[support/ox-hugo/export.el](support/ox-hugo/export.el) uses Emacs with ox-hugo,
writes YAML-front-matter Markdown, and carries the file-level Org `:ID:` into
front matter. It builds an Org ID index from the selected sources and disables
Babel processing, so publishing does not execute source examples.

Markdown is an interchange format. Downstream operations use `Document`, which
contains an ID, kind, metadata, blocks, relations, and provenance. Headings and
their content become nested `Section` blocks; `Component` blocks carry named
properties and child blocks. Supported Hugo figure and relative-reference
shortcodes are resolved during parsing.

Parsing and collection validation are fallible. Publishable collections require
explicit, unique content IDs; the parser's source-URI fallback is insufficient.
Publication and modification dates use `YYYY-MM-DD`, not timestamps.

`WebsiteAssembly` excludes drafts and combines the published documents and feed
with publication ordering, a shared tag index, and contextual backlinks derived
from authored document links. IDs resolve to website URLs only during rendering;
references to documents outside the publication fail assembly. See
[Document references](docs/document-references.md). Publication ordering is newest first, with
missing dates last and content IDs breaking ties. Feed items have URL-based IDs
and remain separate from documents. Invalid CSV records and URLs fail parsing.

## Pipeline construction and execution

The Rhai loader evaluates pipeline declarations into Rust `PipelinePlan` values.
Each operation declares its ordered input types, output type, whether it produces
a runtime value, and whether it writes files. Validation checks dependencies,
type compatibility, cycles, and source/output path constraints.

The Rhai program may declare `SOURCE_ROOTS`, a map of names to directories.
Inputs such as `@data/notes/*.org` resolve against those explicitly configured
roots; matched files are canonicalized and rejected if symlinks escape the root.
Unprefixed inputs and all output destinations retain project-relative semantics.
Org sidecars record named source references and revalidate them against the
current root declarations when resolving authoring navigation. Preview watching
includes the declared source roots without watching generated receipts.

Rhai receives typed artifact handles and a limited document API. Document mappers
can edit exposed metadata and map sections, but cannot change document identity
or provenance. Mapped collections are validated before use. The engine limits
operations, call depth, expression depth, array sizes, and string sizes; direct
I/O functions are not exposed by Berlin.

[PipelineRun](cli/tasks/run.rs) loads the selected plan, executes nodes in
topological order, commits outputs, and records a receipt.
[NodeExecutor](cli/tasks/executor.rs) resolves inputs by dependency position and
dispatches operations. Produced runtime values are checked against operation
signatures and stored by node ID.

`bln plan` displays a graph without executing it. `bln build --dry-run` still
loads and processes inputs where supported, but skips output writes and Org
export. It creates neither a transaction nor a receipt. Separate pipeline runs
do not share a document cache.

## Rendering

The HTML renderer converts semantic blocks directly to HTML. Code blocks use
Syntect highlighting; listing IDs, line references, captions, and highlights are
described in [Code listings](docs/code-listings.md).

The website task supplies projected metadata and rendered document bodies to
Tera templates in the project's `pages/` directory. Template names and page
families are fixed by [the website task](cli/tasks/website.rs). Routes are derived
from titles and tags, checked for collisions, and validated at the write boundary.
Website settings come from `website_config(...)`, passed to `render_website`.
Articles can opt into discussions through boolean `comments` metadata. Website
configuration and the stable-ID template contract are described in
[Article discussions](docs/article-comments.md); themes control the actual embed.

Tera escapes context values. Rendered document HTML is marked safe, and raw HTML
in documents is preserved. Content and templates must therefore be trusted.

The LinkedIn renderer produces text drafts with character-limit diagnostics;
the CLI writes the draft files and a JSON manifest. Collection rendering excludes
draft documents and reports overlong posts without truncating them. Rendering a
single document permits draft previews.

CSS compilation and static asset copying are available as separate pipeline operations.
The CSS operation bundles imports and minifies authored styles through Lightning
CSS. It requires no separate Node-based CSS generator.

The local [notebook theme](themes/notebook/README.md) supplies templates,
design tokens, component styles and progressive client controls for these page
families. `website_config.theme` selects a local directory at rendering time;
content assembly remains independent of presentation. Project files override
theme shared files and defaults. The themed render operation bundles the effective
CSS entries from `styles/entries/*.css` into `assets/css/` and stages static
assets alongside pages, without modifying sources. Supporting styles remain
source-only; templates select which compiled entries each page loads.
Effective theme inputs are recorded in receipts and watched by the preview server.
No registry or download is involved. The synthetic theme specimen is separate from the
minimal publishing fixture and the ignored personal website.

## Output transactions and receipts

A project lock serializes non-dry-run builds. Outputs are written to a staging
directory before installation. Declared output paths must have at most one
outermost root: nested outputs are supported, but unrelated sibling roots are
rejected. Berlin replaces the owned root as a whole, so it must not contain
unmanaged files.

Installation backs up the existing destination and renames the staged output
into place. A journal supports recovery on the next build after interruption.
This is a recoverable two-rename operation, not an uninterrupted atomic switch
for concurrent readers. If installation and restoration both fail, backup and
staging directories are retained for recovery.

Persisted Org intermediates use an explicitly declared workspace such as
`.berlin/generated/org/`. `export_org` receives the workspace and Hugo section
separately, and commits Markdown (`content/<section>/`) and copied assets
(`static/attachments/<content-hash>/<filename>`) together. Attachment links retain
those public `/attachments/...` paths through Markdown and HTML; captioned figures
do not add a presentation-specific prefix. Referenced local files are copied
regardless of extension; missing files fail export, and remote URLs are not fetched.
Consumer pipelines select those paths explicitly; no
implicit producer scheduling occurs. Missing generated source matches fail
with a request to build the producer. Org must be re-exported after source edits.
Typed documents and other runtime values remain in memory.

Receipts are written on a best-effort basis to
`.berlin/receipts/<pipeline>.json`. They record the Berlin version, status,
duration, input/output hashes, content IDs where available, and diagnostics.
Failed executions record the error and an empty output list; setup failures use
a reduced receipt. Receipt-writing errors are logged without replacing the
build result. Receipts describe local execution, not remote deployment state.

## Website releases

Website releases are built in staging and copied into content-addressed bundles
under `.berlin/releases/<id>`, without replacing preview output. A manifest binds
the complete file inventory, public URL, destination, pipeline and Berlin version.
Deployment targets are typed metadata on website output nodes, not build effects.
Publishing verifies a sealed bundle rather than rebuilding or evaluating Rhai.
`release --from-directory` accepts a separately prepared website using the
selected pipeline's declared destination, without executing build operations.
Both paths copy files before adding support assets; the supplied directory stays
unchanged. Imported releases do not attest to the build or external checks that
produced their files.

The GitHub Pages adapter publishes only to a dedicated `gh-pages` branch and
requires matching, pre-existing Pages configuration. Intent and outcomes are
synchronously persisted per release under `.berlin/publications`, independently
of best-effort build receipts. Remote branch leases and reconciliation protect
retries; uploaded and confirmed-live states are distinct. One authoritative
publisher is supported. See [Website releases](docs/website-releases.md) for the
validation boundary, credential setup, and recovery behavior.

## Preview and checks

Website themes may opt into publication search with `search.tera`. The website
renderer derives a section-level text index from the assembled public documents
and writes it alongside a dependency-free browser client. This is an output
projection, not an authoring database: drafts, provenance and raw HTML are omitted.
See [publication search](docs/notebook-search.md) for the template contract.

Article metadata can pin a public `slug` independently of title and source path.
The website task uses one route function for pages, document links, backlinks and
search, and checks `previous_slugs` against the same route registry. Previous
slugs emit static redirect pages, not additional semantic documents. See
[stable article URLs](docs/stable-urls.md) for authoring and hosting behavior.

`bln check` prepares only the document branch of one website output, using a
read-only operation allowlist and the existing node executor. The content
crate's reference analysis collects unresolved links while retaining valid
backlinks; the authoring report derives connection and direct-guide-coverage
observations from that same index. Build assembly still rejects unresolved
references. The CLI presents typed findings as text or versioned JSON without
creating output transactions or receipts. See
[Local authoring check](docs/authoring-check.md) for scope and source-location
limitations.

The Org adapter records heading identities beside exact exported Markdown hashes
in local `.berlin/org-origins/` sidecars. The CLI enriches authoring findings from
these maps without adding editor paths to semantic documents or published pages.
The optional Emacs adapter consumes the JSON report and follows unique Org
headings, with file-only fallback for stale sources or ambiguous locations.

`bln serve` runs the `site` pipeline and serves `_site` on localhost.
Watch mode debounces project changes and rebuilds the entire site pipeline.
It does not run the separate Org export pipeline.

The Nix development environment provides Rust, Node.js, and Emacs with ox-hugo.
`nix develop --command support/check` runs tests, Clippy, and publishing checks
against a temporary project copied from
[support/fixtures/publishing](support/fixtures/publishing/).

See the [README](README.md) for setup and
[Local publishing](docs/local-publishing.md) for project layout.
