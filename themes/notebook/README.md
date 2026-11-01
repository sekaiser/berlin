# Notebook theme

A local Tera/CSS theme for Berlin's website renderer. It shares
typography, colors and components while keeping three compositions distinct:
the continuous homepage sheet, collection openings, and the article with its
navigation margin. No frontend framework, npm packages, downloads, or browser-side theme loader.

## Select the theme in your pipeline

```rhai
let presentation = html_config(#{
    theme: "themes/notebook",
    url: "https://example.com",
    title: "My notebook",
    author: "Your name"
});
output render_html(compose(sources, Fn("notebook")), "_site", presentation);
```

`theme` accepts an explicit local directory path (project-relative or absolute),
or a local name such as `@notebook`. Named themes resolve beneath the absolute
`BERLIN_THEME_DIR` environment setting; for this checkout set it to
`/path/to/berlin/themes`. This is a local lookup, not an online registry.
A separate website repository can also keep a pinned copy at `themes/notebook`.
Berlin neither downloads nor silently updates it.

## Composed publications

The shared registry includes resource cards, citations, reading lists and resource
groups, alongside portfolios, sections, introductions, featured/related work,
callouts and local structural excerpts. Resource cards and citations accept both
tagged local and external references. External descriptions and annotations are
plain text; no external page content is fetched or treated as trusted HTML.
Cards optionally select the closed `resource_card.repository.v1.tera` and
`resource_card.paper.v1.tera` bindings when supplied by a theme; otherwise they
use `resource_card.v1.tera`. Custom kinds always use the generic card.

The theme also supplies version-1 portfolio components under
`shared/pages/components/berlin/`, the page wrapper
`shared/pages/publications/page.tera`, and
`shared/styles/publications/portfolio.css`. This stylesheet is compiled only for
a publication that binds it. `render_html` accepts single publications and explicit
multipage site compositions through the same catalog pipeline.
[Try the synthetic portfolio](../../docs/composable-publications.md) and see the
[Tera slot contract](../../docs/tera-components.md) before overriding a component.

The themed render operation captures effective template/style/static bytes,
compiles only bound CSS entries, and installs files in the output transaction.
Do not separately copy effective `static/` files or compile CSS over those paths.
Use `.with_assets(...)` for additional confined attachments. The example includes
an executable notebook composer in its pipeline configuration.

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
disclosures, and backlinks, with a contents link for every component section.
A short chooser connects common writing needs to the appropriate examples.
The five authored treatments include native downloads of their Org source;
the example pipeline copies `authoring/*.org` into `examples/org/` without running
them. Save the diagram's linked SVG beside `diagrams.org` before exporting it.
These downloads belong to the specimen project, not every themed website.
Reading and Search exercise their real controls.
All specimen content is synthetic. No personal website files are required.

## Resolution and overrides

- `shared/`: CSS, scripts, layout templates and component macros.
- `defaults/`: fallback identity/editorial templates and an empty `project.css`.
- `example/`: synthetic content and configuration; not an input to theme resolution.

Precedence is **project → shared → defaults**, by relative filename within
`pages/`, `styles/`, and `static/`. Only effective files are included in the build
receipt. Templates are parsed together after merging, allowing inheritance across
layers. CSS imports use the same merged, captured bytes without a temporary stylesheet tree.
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

The `notebook_entry::entry` macro accepts `preview_side="right"` for a trailing
preview. Its default, `"auto"`, uses `notebook_entry_layout::preview_side(target)`;
the theme returns `left`. Override that small macro in project templates to
choose placement consistently across home, Notes and tag collections, rather
than repeating per-page decisions. Explicit `left` or `right` arguments take
precedence. Placement is a presentation choice, not document metadata. On phones,
a trailing preview follows the text; source and keyboard order match the layout.

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

