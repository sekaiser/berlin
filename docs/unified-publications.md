# Unified publications

Berlin loads local documents and authored references into one immutable catalog.
A composer selects resources, adds editorial structure, and returns either one
publication or an explicit set of site pages. HTML and text consume that same
composition. Builds do not fetch referenced resources or publish remotely.

## Run an example

From the repository root:

```sh
cargo build --bin bln
project=$(mktemp -d)
cp -R support/fixtures/unified-resources/. "$project/"
export BERLIN_DIR="$project"
export BERLIN_THEME_DIR="$PWD/themes"
target/debug/bln check --pipeline portfolio
target/debug/bln build --pipeline portfolio
target/debug/bln build --pipeline text
target/debug/bln build --pipeline excerpts
target/debug/bln build --pipeline excerpt_text
target/debug/bln build --pipeline site
```

See the fixture's `berlin.pipeline.rhai` for the exact output directories and
composers. `support/check-unified-resources` runs the examples, checks deterministic
rebuilds and failures, and seals synthetic local releases. It needs no credentials.
For a real Org export, use `support/fixtures/publishing` and run its `org` pipeline
before `site` or `linkedin`.

## Author external resources

`resources(load_data("data/resources.yaml"))` is a typed input. It accepts this
versioned YAML schema, with strict unknown-field and duplicate-key rejection:

```yaml
schema_version: 1
resources:
  - id: queue-paper
    kind: paper
    url: https://example.test/papers/queues
    title: A paper about queues
    authors: [Example Author]
    published: '2026-01-10'
    added: '2026-02-03'
    topics: [queues, systems]
    description: A short plain-text description.
```

Required fields are `id`, `kind`, `url`, and nonempty `title`. Optional fields
include authors, publisher, published/added dates, topics, description, metadata_note,
license_note, attribution, language, and a preview with an HTTP(S) `url` and meaningful `alt`. Dates use `YYYY-MM-DD`.
Kinds include article, note, guide, repository, paper, documentation, video,
website, social_post, and explicitly namespaced custom kinds. Kind is independent
of origin: a local paper and an external paper can share a kind.

CSV inputs use the validated `.csv` extension and the same record schema. Their
list-valued columns use JSON arrays, not delimiter guessing. YAML aliases, tags,
multiple documents, invalid URLs, ambiguous records, and duplicate identities
fail. Duplicate URLs with distinct IDs produce a warning. Relative or local
network URLs are not external resource destinations. URLs are validated offline;
Berlin does not inspect, download, authenticate to, or cache their targets.

Curator annotations belong to component placements, rather than automatically
following every use of a catalog resource.

Resource IDs are `local:<content-id>` or `external:<resource-id>`. Raw IDs are
case-sensitive. Local documents retain their original stable content IDs.
External metadata never acquires a fabricated document body or excerpt capability.

## Construct the pipeline

```rhai
fn chosen(c) {
    let selected=c.select(#{kinds_any:["paper"],topics_all:["systems"],order:"added_desc",limit:20});
    publication("papers",#{title:"Papers worth returning to"},[
        reading_list("Selected reading",selected.map_cards())
    ])
}
pipeline reading {
    let sources=catalog([
        documents(load_markdown("content/notes/*.md")),
        resources(load_data("data/resources.yaml"))
    ],publication_options(#{source_site_url:"https://example.test/notebook"}));
    output render_html(compose(sources,Fn("chosen")),"_reading",html_config(#{
        theme:"@notebook",url:"https://example.test/reading"
    }));
}
```

Graph construction does not have loaded documents. A named, uncurried composer
runs only after all catalog dependencies are prepared and validated. Its catalog
is read-only. Rust and Rhai use the same resource queries, component schemas,
reference checks, and budgets. Shared Rhai helpers must enter through explicit
`--pipeline-file` arguments. Ambient `import`, `eval`, clock, process, and filesystem
access are unavailable.

`documents(load_markdown(...))` parses Markdown; `documents(load_data(...))`
accepts prepared semantic JSON. `compile_papers(...)` directly supplies a typed
document batch. Multiple batches can enter one catalog without first merging
their bodies into another document model.

