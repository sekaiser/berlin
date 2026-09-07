# Berlin architecture

Berlin turns source content into channel-specific artifacts. The current authoring path is:

```text
Emacs Org files -> Ox-Hugo -> Markdown -> Berlin -> website
```

Markdown is an interchange format at Berlin's boundary, not the domain model used by downstream
transformations. Parsing produces a typed `berlin_document::Document` containing semantic blocks
and inlines, content identity, normalized metadata, and source provenance.
Parsing is fallible: malformed front matter, unsupported or unresolved shortcodes, and invalid
semantic invariants stop publication with source context rather than degrading into missing data.

Website article summaries, ordering, tag grouping, per-page context, and slugs project their
metadata from the semantic document. Full note bodies are rendered by the separate
`berlin_document_html` channel renderer; Tera receives only the semantic document and its projected
view data. Its reusable `Renderer` projects documents into typed `Html` and highlights semantic
code blocks directly, without reconstructing or reparsing Markdown. During parsing, Hugo relrefs
become resolved semantic links and Hugo figure shortcodes become typed figures before channel
rendering.

The intended phase boundary is:

```text
Org sources -> Markdown sources -> semantic Documents -> channel artifacts -> publication receipts
```

## Reproducible authoring toolchain

The locked Nix development shell provides the single Rust toolchain, Node, and batch Emacs with
Ox-Hugo. The project adapter fixes Ox-Hugo's boundary format to YAML-front-matter Markdown and
builds an invocation-local Org ID index from the selected sources. Exports therefore do not depend
on a user's Emacs configuration or global `org-id-locations` database.

UnoCSS is a small, independent build-time toolchain pinned by `package-lock.json`. It scans the
Tera templates and generated Markdown and writes a project's local `static/css/uno.css`; Berlin then
copies that artifact like any other static asset.

Run the complete repository acceptance path with:

```console
nix develop --command support/check
```

That command checks Rust tests and lints, the reusable code-copy script, a real Ox-Hugo
export, website rendering (including CSS compilation and asset copying), and the
local LinkedIn projection in a fresh temporary project. It uses only invented fixtures
under `support/fixtures/publishing/`, never the author's personal website. UnoCSS
generation is a separate, optional project-local step.

See [Local publishing](docs/local-publishing.md) for the boundary between the public
tool repository and ignored authoring files.

Channel presentation and remote publication state do not belong in the semantic document. The
document represents publishable meaning; renderers project it into channel-specific artifacts,
while deployment state records what was published where.

Publication and modification metadata use validated calendar dates in exact `YYYY-MM-DD`
form. Timestamps are rejected at the input boundary; time-of-day scheduling requires a separate,
explicit policy. Publication views sort borrowed documents newest first, with missing dates last
and content IDs breaking ties, then retain only document references. Main and tag article listings
share this ordering; draft filtering and display limits remain separate concerns.

The semantic kernel is intentionally small. A document has a kind, metadata, blocks, relations,
and provenance. Built-in blocks cover portable prose; `Section` and `Component` provide named,
stable extension points with component IDs. This permits rules to target semantic regions without
turning the whole model into an untyped property tree. Relations connect documents by stable
content ID rather than embedding copies.

Markdown headings and the content ranges beneath them are normalized into nested `Section`
values. Heading level and stable anchor identity are retained, so the HTML projection is unchanged
while Rhai section functions operate on real author content rather than synthetic fixtures.

`berlin_content::Collection<T>` is the typed collection boundary. Document collections validate
unique content IDs and derive immutable views such as draft filtering, publication ordering, and
tag indexes containing `DocumentRef` values. The runtime artifact store now carries a validated
`DocumentCollection`. The website renderer consumes that collection through `WebsiteAssembly`
without converting documents back into parser-era source objects.

External feed data follows the same staged rule without pretending to be a document:
`DataSources -> Feed`. A `Feed` is a typed collection of `FeedItem` values with stable URL-based
identity and source provenance. Combined tag indexes contain separate document and feed-item
references, preserving their semantic distinction.
Malformed CSV records and invalid URLs fail the pipeline; feed parsing never silently drops rows.

A website is likewise not a document. `WebsiteAssembly` owns the document and feed collections
and derives publication ordering and a combined tag index. The website renderer accepts only this
assembly, making site composition an explicit typed phase instead of hidden renderer behavior.

## Typed pipeline core

Pipeline operations and artifact kinds live in Rust, while `berlin.pipeline.rhai` constructs the
production `berlin_core::PipelinePlan`. The script can only compose Rust-native typed artifact
handles. Berlin validates the complete graph before permitting effects, then checks each runtime
artifact against the kind promised by its operation.
Each Rust operation has one authoritative signature describing its exact ordered inputs, output
kind, whether it produces a runtime value, and whether it writes an output. Validation and runtime
dispatch consume that signature, so effect-only outputs cannot be used as in-memory dependencies.

The scripting experiment must preserve that property. A script is a graph-construction frontend,
not a second executor. It must produce the same typed `PipelinePlan`; filesystem, process,
network, rendering, and publication effects remain Rust capabilities.

Runtime execution uses the same graph boundary. The site pipeline materializes
`MarkdownSources`, transforms them into semantic `Documents`, loads `DataSources`, and passes those
artifacts through a `WebsiteAssembly`. CSS roots and static assets likewise flow from their load
nodes into their effectful nodes, whose output paths come from the plan.

A private `NodeExecutor` dispatches operations to focused task methods. Inputs are resolved by
their declared dependency position and checked against the operation signature; runtime lookup
does not search for a matching artifact elsewhere in the dependency list. Pipeline orchestration
retains ownership of the artifact store, output transaction, and receipt lifecycle.