The tag palette lives in `shared/styles/components/tag-palette.css`: 64 fixed
slots, arranged as 32 color bases with two tones each. The bases span warm and
cool hues plus cocoa, taupe, stone and graphite neutrals, with varied saturation
and lightness to keep the palette from reading as repeated pale tints. Every slot provides
`--tag-N-paper`, `--tag-N-ink` and `--tag-N-edge` tokens. Text contrast is at least
4.5:1; edges have at least 3:1 contrast against their tag surface.

Tags choose a slot by hashing the lowercased label with UTF-8 FNV-1a, modulo 64.
The `stable_hash` template filter renders `data-tag-color` before JavaScript runs.
Reading filters copy their corresponding link's slot; search uses the same fixed
algorithm in `static/js/tag-colors.js`. Colors survive rebuilds, collection order
changes and new tags. Collisions are expected; labels, not colors, identify tags.
Unselected Reading filters remain uncolored. Projects can override a slot's
tokens or retain deliberate `data-tag` color assignments in their own CSS.

Moss, sky, apricot, lavender, ochre and rose tokens remain available for secondary
surfaces and existing project overrides; they are not the 64-slot tag palette.
The same file defines the display, reading, interface and monospace font stacks,
metadata, control, code and caption typography, outer width and base gutter.
Responsive layouts override the gutter at their established breakpoints, not the palette.

Article prose and introductions fill the available reading column; the contents
margin occupies its own grid track on desktop. Projects can set `--reading-width`
on `.study-page` to opt into a narrower measure. Export an explicit contents block
inside the Org `frontmatter` block with `#+toc: headlines 2` for two heading levels;
the article script places it in the desktop margin and a mobile disclosure.
Without JavaScript, it remains in its original position in the document.

`--figure-gap` gives traversals, results, comparisons and diagram plates the same
outer rhythm: 42px above and below on larger screens, 30px at 600px and below.
Override it in project CSS to adjust those figures together. Their internal grids,
captions and surfaces remain component-specific; observations, ordinary figures,
code listings and collection entries keep their own spacing. The component rules
retain a 42px fallback for print, where the screen-only tokens are not loaded.

| Entry point | Responsibility |
| --- | --- |
| `vendor.css` | Shared Normalize.css and GitHub Light foundation, loaded first (screen only) |
| `notebook.css` | Palette, typography, code, tags, shared reading layout and environment |
| `overview.css` | Homepage and collection compositions |
| `article.css` | Article utility row and backlinks |
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
| Article utilities | Ruled row below the introduction: estimated reading time left, LinkedIn/X/email/copy icons right. Tooltips and accessible names identify actions; links use the canonical URL. No third-party requests until clicked |
| Introduction | Source-owned `.frontmatter > .abstract`; all paragraphs preserved |
| Contents | Source-owned `.toc`; progressive relocation, current-section marker, mobile disclosure |
| Entry / bookmark | Macros with escaped labels, optional artwork and authored annotation; entries can opt into a selected treatment |
| Code | Berlin renderer markup; exact copying, wrapping, stable line references and inline notes |
| Traversal | Authored figure with an ordered sequence, optional branches, and native links into source |
| Result table | Captioned table with a light default, forest variant, and optional native source disclosure |
| Comparison | Two labeled specimens under one caption, side by side or stacked in reading order |
| Diagram plate | Opt-in image surface with the native caption, optional legend and full-size attachment link |
| Observation | Source-owned reading note with a small heading, ordinary prose and optional source link |
| Search | Berlin index/client; labels, live status, real result links and empty/error states |
| Backlinks | Grouped by stable source ID; first passage visible, additional passage links in a native disclosure |
| Discussion | Explicit frontmatter opt-in; reader consent before loading the external widget |
| Footer | Real profile links when configured; accessible icon labels and native back-to-top link |

Use component markup rather than styling a generic element to resemble a control.
Keep actions functional; disabled controls should explain why they are unavailable.
Reduced-motion and print rules remain part of the shared styles.

