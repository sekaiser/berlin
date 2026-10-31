# Composable publications

A publication selects existing work and adds editorial structure around it.
Source documents remain the source of truth. A portfolio can show metadata cards,
curated annotations and selected sections without copying articles into another
authoring model. Rust and Rhai create the same channel-neutral representation.

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
    let selected = select_work(catalog, #{
        tags_all: ["storage"],
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
    let documents = parse_markdown(load_markdown("content/*.md"));
    let composed = compose_publication(documents, Fn("selected_work"),
        publication_options(#{source_site_url: "https://example.test/notebook"}));
    output render_publication_website(composed, "_portfolio",
        website_config(#{theme: "@notebook", url: "https://example.test/portfolio"}));
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

Use a second pipeline and `render_publication_text(composed, "_text",
text_config(#{profile: "plain"}))` for text. Separate output roots belong to
separate runs; a transaction still owns one output root. Composed renderers own
their entire output directory, including assets and manifests. Do not place
another output operation inside that directory.

## Select and reference work

`select_work` accepts `ids`, `kinds`, `tags_any`, `tags_all`, `published_from`,
`published_to`, `order`, `limit` and `allow_empty`. Dates are inclusive
`YYYY-MM-DD` bounds. Orders are `published_desc`, `published_asc`, `title_asc` and
`curated`. Missing dates sort last; content IDs break ties. Limits apply after
selection and sorting. An empty result fails unless explicitly allowed.

For a curated list use `#{ids: ["queues", "storage"], order: "curated"}`.
Written order is preserved; missing, duplicate or draft IDs fail. Query selection
always excludes drafts. `work_ref(catalog, "storage")` creates one reference;
`pinned_work_ref(catalog, "storage", #{source_sha256: "..."})` additionally checks
the expected source revision. `document_digest` pins the prepared semantic
document, including mappings. Digests detect mismatches; they do not retrieve
historical content.

`section_ref(catalog, "storage", "handoff")` selects a structural section by its
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

The seven [component contracts](component-contracts.md) support HTML and text.
[Rust builders and `view!`](view-macro.md) offer typed composition. Rhai shares
their schema validation and budgets. [Tera presentations](tera-components.md)
remain replaceable local theme files. Publication metadata controls the page
title; `WebsiteConfig.title` is site identity and cannot overwrite it.

`check` validates permitted source preparation, composition, references, channel
support and an in-memory render. It validates required CSS entry declarations;
CSS compilation remains a build-stage check because it needs a merged workspace.
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
`publication.json` is bound through the exact release bundle; they do not claim
the original-work routes as local legacy document inventory. Imported directory
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