## Select and compose

Queries can filter by tagged IDs, origin, kind, topic, author, source namespace,
date, and capability. Different fields are intersected; values within an `any`
filter are alternatives. `topics_all` requires every listed tag. Explicit empty
`any` filters and duplicate terms fail. Missing dates sort last, and resource IDs
break ties. `order:"curated"` preserves the explicit ID order. A selection records
its normalized query digest, selected IDs and revisions, and selection digest.

Local drafts are excluded before catalog indexing. Source-link validation uses
the complete eligible local catalog, independently of a portfolio subset.
`source_site_url` states where original local work is available; `html_config.url`
states where the new publication will live. Neither is inferred from local paths.

Use resource cards, featured work, citations, related work, reading lists and
resource groups for metadata presentations. Introductions, sections and callouts
add editorial content. `section_ref(c,id,stable_section_id)` selects an explicit
structural section. `document_excerpt(c,id)` selects the complete semantic body.
Excerpts retain attribution and source revisions; repeated placements have their
own anchors, footnotes and listing references. They resolve outside links against
the original source, including work not selected for the portfolio.

See [component contracts](component-contracts.md), [Rust `view!`](view-macro.md),
and [Tera slots](tera-components.md). Portable excerpts reject raw HTML and
unregistered source components instead of silently dropping them.

## Compose a notebook

Return `site(id, metadata)` from the composer and add only the entries needed:

- `add_page(key, route, publication)` for home, notes, reading, about, or another composition.
- `add_document_pages(selection, prefix)` for complete local articles and papers.
- `add_tag_pages(selection)` for mixed-resource tag collections.
- `add_search(selection)` for local semantic text and external public metadata.
- `add_redirects(selection)` for explicit previous-slug redirects.

Routes share one collision registry, including case-folded collisions. Source
scope remains distinct from selected generated pages. The maintained notebook
fixture shows a complete composer with navigation, metadata, comments opt-in,
backlinks, listings, attachments, tags and search. Reading-list components retain
local search and multi-tag intersection with progressive enhancement; without
JavaScript all references remain available.

## Presentation, outputs and evidence

`render_html` handles both a single publication and a multipage site. Effective
project files override theme shared files and defaults. Only bound CSS entries
are compiled from captured imports; `styles/publications/` takes precedence over
`styles/entries/`. Effective static files are copied once. Additional assets use
`.with_assets(load_assets("attachments/**/*"),"attachments")`.

A renderer owns its complete output directory. Independent copy/CSS outputs
cannot overwrite it. Declared upstream paper or diagram producers may occupy
confined, disjoint subdirectories; conflicting paths fail before installation.
All files share the existing output transaction. Failures preserve installed
output. Releases use exact staged bundles; imported directories do not acquire
invented build provenance. Historical sealed bundles are never rewritten.

`render_text(composed,output,text_config(#{profile:"plain"}))` directly interprets
one publication without Tera or HTML stripping. The `linkedin_draft` profile
preserves over-limit text and emits diagnostics. Multipage-to-text flattening is
rejected; choose an explicit single composition for that channel.

`check` and `build --dry-run` prepare permitted inputs and render in memory,
including selected CSS validation, without installing outputs or writing receipts.
They do not run missing Org or TeX preparation as a substitute for source inputs.
`serve --pipeline NAME --watch` watches declared sources, resource files,
pipeline helpers, presentation files and overrides while ignoring output/state.

Public manifests record input/query/source/placement evidence and separate
composition and presentation fingerprints. Final inventories hash every output
except the inventory's own manifest, avoiding a self-hash. A sealed release also
binds that manifest. Operational receipts retain local input locations; public
manifests use logical names and omit local paths, bodies and credentials.

Budgets are finite and downward-only: catalog items/descriptor bytes, filters,
query work, component occurrences/depth, strings/properties, slots, fragments,
excerpts and output bytes. Rhai additionally limits instructions and native graph
materialization. Configuration handles share immutable payloads; native graph
copies are charged before allocation against a cumulative 64 MiB encoded-byte
budget. These are workload limits, not a promise that process RSS equals them.