### Featured entries: select without reordering

`macros/notebook_entry.tera` accepts `featured=true`. This adds the
`journal-entry--featured` modifier and a small “Selected note” label to the same
entry, using a larger regular-weight serif title and more room for its preview.
It remains on the existing ivory surface. Entries without a preview stay text-led;
the component supplies neither a substitute image nor a placeholder.

Selection belongs to the template placing the entry, not to the document's
publication date or kind. For example, in a project-owned homepage:

```tera
{% import "macros/notebook_entry.tera" as notes %}
{% for a in articles %}
  {% set selected = a.target == "/notes/a-note.html" %}
  {{ notes::entry(title=a.title, description=a.description, date=a.date,
      target=a.target, topics=a.tags, kind=a.kind, preview=a.preview,
      featured=selected) }}
{% endfor %}
```

Replace the example route with an actual published permalink. This does not sort,
pin, duplicate, or fetch an entry outside the collection passed to the template.
If the selected note is absent, no featured entry is rendered. Other placements,
including Notes and tag pages, stay compact unless they explicitly opt in.
The default theme does not automatically designate any entry as selected. Prefer
one selected note per collection; the stress fixture deliberately includes both
an illustrated and a text-only variant for testing.

Both variants preserve the title, description, publication metadata, kind, tags,
preview alt text, and native link destinations. Keep `heading_level=2` when entries
sit directly below a page heading; the default is level 3 for a titled section.
The preview precedes the text in DOM and visual order, stacking above it on phones.
Images retain their aspect ratio and are never cropped. No additional control,
animation, or JavaScript is involved.

### Traversals: explain a sequence

Use a `figure.notebook-traversal` with a `figcaption` and an
`ol.traversal-steps`. Each direct list item is one step. Give its heading the
`traversal-step-title` class; choose the heading level that fits the document.
Include `role="list"` on the ordered list to retain list semantics in browsers
that suppress them when native markers are visually replaced. Numbering resets
for each figure; ordinary lists inside a step are not traversal steps.

Optional parts are `traversal-eyebrow`, `traversal-branches` (a grid of `div`
branches), `traversal-label`, `traversal-path`, and `traversal-result`. Omit them
when the explanation does not need them. Use a unique caption ID with
`aria-labelledby` for each figure. Links are ordinary anchors to existing code
line IDs; the component neither creates source anchors nor executes examples.

Copy [the complete Org example](example/authoring/traversal.org). Its figure is
inside `#+begin_export html`, so write HTML inside that block, not Org or Markdown
syntax. Berlin's existing Ox-Hugo export preserves it; no additional exporter,
template registration, Rhai operation, or JavaScript is required. The same HTML
works in Markdown. The specimen article includes both the full example and a
minimal two-step figure. These are website presentation blocks, not a new
cross-channel semantic document type.

The component uses the common ink, accent and rule tokens, with
`--traversal-surface` and `--traversal-heading` defining its two surfaces. It keeps
the connected margin on wide screens and moves numbers above the text on phones.
No motion is required; the figure remains readable without JavaScript and in print.
Medal counts and other domain-specific decoration belong to the article, not
the traversal component. Earlier `medal-flow` / `flow-*` markup remains styled
for existing exports; use the generic classes for new content.

### Result tables: present an outcome

Use `figure.notebook-result` for a light result panel. Add
`notebook-result--forest` for the dark treatment. Both variants use the same
markup: a `figcaption`, a `.result-table` scrolling region containing a semantic
`table`, and optionally `details.result-source`. Copy the
[complete Org examples](example/authoring/results.org); the same HTML works in
Markdown. No theme script, data parser, or execution step is required.

The caption requires a `.result-title`; `.result-eyebrow`, `.result-description`
and `.result-meta` are optional. Associate the figure with its caption using a
unique ID and `aria-labelledby`. Give the table its own `caption` (use
`.result-accessible-caption` to hide it visually), and use `scope="col"` and
`scope="row"` on header cells. Mark numeric cells and their column headings with
`.result-number`; text columns stay left-aligned. There is no required column
count, ranking, summation, or special treatment of zero.

