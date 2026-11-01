![Black-and-white illustration of the Berlin skyline](images/berlin-header.svg)

# Berlin

A local-first, programmable publishing system built in Rust. Transform structured
content into websites and channel-specific publications through typed pipelines.

Berlin starts from a simple idea: keep ownership of your content, describe how it
should be processed, and publish it in more than one place without maintaining a
separate authoring workflow for every channel.

**Status:** experimental and actively evolving. Today Berlin builds static websites
and local LinkedIn drafts, and supports explicit sealed website releases to an
existing GitHub Pages publishing branch. It does not yet publish through LinkedIn,
X, or Substack APIs, synchronize remote articles, or collect audience metrics.

## How it works

```text
Org + ox-hugo (optional) → Markdown → typed documents → channel renderers → local outputs + receipts
```

Markdown is an input and interchange format—not the internal document model.
Berlin parses it into typed blocks, inlines, metadata, and provenance. Processing
and rendering work with that shared representation instead of repeatedly parsing
Markdown or HTML.

Pipelines are written in Rhai. Rust defines the available operations and their
input/output types; the script composes them into a graph. Berlin validates that
graph before executing filesystem or process operations. Scripted document and
section mappings operate through a bounded API, without direct I/O access.

Current capabilities include:

- Org export through Emacs and ox-hugo, without executing source-code examples.
- Markdown parsing, CSV feeds, and website assembly with shared tag indexes.
- Static HTML through Tera templates, CSS compilation, and asset copying.
- [Composable publications](docs/composable-publications.md): curated/query-based portfolios and structural excerpts, composed in Rust or execution-time Rhai, with replaceable Tera HTML and independent text output.
- [Stable article URLs](docs/stable-urls.md), with optional static redirects from previous slugs.
- Syntax-highlighted code with named listings, line references, captions, and highlights.
- [TeX equations](docs/mathematics.md) from Org or Markdown, rendered as static MathML without JavaScript.
- [LaTeX graphics](docs/diagrams.md): shared Org/CLI/pipeline rendering, including TikZ, PGFPlots and LuaLaTeX graph layouts.
- [Pipeline-driven LaTeX publishing](docs/papers.md), with checked references and citations, notebook HTML and a downloadable PDF.
- LinkedIn draft text and manifests, with character-limit diagnostics; [release announcements](docs/announcement-workflow.md) with exact-text approvals and local records of user-reported posts.
- Local build receipts recording input/output hashes, provenance, and diagnostics.
- [Project-local SQLite lifecycle history](docs/lifecycle-state.md) for build attempts, sealed article revisions, and recorded publications.
- [Dependency-checked build caching](docs/build-caching.md) for CSS and supported LaTeX papers.
- Staged output replacement with recovery support, plus a local preview server.
- [Sealed website releases and explicit GitHub Pages publishing](docs/website-releases.md), with optional native production URL/asset preparation, file and document revision comparisons, and recoverable publication records.

One catalog combines local semantic documents and explicitly authored external
resources. Composers select and arrange these into a single publication or a
multipage notebook. Rust builders and Rhai use the same typed components; Tera
presents HTML and a separate renderer produces plain text or LinkedIn drafts.
The [notebook theme](themes/notebook/README.md) supplies local templates, styles
and progressive controls. Project files override shared theme files and defaults.
Raw HTML in authored documents remains trusted input.

## Try it

You need a current stable Rust toolchain. Emacs is optional when starting from
Markdown; Org export additionally needs ox-hugo. The Nix development shell provides
Rust, Node.js, and Emacs with ox-hugo.

```sh
git clone https://github.com/sekaiser/berlin.git
cd berlin
cargo build --bin bln
```

Create a separate temporary project using the synthetic publishing fixture. It is
a minimal functional example, not the author's personal website or a polished theme.
Run these commands from the repository root, in the same shell:

```sh
berlin_project=$(mktemp -d)
cp -R support/fixtures/publishing/. "$berlin_project/"
mkdir -p "$berlin_project/support"
cp -R support/ox-hugo "$berlin_project/support/"
export BERLIN_THEME_DIR="$PWD/themes"

BERLIN_DIR="$berlin_project" target/debug/bln plan --pipeline site
nix develop --command env BERLIN_DIR="$berlin_project" target/debug/bln build --pipeline org
BERLIN_DIR="$berlin_project" target/debug/bln check --pipeline site
BERLIN_DIR="$berlin_project" target/debug/bln build --pipeline site
BERLIN_DIR="$berlin_project" target/debug/bln serve --port 8081
```

