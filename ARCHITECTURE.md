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
| [bln-math](crates/bln-math/) | Bounded, offline TeX-equation rendering to native MathML and its structural stylesheet. |
| [bln-paper](crates/bln-paper/) | Pure, constrained Pandoc/BibTeX projection into semantic documents, retaining compiled labels and bibliography text. |
| [bln-content](crates/bln-content/) | Validated document/feed collections, publication ordering, and tag indexes. |
| [bln-publication](crates/bln-publication/) | Immutable source catalog, selection, component contracts, structural excerpts, occurrence identity and publication manifests. |
| [bln-publication-macros](crates/bln-publication-macros/) | Syntax-only component derive, typestate builders and `view!` expansion. |
| [bln-publication-html](crates/bln-publication-html/) | Session-local Tera composition with explicit, checked slot ownership. |
| [bln-publication-text](crates/bln-publication-text/) | Independent plain/LinkedIn-draft interpretation of publication components. |
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

`PublicationAssembly` is an alternative output path, not a filtered
`WebsiteAssembly`. An immutable catalog retains the full prepared source
collection. Queries/curation select references into it; structural excerpts
retain their source identity while each placement receives an occurrence ID.
Links to unselected original work resolve against the catalog's source-site URL,
independently of the new publication's destination URL.

`compose_publication` invokes a named one-argument Rhai composer after document
mapping. Rust builders and `view!` construct the same normalized specification.
One frozen registry supplies property, slot and channel validation. HTML uses
ordinary theme templates and a per-render fragment store;
`render_slot(owner=view.id, name="body")` consumes only renderer-created content.
Direct text rendering walks semantic content without stripping HTML. Public
manifests separate composition identity from presentation inputs and use captured
input bytes. See [Composable publications](docs/composable-publications.md).

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
export. It creates neither a transaction, receipt, nor operation-cache entry.

[ArtifactCache](cli/tasks/cache.rs) stores versioned, content-addressed operation
results under `.berlin/cache/artifacts-v1/`. Consumers define complete input keys
and validate typed results; storage checks integrity and installs entries
atomically. Cache failures fall back to computation. Paper compilation uses
per-source keys and conservative toolchain verification. CSS compilation records
the bundler's actual reads and import resolutions. Dependency hints are verified
against current files before looking up a content-addressed compiled stylesheet.
Theme paths are logical paths within the merged styles directory, independent of
temporary workspace names. Project overrides use that same resolution path.
Cache hits still enter normal output staging and validation. `--no-cache`
bypasses reads and writes. Other pipeline operations still execute each run;
there is no graph-wide incremental scheduler. See [build caching](docs/build-caching.md).

## Rendering

The HTML renderer converts semantic blocks directly to HTML. Code blocks use
Syntect highlighting; listing IDs, line references, captions, and highlights are
described in [Code listings](docs/code-listings.md).

Equations remain `Inline::Math` or `Block::Math` values containing TeX. Both HTML
projections use the same fallible MathML renderer; styles accompany math-containing
fragments, with no browser script or external rendering process. Org export
preserves math before Blackfriday's text escaping; Markdown validation reports
unsupported expressions with source locations. Search and text-only channels
retain the TeX. See [Mathematics](docs/mathematics.md) for supported authoring syntax
and the boundary between equations, diagrams, and complete LaTeX documents.

Org-authored TikZ compilation belongs to explicit authoring. The optional
`support/emacs/berlin-diagrams.el` adapter uses Org for expansion/result insertion
and Berlin's shared graphics service for compilation and SVG conversion. The
standalone `render-diagram` CLI and typed `compile_diagrams` pipeline operation
use the same service, explicit input snapshots and cache. pdfLaTeX and LuaLaTeX
toolchains live in the optional `diagrams`/`papers` shells. Standalone graphics
do not use the semantic paper package whitelist.
Publishing consumes saved SVGs through the ordinary content-addressed attachment
path. Neither the batch exporter nor `berlin-check` loads the authoring adapter.

Paper compilation is pipeline-owned: `load_latex` snapshots sources and their
declared includes, class wrappers, bibliographies and images; `compile_papers` runs the optional `papers` toolchain in
temporary working directories. Build, watch and release require `--allow-tex`.
The pure paper adapter converts Pandoc AST nodes and compiled BibTeX text into
semantic documents, preserving TeX's label numbers. Paper TikZ pictures and PDF
figures become SVG assets for HTML. External commands have deadlines, bounded
captured output and Unix process-group cleanup. `merge_documents` combines
these with other inputs. PDFs, figure assets and HTML share the output transaction;
declared dependency edits are watched without manual import steps. `parse_documents`
also accepts independently prepared semantic JSON.
See [LaTeX papers](docs/papers.md) for the supported profile and trust boundary.

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

Native releases also bind a versioned inventory of the selected renderer's public,
mapped documents. Stable IDs connect revisions across title and route changes;
separate content and metadata hashes exclude provenance and presentation. The
inventory contains no document bodies or source paths and is not published as a
site asset. Native manifests use schema 2; schema-1 bundles and their IDs remain
readable. Imported directories carry no document inventory.

`build --production` and `release --production` finalize the staged website only
after all its output operations complete. This built-in offline step rewrites
local HTML/CSS URLs, versions assets from final dependency bytes, validates
canonicals/local links/redirects, and creates sitemap/robots files. HTML uses
`lol_html` with entity decoding; CSS reuses Lightning CSS. No arbitrary command
hook or additional runtime executable is involved. Release preparation retains
the same run's document inventory and seals only after those checks succeed.
Normal preview builds and prepared-directory imports do not run this step.

`release-diff` verifies two local bundles and compares file inventories, release
metadata, and document revisions where both inventories are available. Missing
document history is reported explicitly, not reconstructed from current inputs.
The command does not load authoring inputs, acquire the recovery lock, or read or
write publication state. Comparisons do not establish what is live remotely or
decide whether an editorial change warrants an announcement.

`release-drafts` consumes the verified comparison to prepare review-only LinkedIn
announcement starting points for added or content-modified documents. It requires
two native inventories for the same website URL and destination, and uses the
shared LinkedIn renderer's diagnostics without fabricating semantic articles.
Plans contain release identities, public titles/URLs, revision hashes, and draft
text, not source bodies. Preview is read-only; explicit export creates a new
directory and never overwrites an existing review. It neither checks live state
nor publishes, schedules, or records social posts. The full-document Rhai
`render_linkedin` operation remains separate.

`announcement review`, `record`, and `status` add a local manual-posting workflow
to those exports. Exact-text approval IDs bind the account, release pair and
document revision. Append-only approvals and user-reported publication records
live under `.berlin/announcements`, using a dedicated lock and no-clobber writes,
not the build/recovery lock. Edited drafts require new approval; repeat reports
are idempotent and conflicting duplicates are refused. History remains readable
without source files or release bundles. No social API or live check is performed;
a reported post is not verified delivery. See the
[announcement workflow](docs/announcement-workflow.md) for the evidence and
duplicate-detection boundaries.

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