Give `.result-table` a meaningful accessible name, `role="region"`, and
`tabindex="0"` so a wide table can be scrolled with the keyboard. The table
scrolls within the figure, not the entire page. A source disclosure can contain
plain `pre > code` or an existing Berlin `.code-listing`; it keeps its native
keyboard behavior and the source retains a light reading surface. Figures without
a source disclosure do not leave an empty footer. Print styling uses dark ink on
white and exposes the source in browsers supporting `::details-content`.

The forest palette is defined by the `--forest-*` tokens in `tokens.css`.
The component maps both variants to local `--result-*` roles; it does not style
ordinary article tables or couple them to a dark surface. Domain-specific colors
and column proportions remain project-owned. The Rust article's adapter reads
its recorded medal output and emits this markup; neither that parser nor the
medal data is part of the shared theme. Like traversal figures, these are website
presentation blocks, not a new semantic document type or an automatic Org-table
conversion.

### Comparison spreads: show the difference

Use `figure.notebook-comparison` with one `figcaption` and a
`.comparison-panels` wrapper containing two `.comparison-panel` elements.
Label each panel with a heading using `.comparison-label`; choose heading levels
that fit the surrounding document. The caption uses `.comparison-title`, with
optional `.comparison-eyebrow` and `.comparison-description`. An optional
`.comparison-note` follows both panels for a shared explanation or source link.
Use a unique caption ID and `aria-labelledby` on the figure.

Copy the [Org comparison example](example/authoring/comparison.org). As with the
other specimens, HTML inside an Org export block works through the existing
pipeline without an additional parser or script. Panels may hold prose, a plain
`pre > code`, or an existing Berlin code listing. Give scrollable `pre` elements
`tabindex="0"` and a descriptive `aria-label`; code retains its source whitespace
and scrolls internally. Comparison styles do not replace code-listing controls.

For paired code listings, add `notebook-comparison--code` to the figure. This
variant stacks the panels when the figure is 880px wide or narrower, including
when an article's navigation margin reduces its available width. When placing
Org source blocks between HTML export fragments, keep the HTML flush left so
the Markdown parser does not interpret it as an indented code block.

The authored order is the reading order: left then right on wide screens, first
then second at 700px and below. In print, panels stack and plain code wraps.
The two surface treatments distinguish the specimens, not success and failure.
Use meaningful labels such as input/output or two representations. The component
does not compute a diff, execute examples, synchronize scrolling, or imply that
one approach is better. A before/after claim must come from the content itself.

### Diagram plates: give a figure its own surface

Ordinary captioned images are centered in the reading column, with captions
wrapping to the displayed image width. They shrink to fit narrow screens without
stretching smaller images. Diagram plates retain their full-width caption surface.

Wrap a captioned image in `notebook-diagram`. Unlike the comparison and result
examples, this can be written entirely with ordinary Org special blocks and file
links. Copy [the Org example](example/authoring/diagrams.org), with the saved
[notebook SVG](example/static/diagrams/notebook-plot.svg) beside it:

```org
#+begin_notebook-diagram
#+CAPTION: What the diagram shows.
#+ATTR_HTML: :alt A useful description of the information in the image.
[[file:diagram.svg]]

#+begin_diagram-legend
- Explain a symbol or relationship that matters to the reader.
#+end_diagram-legend

#+begin_diagram-actions
[[file:diagram.svg][Open the full-size diagram]]
#+end_diagram-actions
#+end_notebook-diagram
```

Keep the image as an Org file/attachment link, not a raw HTML path: the existing
exporter resolves and publishes the asset with a content-addressed URL. Captions
and alt text remain source-owned. Omit the legend or action block when unnecessary.
The equivalent HTML is a `.notebook-diagram` wrapper around a native `figure`
containing `img` and `figcaption`, followed by optional `.diagram-legend` and
`.diagram-actions` blocks.

