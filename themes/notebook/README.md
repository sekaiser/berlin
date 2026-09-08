# Notebook theme

A local Tera/CSS theme for Berlin's website renderer. It shares
typography, colors and components while keeping three compositions distinct:
the continuous homepage sheet, collection openings, and the article with its
navigation margin. No frontend framework, npm packages, downloads, or browser-side theme loader.

## Select the theme in your pipeline

```rhai
let presentation = website_config(#{
    theme: "themes/notebook",
    url: "https://example.com",
    title: "My notebook",
    author: "Your name"
});
output render_website(website, "_site", presentation);
```

`theme` accepts an explicit local directory path (project-relative or absolute),
or a local name such as `@notebook`. Named themes resolve beneath the absolute
`BERLIN_THEME_DIR` environment setting; for this checkout set it to
`/path/to/berlin/themes`. This is a local lookup, not an online registry.
A separate website repository can also keep a pinned copy at `themes/notebook`.
Berlin neither downloads nor silently updates it.

The themed render operation resolves templates, compiles `styles/entries/*.css`, and
copies effective `static/` assets into its staged output. Do not also declare
CSS/copy operations for those same files. Pipelines without `theme` keep their
existing explicit CSS and asset operations.

## Try the synthetic specimen

From the Berlin repository root:

```sh
cargo build --bin bln
notebook_project=$(mktemp -d)
cp -R themes/notebook/example/. "$notebook_project/"
mkdir -p "$notebook_project/themes"
cp -R themes/notebook "$notebook_project/themes/notebook"
BERLIN_DIR="$notebook_project" target/debug/bln serve --port 8082
```

Open `http://localhost:8082/`. The article `notes/component-specimens.html`
demonstrates the palette, introduction, metadata, tags, prose, tables, code,
disclosure, and backlinks. Reading and Search exercise their real controls.
All specimen content is synthetic. No personal website files are required.

## Resolution and overrides

- `shared/`: CSS, scripts, layout templates and component macros.
- `defaults/`: fallback identity/editorial templates and an empty `project.css`.
- `example/`: synthetic content and configuration; not an input to theme resolution.

Precedence is **project → shared → defaults**, by relative filename within
`pages/`, `styles/`, and `static/`. Only effective files are included in the build
receipt. Templates are parsed together after merging, allowing inheritance across
layers. CSS imports see the same merged filesystem in a temporary workspace.
Images, fonts, and scripts belong under `static/`. Stylesheet entry points are
the `.css` files directly inside `styles/entries/`; each becomes
`assets/css/NAME.css` in the output. Supporting files elsewhere in `styles/`
are import-only and are not published separately.
Directories merge; an override replaces an entire file, not individual blocks.

Builds never install or overwrite project sources. To customize a template,
copy that one file into your project's `pages/` at the same relative path and
edit it there. Theme changes are picked up by the next build or `serve --watch`.
Symbolic links within presentation directories are rejected to avoid accidental
publication of unrelated files. Only trusted themes and templates should be used.

`install.cjs` is an optional utility for copying theme files as project overrides.
It does not configure a pipeline or replace theme selection. Existing copies
override the theme; move redundant copies aside when migrating, retaining any
intentional customizations. Its `--check`/`--update` modes apply only to that
vendored workflow, not to projects using native theme resolution.

