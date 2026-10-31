# Composable publications

A publication selects existing work and adds editorial structure around it.
Source documents remain the source of truth. A portfolio can show metadata cards,
curated annotations and selected sections without copying articles into another
authoring model. Rust and Rhai create the same channel-neutral representation.

See [the unified catalog and site guide](unified-publications.md) for external
resources and multipage composition.

## Run the synthetic examples

From the repository root:

```sh
cargo build --bin bln
publication_project=$(mktemp -d)
cp -R support/fixtures/composable-publications/. "$publication_project/"
export BERLIN_DIR="$publication_project"
export BERLIN_THEME_DIR="$PWD/themes"
target/debug/bln plan --pipeline portfolio
target/debug/bln check --pipeline portfolio
target/debug/bln build --pipeline portfolio --dry-run
target/debug/bln build --pipeline portfolio
target/debug/bln build --pipeline text
target/debug/bln build --pipeline curated
target/debug/bln build --pipeline excerpts
target/debug/bln build --pipeline excerpt_text
target/debug/bln serve --pipeline portfolio --watch --port 8082
```

`_portfolio/index.html` is the query-selected portfolio; `_text/publication.txt`
is its independent text interpretation. `_excerpts` reuses one source section in
two placements with separate anchors, code references and footnotes.
`_excerpt_text` uses LinkedIn draft diagnostics without sending anything remotely.
The fixture has twelve synthetic documents, including a draft and unselected
sentinels. No personal content or credentials are needed.

## Define a composer

```rhai
fn selected_work(catalog) {
    let selected = catalog.select(#{
        topics_all: ["storage"],
        order: "published_desc",
        limit: 8
    });
    let entries = [];
    for source in selected {
        entries.push(view_component("berlin.featured_work@1", #{source: source}, #{}));
    }
    publication(#{id: "storage", title: "Selected Storage Work"},
        view_component("berlin.portfolio@1", #{}, #{body: entries}))
}

pipeline portfolio {
    let sources = catalog([documents(load_markdown("content/*.md"))],
        publication_options(#{source_site_url: "https://example.test/notebook"}));
    let composed = compose(sources, Fn("selected_work"));
    output render_html(composed, "_portfolio",
        html_config(#{theme: "@notebook", url: "https://example.test/portfolio"}));
}
```

The named, uncurried one-argument composer runs after its document dependencies,
including mappings. Graph declarations do not have a loaded catalog. `plan`
checks the function reference and graph without invoking the composer or reading
source content. The runtime catalog is read-only and indexed once; references do
not contain copied source bodies.
Implicit Rhai `import` is disabled: it could read helper files outside the
captured program inputs. Select shared helper code through repeatable
`--pipeline-file` arguments instead; those files are compiled together and
captured in receipts and render fingerprints.

Use a second pipeline and `render_text(composed, "_text",
text_config(#{profile: "plain"}))` for text. Separate output roots belong to
separate runs; a transaction still owns one output root. Composed renderers own
their entire output directory, including assets and manifests. Do not place
another independent output operation inside that directory. Declared upstream paper and diagram producers may occupy disjoint subdirectories; collisions fail before installation.

## Select and reference work

`catalog.select` uses the shared `ResourceQuery`: optional tagged `ids`,
`origins_any`, `kinds_any`, `topics_any`, plus `topics_all`, `topics_none`,
`author_text`, `source_namespace`, `capabilities_all`, inclusive published/added
date bounds, `order` and `limit`. Different fields are ANDed; each `any` field
uses OR. Explicit empty `any` filters and duplicate terms fail. Tag keys use ASCII
case-folding for ASCII tags and exact Unicode comparison otherwise. Orders are
`published_desc`, `added_desc`, `title_asc`, `id_asc` and `curated`. Missing dates
sort last; tagged resource IDs break ties. Limits apply after filtering/sorting.
An empty selection is valid; a required nonempty receiving slot may still fail.