The component preserves aspect ratio and never crops, recolors or inlines an
image. The white `--diagram-canvas` separates precise artwork from its warm caption
surface. Images fit the available width without upscaling by default; for a vector
that needs a larger intended size, add `#+ATTR_HTML: :style --diagram-width:40rem;`
before the outer block. Dense labels can still require the full-size link on a
phone. That link opens the original image, not a JavaScript lightbox. Print keeps
the image and explanation while omitting the action row.

The specimen uses the existing PGFPlots fixture's four data points with the
opt-in [notebook plotting style](authoring/pgfplots/README.md). Its original blue
rendering remains available in a native comparison disclosure. The style sets
ink, labels, axes, grid and series encodings only when `notebook axis` is selected;
it does not alter existing plots, data ranges, or meaningful author-chosen colors.
Its colors correspond to the CSS palette, but changing CSS cannot recolor a saved SVG.

Source and reproduction commands are documented alongside the SVGs. Normal theme
builds do not run TeX. Any supported saved image can use the plate, including TikZ
SVGs and technical screenshots. Ordinary photographs and decorative artwork are
not restyled unless explicitly placed in this wrapper.

### Observations: keep a detail beside the work

Use `.notebook-observation` for a short authored observation, not a warning,
quotation, or a substitute for the main explanation. The theme gives its title
a narrow column beside normal-weight prose on wide screens, then stacks them
on phones. It remains in the authored reading order at every size, including
without JavaScript. Nothing is moved into the navigation margin or hidden on mobile.

Copy [the native Org example](example/authoring/observations.org). The parts are
`.observation-title`, `.observation-body`, and an optional `.observation-actions`.
Give the outer block `role="note"` and an accessible name matching its title:

```org
#+ATTR_HTML: :role note :aria-label Zero is still a value
#+begin_notebook-observation
#+begin_observation-title
Zero is still a value
#+end_observation-title

#+begin_observation-body
In this example, =Some(0)= records a value; =None= records its absence.
#+end_observation-body
#+end_notebook-observation
```

This uses the existing Org special-block exporter, not a new directive or parser.
The equivalent HTML uses classed `div` elements and ordinary paragraphs. A title
is a label for the note, not an extra article heading or contents-list entry.
Use native links to actual targets in an action block; omit it when unnecessary.
The complete example uses Org's named code reference, `(ref:recorded-value)` and
`[[(recorded-value)][See the recorded value]]`, so Ox-Hugo resolves the destination
from the source. The `-r` switch removes the reference marker from published code.
The component does not create references, execute code, or invent annotations.
Print retains the observation and omits its action row.

### Article utility row

The shared article layout includes `partials/article-sharing.tera` below the
introduction. Override the `article_sharing` block to replace or omit the row.
Its links work without JavaScript; without introduction relocation, the row
appears above the source introduction. `article-actions.js` progressively adds
an estimated duration at 220 prose words per minute, excluding code listings,
and enables Copy link when the Clipboard API is available. Copy feedback is
announced through a live status region, including permission failures. The row
does not add bookmarks, audio playback, tracking scripts or platform SDKs.

### CSS ownership

`shared/styles/components/entry.css`, imported by `entries/overview.css`, owns
compact and selected collection entries, including metadata, excerpts, previews,
responsive sizing and print treatment. The overview stylesheet owns the surrounding
page composition, not the internals of an entry.

Code listing appearance, source-row geometry, syntax contrast, copy/wrap states,
line anchors, inline notes, and their mobile/print rules belong to
`shared/styles/components/code.css`. Its base renderer rules and article variant
are colocated; layout files must not restyle them.