Open <http://localhost:8081>. Stop the server with Ctrl-C. `serve` builds the `site`
pipeline and serves its declared website output; use `--pipeline NAME` to select
another website pipeline and `--watch` to rebuild when project inputs change.
`BERLIN_DIR` selects the publishing project; otherwise Berlin uses the current directory.

Generate reviewable LinkedIn drafts without sending anything to LinkedIn:

```sh
BERLIN_DIR="$berlin_project" target/debug/bln build --pipeline linkedin
```

Website files are written to `$berlin_project/_site`, LinkedIn drafts to
`$berlin_project/.berlin/linkedin`, and receipts to `$berlin_project/.berlin/receipts`.
Hosting the resulting website is a separate deployment step.

Projects can opt into SQLite with `bln state migrate`. Subsequent build attempts
append to `.berlin/state.sqlite`; existing JSON receipts remain archived.
`bln state builds` lists build attempts, while `bln state documents` and
`bln state document CONTENT-ID` inspect article revisions from sealed releases and
their recorded publication status. These commands perform no remote operations.
See [lifecycle storage](docs/lifecycle-state.md) for upgrades and backup/restore.
The [metrics storage foundation](docs/metrics.md) reserves typed definitions and
observations for future collectors. Article inspection includes metric coverage;
engagement collection and imports are not enabled.

Org intermediates live under `.berlin/generated/org/`. The export operation takes
an output workspace and an explicit Hugo section:
`export_org(sources, "ox-hugo", ".berlin/generated/org", "notes")`.
Markdown goes to `content/notes/` inside that workspace; copied source assets go
to `static/attachments/<content-hash>/<filename>`. The site copies that attachment
tree to `_site/attachments/`, matching the exported URLs for images and downloads.
The complete workspace is replaced atomically after a successful
export. Run `org` before `site` or `linkedin`, and again after changing Org sources;
consumer pipelines read explicit generated paths and do not automatically refresh
exports.

To try the Org authoring path with Nix:

```sh
BERLIN_DIR="$berlin_project" nix develop --command target/debug/bln build --pipeline org
BERLIN_DIR="$berlin_project" target/debug/bln build --pipeline site
```

Org export is a separate pipeline: watching the website does not automatically
re-export Org sources.

`bln check` inspects the existing Markdown after document mappings, without
publishing. It reports all unresolved document references and optional
observations about disconnected notes and guide coverage. Add `--json` for
structured output. See [Local authoring check](docs/authoring-check.md) for its
scope, source locations, and exit behavior.
An optional [Emacs adapter](docs/emacs-authoring.md) makes findings clickable in
the original Org source when a matching local export map is available.

## A pipeline in practice

Authoring sources may live outside the publishing project. Declare named roots
explicitly in `berlin.pipeline.rhai`:

```rhai
const SOURCE_ROOTS = #{data: "../data"};
// Use load_org("@data/notes/*.org") or load_data("@data/feed.csv").
```

Root locations are relative to the project (absolute directories also work).
Named input paths reject traversal and symlinks escaping their configured root.
Unprefixed patterns remain project-relative; output paths remain confined to the
project. Org navigation resolves named references against the current declaration,
so a cached origin cannot grant access to a removed root.

A project defines its pipelines in `berlin.pipeline.rhai` by default. To select
another file, or combine several files, repeat `--pipeline-file`:

```sh
bln build --pipeline-file shared.rhai --pipeline-file website.rhai --pipeline site
bln plan --pipeline-file shared.rhai --pipeline-file website.rhai --json
bln check --pipeline-file shared.rhai --pipeline-file website.rhai
bln serve --pipeline-file shared.rhai --pipeline-file website.rhai --watch
```

Explicit files **replace** the default; include `berlin.pipeline.rhai` explicitly
if it should participate. Files form one Rhai program in the supplied order,
sharing functions, constants, and a single `SOURCE_ROOTS` declaration. Define each
pipeline name once; duplicate pipeline names and repeated files are errors.
File arguments may be absolute or relative to the project directory (`BERLIN_DIR`,
or the working directory when unset). Source roots, input patterns, and output
paths remain project-relative, not relative to the selected scripts. All selected
files are included in build receipts and watched by `serve --watch`.
`--pipeline` still selects which named pipeline to execute; selecting more files
does not execute every pipeline they define. Combined-program errors include
the starting line of each source file.