For a curated list use `#{ids: ["local:queues", "local:storage"], order: "curated"}`.
Written order is preserved; missing, duplicate or draft IDs fail. Query selection
always excludes drafts. `resource_ref(catalog, "local:storage")` creates a
reference; `pinned_resource_ref(catalog, "local:storage", #{record_revision: "..."})`
checks its normalized revision. `external:` references use the same query and
lookup contract but do not contain semantic document bodies. Selection values
expose `query_digest` and `selection_digest`; `catalog.describe(ref)` exposes
only public metadata. Digests detect mismatches; they do not retrieve history.

`section_ref(catalog, "local:storage", "handoff")` selects a structural section by its
explicit stable ID. A selected section includes nested content and required
footnotes. Missing dependencies, raw HTML and unregistered semantic components
fail rather than silently losing content. Local anchors, footnotes and code-line
references are namespaced per occurrence. Links outside the excerpt lead to the
original article; links to other documents resolve against the full catalog,
including unselected original work. Assets resolve at the original source site.
This does not copy the source website or its attachments into the portfolio.

The source-site URL describes where original work is published. The website
configuration URL describes the new publication. These can use different hosts
and mount prefixes. Source titles must be nonempty; original URLs are explicit
or derived through Berlin's existing article-route rules.

## Presentation and lifecycle

The [component contracts](component-contracts.md) support HTML and text.
[Rust builders and `view!`](view-macro.md) offer typed composition. Rhai shares
their schema validation and budgets. [Tera presentations](tera-components.md)
remain replaceable local theme files. Publication metadata controls the page
title; `WebsiteConfig.title` is site identity and cannot overwrite it.

`check` validates permitted source preparation, composition, references, channel
support and an in-memory render. It compiles the required CSS import closure from captured bytes in memory.
`build --dry-run` also avoids outputs, receipts and temporary theme workspaces.
Neither command exports Org or imports papers as a substitute for missing
generated source inputs. Run those preparation pipelines explicitly first.

`serve --pipeline NAME` serves the declared website directory. Text-only and
ambiguous targets fail. Watch mode observes source/script/template/style/asset
changes and new overrides, keeps the previous successful output on failure, and
excludes generated output and receipts.

Builds use existing confinement, locks, staging and recoverable installation.
This is not a promise of an uninterrupted atomic switch for concurrent readers.
HTML outputs can use existing offline `release` and verification. Their
`publication.json` is bound through the exact release bundle; a multipage site records only its generated local document routes in the release document inventory. Single portfolios do not claim original-work routes as generated pages. Imported directory
releases retain byte-bundle semantics without inferred build provenance. Remote
publication still requires Berlin's separate explicit release confirmation flow.

## Reproducibility and limits

HTML writes `index.html`, theme assets and `publication.json`; text writes
`publication.txt` and `manifest.json`. Composition fingerprints identify the
normalized selection/editorial structure and source revisions. Render-input
fingerprints additionally include channel/configuration and consumed templates,
styles, assets and pipeline bytes. Theme changes do not change the composition
fingerprint. Public manifests contain logical names and hashes, not local paths,
source bodies or credentials. Operational receipts retain local input locations.

Defaults are supported maxima: 10,000 component occurrences, depth 32, 1,000
selected sources per query, 1,000,000 UTF-8 bytes per property string, 8 MiB total
property bytes, 16 slots per component, 16 MiB rendered output, 64 MiB retained
slot fragments and 10,000 excerpt dependency nodes. `publication_options` accepts
a `limits` map that can lower these values. Native Rhai fragments share state
until bounded lowering; their cumulative construction also has a node/property
budget and a bounded edge count. These are payload limits, not a process-RSS
guarantee. Parsed source catalogs and compiler/runtime overhead consume memory
outside them. No implicit wall-clock values, randomness or environment reads are
available inside the new template renderer. PDF and slide engines are outside
this feature.