`shared/styles/components/examples.css` owns result disclosures, including the
embedded-result variant that suppresses code controls. `components/traversal.css`
owns step-by-step figures and their responsive/print treatment, including legacy
authoring aliases. `components/result.css` owns captioned result tables, their
light/forest variants and source disclosure surfaces. `components/comparison.css`
owns paired specimens and their stacking behavior. `components/diagram.css` owns
opt-in diagram plates; the graphic contents remain untouched. `components/observation.css`
owns in-flow observations and the existing template margin-note treatment. The layout files own the reading
column, navigation margin, and illustrated environment, not these components'
internal presentation. An ownership regression test protects that boundary.

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

## Design maintenance

The current palette, typography and three page compositions are the design baseline.
Use the existing components for new content; add a variant only when a real authoring
need cannot be expressed clearly with them. Keep variants opt-in and preserve the
default rendering of existing documents. Personal artwork, editorial selections and
domain-specific data adapters remain project-owned.

For a visual fix, add a synthetic regression case and check the actual rendered
result, not just the generated markup. Shared changes must retain the continuous
homepage surface, distinct collection opening and article reading margin. Verify
desktop, tablet and phone layouts, the code-heavy middle and footer, keyboard
navigation, no-JavaScript behavior and print. Compare an illustrated project too;
the synthetic specimen does not exercise every project's artwork or overrides.

Keep validation screenshots outside the distributed theme. Publishing requires a
fresh build and review of the sealed release, rather than edits to generated pages.
The checks below are repeatable acceptance tools, not a claim of pixel-perfect or
cross-browser certification.

## Checks and distribution

Run `node --test themes/notebook/test.cjs themes/notebook/tests/*.test.cjs` after building Berlin. It tests legacy
vendoring, notices, and native theme builds without installed files at origin-root
and repository-prefix URLs. `support/check` includes it. Browser checks still
need to cover desktop, tablet, phone, the code-heavy middle and footer; structural
tests are not screenshot comparisons.

`support/check` also runs Org traversal, result-table, comparison, diagram and observation round-trip tests: the
authoring fixtures are exported without Babel execution, then rendered through
the real theme with the figure markup, source data and code-link destinations checked.

The example article includes a long caption, an overflowing source line and an
inline code note. Additional synthetic fixtures in `tests/fixtures/stress/`
exercise a long title, twenty tags, missing optional metadata, an untagged saved
reference, a long URL, a wide table, and selected entries with and without artwork.
Tests build a separate empty collection
as well. These fixtures never read or modify a personal publishing project.

Run the opt-in browser suite with an externally provided Playwright installation:

```sh
BERLIN_PLAYWRIGHT_PATH=/path/to/node_modules/playwright \
  node themes/notebook/tests/browser.cjs
```

`BERLIN_BROWSER_EXECUTABLE` can select an already installed Chromium executable.
`BERLIN_TEST_BINARY` can select a built Berlin binary. No browser or frontend
dependency is installed by these checks. Screenshots go to a temporary directory,
or `BERLIN_VALIDATION_DIR` when supplied; temporary publishing projects are removed.
The suite checks desktop, tablet, 390px/320px phones, 200% root text size, keyboard
notes, tag filtering, persisted filters, and no-JavaScript fallbacks using real
build output served from a temporary loopback HTTP server under a URL prefix.
It verifies native Org downloads against the original file bytes, section navigation,
the shared figure-gap override, and focus contrast in both result-table variants.
Each preview server and its temporary project are closed after the checks.
It is not a cross-browser certification; inspect
the screenshots and the illustrated project when changing shared layout rules.

Berlin's repository license governs the theme's Berlin-owned material. The
theme includes its LICENSE, NOTICE and third-party notices under
`static/licenses/berlin/`. Fraunces is bundled with its own SIL OFL notice and
source information under `static/fonts/fraunces/`. Retain these notices in output.
When the repository's licensing documents change, update the bundled copies too;
the theme tests check LICENSE and NOTICE consistency.
