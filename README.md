# Berlin

A local-first, programmable publishing system built in Rust. Transform structured
content into websites and channel-specific publications through typed pipelines.

Berlin starts from a simple idea: keep ownership of your content, describe how it
should be processed, and publish it in more than one place without maintaining a
separate authoring workflow for every channel.

**Status:** experimental and actively evolving. Today Berlin builds static websites
and local LinkedIn drafts. It does not yet publish through LinkedIn, X, or Substack
APIs, synchronize remote content, or monitor live publications.

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
- Syntax-highlighted code with named listings, line references, captions, and highlights.
- LinkedIn draft text and manifests, with character-limit diagnostics.
- Local build receipts recording input/output hashes, provenance, and diagnostics.
- Staged output replacement with recovery support, plus a local preview server.

The website renderer currently uses a fixed set of page families and template
names. It is not yet a general-purpose theme or component framework. Content and
templates are trusted inputs; raw HTML is supported, not sanitized.

## Try it

You need a current stable Rust toolchain. Emacs is optional when starting from
Markdown; Org export additionally needs ox-hugo. The Nix development shell provides
Rust, Node.js, and Emacs with ox-hugo.

The current public implementation is on `publish/berlin`:

```sh
git clone --single-branch --branch publish/berlin https://github.com/sekaiser/berlin.git
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

BERLIN_DIR="$berlin_project" target/debug/bln plan --pipeline site
BERLIN_DIR="$berlin_project" target/debug/bln build --pipeline site
BERLIN_DIR="$berlin_project" target/debug/bln serve --port 8081
```

Open <http://localhost:8081>. Stop the server with Ctrl-C. `serve` builds the `site`
pipeline before serving `_site`; add `--watch` to rebuild when project inputs change.
`BERLIN_DIR` selects the publishing project; otherwise Berlin uses the current directory.

Generate reviewable LinkedIn drafts without sending anything to LinkedIn:

```sh
BERLIN_DIR="$berlin_project" target/debug/bln build --pipeline linkedin
```

Website files are written to `$berlin_project/_site`, LinkedIn drafts to
`$berlin_project/_berlin/linkedin`, and receipts to `$berlin_project/_berlin/receipts`.
Hosting the resulting website is a separate deployment step.

To try the Org authoring path with Nix:

```sh
BERLIN_DIR="$berlin_project" nix develop --command target/debug/bln build --pipeline org
BERLIN_DIR="$berlin_project" target/debug/bln build --pipeline site
```

Org export is a separate pipeline: watching the website does not automatically
re-export Org sources.

## A pipeline in practice

A project defines its pipelines in `berlin.pipeline.rhai`. This configuration
uses the same Markdown sources for a website and LinkedIn drafts:

```rhai
fn documents() {
    parse_markdown(load_markdown("content/notes/*.md"))
}

pipeline site {
    let presentation = website_config(#{
        url: "https://example.com",
        title: "My notebook",
        author: "Example Author",
        description: "Notes on software and things worth exploring"
    });

    let feed = parse_feed(load_data("data/feed.csv"));
    let website = assemble_website(documents(), feed);

    output render_website(website, "_site", presentation);
    output compile_css(load_css("css/styles.css"), "_site/css/styles.css");
    output copy_assets(load_assets("static/**/*"), "_site/static");
}

pipeline linkedin {
    output render_linkedin(documents(), "_berlin/linkedin");
}

// Optional authoring step: export Org before running either publishing pipeline.
pipeline org {
    output export_org(load_org("data/*.org"), "ox-hugo", "content/notes");
}
```

The `documents()` helper shares the parsing definition, not a cache: each selected
pipeline loads its own inputs. The website renderer uses templates from `pages/`;
LinkedIn drafts use the same semantic documents without those templates. Neither
pipeline sends content to a remote service.

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

UnoCSS is optional project-level styling tooling, separate from Berlin's Rust
renderer. Its dependencies are locked in `package-lock.json`. Supply your own
project-local `unocss.config.ts`, then use `npm ci` and `npm run css` to generate
utilities from your templates and Markdown. The configuration is ignored and is
not bundled with Berlin. The minimal fixture does not require it.

## Further reading

- [Architecture and design boundaries](ARCHITECTURE.md)
- [Local publishing projects](docs/local-publishing.md)
- [Code listings and Org references](docs/code-listings.md)