Customize the project-owned masthead, footer, homepage, About page, metadata and
`notes/[slug].tera`. The latter extends the shared article layout and supplies
optional title, observation and continuation blocks. The `notebook_summary` and
`notebook_connection` macros are empty extension points by default.
`notebook_preview` renders optional article metadata (`source`, `alt`, `width`,
`height`); it does not select artwork by article title or route. Entries without
previews remain text-led. Site-root image URLs are rebased to the configured site
URL, including repository-hosted sites. See [Org preview authoring](../../docs/emacs-authoring.md#article-previews).

## Foundations

The authoritative common palette is `shared/styles/tokens.css`:

| Role | Token | Value |
| --- | --- | --- |
| Reading surface | `--surface` | `#fcf9f0` |
| Surroundings | `--surroundings` | `#dfe7df` |
| Main ink | `--ink` | `#273b32` |
| Secondary ink | `--muted` | `#5d695a` |
| Terracotta accent | `--accent` | `#94492f` |
| Quiet rule | `--rule` | `#d7dccd` |
| Strong rule | `--rule-strong` | `#9fae98` |
| Accent rule | `--accent-rule` | `#b6815d` |

Moss, sky, apricot and lavender tokens support tags and secondary surfaces.
The same file defines the display, reading, interface and monospace font stacks,
metadata and caption typography, outer width and base gutter. Responsive layouts
override the gutter at their established breakpoints, not the palette.

| Entry point | Responsibility |
| --- | --- |
| `vendor.css` | Shared Normalize.css and GitHub Light foundation, loaded first (screen only) |
| `notebook.css` | Palette, typography, code, tags, shared reading layout and environment |
| `overview.css` | Homepage and collection compositions |
| `article.css` | Article backlinks |
| `reading.css` | Reading-list controls |
| `search.css` | Search controls and results |
| `comments.css` | Optional article discussion controls |
| `plain.css` | Optional landscape-free layout; retains the homepage desk |
| `project.css` | Personal overrides, loaded last |
| `giscus.css` | Standalone discussion iframe theme, never loaded by the main page |

Templates load the shared vendor foundation first, then the common entry,
relevant page entries, and project overrides. A project can add its own entries
and link them explicitly.
Building an entry does not automatically load it on every page. Supporting
typography, layout, and component files stay separated in the source tree.
The foundation's screen-only scope and the layouts' print rules are retained.
Vendor copyright and MIT licence notices ship in `static/licenses/notebook-vendor.txt`.

Stylesheets use ordinary CSS, not CSS modules. Local `@import` paths are resolved
relative to their source file within `styles/`. Remote imports remain in the CSS;
builds do not fetch them. Put remote imports before bundled local imports.

`url()` values are relative to the **emitted stylesheet**, including values inside
custom properties: use `../../static/pics/...` or `../../static/fonts/...` from
`assets/css/`. Berlin does not automatically rebase these values. This keeps
paths valid at both domain roots and repository prefixes. Project overrides
belong in `styles/entries/project.css` and remain a separate compiled entry.

## Components and states

| Component | Contract |
| --- | --- |
| Tag link | `macros/tag.tera`; real destination, visible focus, unclipped focus ring |
| Tag toggle | Reading script; button with `aria-pressed`, uncolored when off, 44px touch target |
| Article metadata | Native `time` elements; publication and optional modification date |
| Introduction | Source-owned `.frontmatter > .abstract`; all paragraphs preserved |
| Contents | Source-owned `.toc`; progressive relocation, current-section marker, mobile disclosure |
| Entry / bookmark | Macros with escaped labels, optional artwork and authored annotation |
| Code | Berlin renderer markup; exact copying, wrapping, stable line references and inline notes |
| Search | Berlin index/client; labels, live status, real result links and empty/error states |
| Backlinks | Derived public references, linking to the source passage |
| Discussion | Explicit frontmatter opt-in; reader consent before loading the external widget |
| Footer | Real profile links when configured; accessible icon labels and native back-to-top link |

Use component markup rather than styling a generic element to resemble a control.
Keep actions functional; disabled controls should explain why they are unavailable.
Reduced-motion and print rules remain part of the shared styles.

## Illustration slots

The starter intentionally uses the `theme-plain` body class and `plain.css`.
It does **not** contain the author's paintings, photographs, identity, summaries,
or article-specific diagrams. No generic artwork is presented as a replacement.

To use the illustrated compositions, remove the `theme-plain` class and the
`plain.css` link in the project-owned `_base.tera`. Define these CSS properties
in `styles/entries/project.css` with appropriately licensed image-only assets:

```css
:root {
  --art-article-small: url("../../static/pics/article-small.webp");
  --art-article-wide: url("../../static/pics/article-wide.webp");
  --art-collection-wide: url("../../static/pics/collection.webp");
  --art-collection-small: url("../../static/pics/collection-small.webp");
}
```

The existing scene geometry expects article art around 3:2 and collection art
around 16:9. Composition matters as much as ratio: inspect crops at desktop and
tablet widths. Mobile rules suppress the background environment. A homepage
foreground illustration is a separate editorial choice in `index.tera`.
The default homepage uses `partials/engineering-desk.tera`, backed by the shared
desk PNG and responsive WebP assets in `shared/static/pics/study/`. Personal
homepages can include that same partial while retaining their own copy and
caption. Public asset URLs remain under `/static/pics/study/`; projects need
no local copies. The image provenance is documented alongside the theme assets.

## Checks and distribution

Run `node --test themes/notebook/test.cjs themes/notebook/tests/*.test.cjs` after building Berlin. It tests legacy
vendoring, notices, and native theme builds without installed files at origin-root
and repository-prefix URLs. `support/check` includes it. Browser checks still
need to cover desktop, tablet, phone, the code-heavy middle and footer; structural
tests are not screenshot comparisons.

Berlin's repository license governs the theme's Berlin-owned material. The
theme includes its LICENSE, NOTICE and third-party notices under
`static/licenses/berlin/`. Fraunces is bundled with its own SIL OFL notice and
source information under `static/fonts/fraunces/`. Retain these notices in output.
When the repository's licensing documents change, update the bundled copies too;
the theme tests check LICENSE and NOTICE consistency.