`PipelineRun` coordinates preparation, node execution, commit, and receipt writing. It retains
partial artifacts and diagnostics for failure receipts and holds the project lock through receipt
writing and transaction cleanup. Dry runs create no transaction or receipt; receipt errors are
warnings and never replace the execution outcome.

Loaded text is represented by a small immutable source-file value containing its path, URI, and
text. Markdown and CSV parse directly into their typed domain collections, while CSS compiles from
its root path. There is no parser cache: watch mode rebuilds the complete graph, so retaining
parser-era source wrappers and invalidation state would add machinery without avoiding work.

## Rhai pipeline and transformations

Rhai embeds directly in Rust, accepts Rust-native value types, and supports the compact
`pipeline name { ... }` syntax. It is a graph-construction and pure-transformation frontend, not a
second effect executor:

- expose typed Rust artifact handles and operation constructors;
- reject invalid operation composition during script evaluation;
- retain named mapping functions as typed `FunctionRef` values in the plan;
- map owned document values and nested sections through a narrow API;
- keep document identity and provenance immutable from scripts;
- bound script operations, recursion, arrays, and strings, and expose no I/O capability;
- reject source and output paths that escape the project root;
- keep filesystem, process, rendering, and publication effects in Rust.

`berlin.pipeline.rhai` is the single source of truth for named pipelines. `bln plan` displays the
same scripted graph that `bln build` executes; both commands accept any pipeline name defined by
the script.

The first non-website projection is deliberately local: semantic documents render to typed
`LinkedInPostDraft` values and materialize as reviewable text files plus a JSON manifest. Draft
documents are excluded, and overlong feed posts carry a typed diagnostic instead of being silently
truncated. There is no network publication step. A future publisher can consume these artifacts
without coupling LinkedIn credentials or remote state to document transformation.

`berlin_document_linkedin::Renderer` exposes single-document rendering and collection rendering.
Collection rendering filters drafts; explicit single-document rendering permits previewing them.
The renderer owns diagnostic policy, while private text helpers project blocks and inlines.
Serializable draft types and public API behavior tests are kept in separate modules.

Website and LinkedIn outputs are separate named pipelines. `bln build` builds the site, while
`bln build --pipeline linkedin` writes private review artifacts under the ignored `_berlin`
directory; they are never placed under the served `_site` tree. A shared Rhai helper defines the
common Markdown parsing subgraph without weakening artifact types. Document mapping remains an
opt-in operation; the default pipelines do not execute an identity transformation.

Watch mode deliberately performs a complete rebuild. A single file-event loop watches the project
inputs, debounces changes, and reruns the site pipeline while the HTTP server remains active.
Pipeline loading and parsing already operate over the complete input set, and a full rebuild
correctly handles additions, deletions, and changes to derived indexes without a second layer of
cache invalidation, task-specific dependency tracking, or a separate glob model.

Non-dry-run pipelines materialize all declared outputs below a project-local staging root. Nested
outputs such as website pages, compiled CSS, and static assets share the outer website root. Only a
successful graph execution replaces the previously published local tree, so failed builds preserve
the last good output and removed inputs cannot leave stale pages or assets behind.
One project lock serializes builds. A pipeline must have a single outer owned output root. The final
backup/install is a recoverable two-rename swap, not a zero-downtime reader-atomic deployment: a
durable transaction journal records that narrow window, and the next build restores the previous
output or accepts the completed replacement after an interrupted process. Remote deployment will
need a target-native atomic pointer or release mechanism if uninterrupted readers are required.

Every non-dry-run pipeline also writes `_berlin/receipts/<pipeline>.json`. A receipt records the
Berlin version, status and duration, project-relative inputs and outputs with SHA-256 hashes,
portable content IDs where available, and channel diagnostics. Failed executions write an empty
output list and the error while the output transaction preserves the last successful artifacts.
Receipts are local operational state, not semantic document data or declared publication outputs.
Receipt errors are reported as warnings after a successful publication and cannot retroactively
turn an installed output into a failed build. Setup failures receive a reduced receipt containing
the pipeline source when it is available.

Website routes are derived through portable slugs, checked for collisions, and confined again at
the final write boundary. Tera escapes context data by default; the semantic document renderer is
the narrow explicit trusted-HTML boundary.

Website presentation is configured in `berlin.pipeline.rhai` using
`website_config(#{ title: "My site", url: "https://example.com", profiles: #{ github:
"https://github.com/example" } })`, passed as the third argument to `render_website`.
The map is converted to a strict typed `WebsiteConfig` (unknown fields, wrong types and
invalid URLs are rejected) and stored on the website render operation. Assembly remains
channel-neutral. Each website output can have its own presentation settings, visible in
`bln plan --json`; receipts track the pipeline source rather than a separate settings file.

Migration: move `[site]` fields from the former `berlin.toml` into the `website_config` map,
with `[profiles]` as its nested `profiles` map. The separate TOML file, ancestor discovery,
and `build`/`serve --config` flag are no longer used. Even websites using defaults explicitly
pass `website_config(#{})`.

## Evidence from the first vertical slice

LinkedIn projection carries diagnostics when an article exceeds the feed-post limit.
Channel projection needs an explicit editorial selection/composition policy; silently
truncating a general document would lose meaning. Future language primitives should be
driven by authored mappings over semantic sections, not by expanding `Document` into a
universal property graph.

Ox-Hugo carries each source file's Org `:ID:` into Markdown front matter. Markdown parsing retains a
source-URI fallback so isolated parsing remains useful, but validated document collections reject
that fallback: every publishable document must have an explicit, portable content ID.
