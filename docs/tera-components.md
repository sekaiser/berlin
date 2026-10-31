# Tera publication presentations

HTML uses Tera 1.x. The view macro constructs components; templates own markup.
Effective presentation files follow the existing precedence:

1. Project `pages/`, `styles/`, `static/`.
2. Theme `shared/`.
3. Theme `defaults/`.

A composed website without a selected theme uses complete project-local files.
Named themes remain local lookups through `BERLIN_THEME_DIR`. No theme download
or template installation occurs. The effective files are captured before use;
rendering, CSS compilation, asset copying and manifest hashes use those bytes.

The page template is `pages/publications/page.tera`. Built-ins bind to
`pages/components/berlin/<name>.v1.tera` and require `portfolio.css`. Stylesheet
declarations must appear exactly once in the page head. The page must contain one
`h1`, unique IDs and valid local fragment links. The publication title is
authoritative; site configuration never substitutes its own page title.

Publication-only CSS entries live under `styles/publications/`, with the same
project/shared/default precedence as other presentation files. The renderer
compiles only the filenames required by active component bindings and emits
them under `assets/css/`. A binding can also explicitly use a shared entry from
`styles/entries/`; the publication-specific entry takes precedence if both have
the same name. Legacy website builds continue compiling `styles/entries/` and
do not emit publication-only stylesheets.

## Explicit slots

```tera
<section id="{{ view.anchor }}">
  <h{{ view.heading_level }}>{{ props.title }}</h{{ view.heading_level }}>
  {{ render_slot(owner=view.id, name="body") }}
</section>
```

`props` contains validated properties, `publication` contains publication
metadata, `site` contains presentation settings, and `view` contains occurrence
identity and heading level. `slots` describes available slots. Tera macros and
inheritance remain ordinary theme features. The one legacy
`config_site_url` constant is bridged for notebook link macros.

Children render before the parent. A session-local fragment store accepts only
fragments produced by this renderer or its bounded semantic HTML renderer.
`render_slot` explicitly names the owning occurrence; there is no implicit
current component. Unknown owners/slots and repeated consumption fail. Every
nonempty slot must be consumed; optional empty slots may be omitted.

Slots can be inserted only as element content. Attribute, script, style and
comment placement fails. Plain text cannot enter the trusted-fragment bridge.
Editorial strings are autoescaped once; do not add `safe`. The new rendering
path rejects `safe` and ambient time, randomness and environment functions in
the selected template dependency closure. Unused legacy template files keep
their old behavior. Concurrent root renders have independent slot stores.

Featured-work templates consume `source_description`; excerpt templates consume
`excerpt`. These renderer-owned slots are not authored component children. Source
properties contain display metadata and original URLs, not whole source bodies.
Use `props.annotation_paragraphs` for escaped annotation paragraphs.

## Replace a presentation

Copy `support/fixtures/composable-publications/alternate/pages` and `styles` into
a copied fixture project. Rebuild the same pipeline. It uses a different card
template and stylesheet while retaining composition/source fingerprints. A
presentation change affects the render-input fingerprint. Required templates,
slot consumption and CSS bindings are checked explicitly; there is no silent
fallback to a different component version.

`check` and dry-run render in memory and validate CSS entry declarations. Actual
CSS compilation happens during a staged build. Output and retained-slot budgets
are enforced by fallible sinks; these limits do not bound the entire process RSS.