This configuration uses the same catalog for a standard notebook and a
summary/link draft of the latest local document. The reusable notebook composition
supplies ordinary routes and empty states; custom functions are optional.

```rhai
// Synthetic Org notebook. Custom composers are optional.
pipeline org {
    output export_org(load_org("data/*.org"), "ox-hugo", ".berlin/generated/org", "notes");
}
let content = catalog([
    documents(load_markdown(".berlin/generated/org/content/notes/*.md")),
    resources(load_data("data/resources.yaml"))
], publication_options(#{source_site_url: "https://example.com"}));

pipeline site {
    let website = compose(content, notebook(#{
        id: "example-notebook", title: "Example notebook",
        pages: [#{id: "about", route: "about.html", title: "About",
                  introduction: "A synthetic publishing fixture."}]
    }));
    output render_html(website, "_site", html_config(#{
        theme: "@notebook", url: "https://example.com", title: "Example notebook",
        author: "Example Author", description: "A synthetic publishing fixture"
    })).with_assets(load_assets(".berlin/generated/org/static/attachments/**/*").named("org_assets"),"attachments");
}
pipeline linkedin {
    output render_text(compose(content, article_draft()), ".berlin/linkedin", text_config(#{profile: "linkedin_draft"}));
}
```

Both renderers consume validated compositions. HTML uses effective Tera files;
text walks the semantic representation directly. Neither build sends content
to a remote service. [Unified publications](docs/unified-publications.md) covers
external-resource schemas, queries, source URLs and output ownership.

This example works with the files in the [synthetic project](support/fixtures/publishing/).
Run each pipeline explicitly, using the project directory from the quick start:

```sh
BERLIN_DIR="$berlin_project" target/debug/bln plan --pipeline site --json
BERLIN_DIR="$berlin_project" target/debug/bln build --pipeline site
BERLIN_DIR="$berlin_project" target/debug/bln build --pipeline linkedin
```

For Org sources, run the `org` pipeline first, as shown above. Add `--dry-run` to
a build command to preview its effects without writing outputs.

Keep publishing inputs separate from Berlin's source. Root-level authoring,
template, and asset directories are ignored, as are generated outputs. Ignore
rules do not remove files already tracked by Git or erase their history. See
[local publishing projects](docs/local-publishing.md) for details.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
node --test support/web/code-controls.test.cjs
```

For the full acceptance check, including a real ox-hugo export and website/LinkedIn
generation in a temporary synthetic project:

```sh
nix develop --command support/check
```

Berlin bundles and minifies CSS through Lightning CSS in its Rust build pipeline.
No npm installation is required. Node.js runs the JavaScript regression tests.

## Further reading

- [Unified publications](docs/unified-publications.md) and [breaking migration](docs/unified-publications-migration.md).

- [Architecture and design boundaries](ARCHITECTURE.md)
- [Local publishing projects](docs/local-publishing.md)
- [Code listings and Org references](docs/code-listings.md)
- [Local authoring check](docs/authoring-check.md)
- [Publication search](docs/notebook-search.md)

## License

Berlin is [source-available](LICENSE), not open source. Personal and other
noncommercial use is permitted under the license, including publishing generated
websites and posts.

An unmonetized personal technical blog is permitted despite incidental
professional visibility; deliberate business promotion requires permission.
Lawfully generated output may remain online and later be monetized without
running Berlin again. Further business use of Berlin requires authorization.

Using Berlin to conduct or support business requires **explicit written approval
from Sebastian Kaiser or a commercial license**. This includes internal business
tools, company websites, client work, and evaluation for a business deployment,
even when Berlin itself is not sold.

The license also sets conditions for redistribution, contributions, and AI use.
It permits building and testing solely to prepare contributions, including
organizational contributions. If you publish supplied presentation assets or
adapted quick-start examples, preserve their notices and make LICENSE and NOTICE
available with the assets or alongside the hosted output; Berlin does not
copy these files for every output path. Production website builds and sealed
website releases include Berlin's licence and notices under `licenses/berlin/`.
Third-party licenses and previously granted rights remain applicable; see
[NOTICE](NOTICE). Request permission through the
[issue tracker](https://github.com/sekaiser/berlin/issues).

Retained Deno-derived CLI code carries its
[MIT notice](cli/THIRD_PARTY_NOTICES.txt); the source-available terms do not
replace that permission or valid earlier MIT grants for Berlin.
