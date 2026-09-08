# Local authoring check

```sh
bln check
bln check --pipeline site --json
```

`BERLIN_DIR` selects the project as it does for `build` and `plan`. The default
pipeline is `site`. The selected pipeline must contain exactly one website
output, so unrelated publications cannot accidentally supply each other’s link
targets. Select a separate pipeline when a project has multiple publications.

## What it checks

The report uses the website’s document input **after its Rhai mappings**. Only
documents not marked as drafts contribute to connections or guide coverage.

| Finding | Severity | Meaning |
| --- | --- | --- |
| `unresolved_reference` | Error | An authored document ID link targets a document absent from this publication, including a draft. Every unresolved occurrence is reported. |
| `disconnected_note` | Observation | A published document has no incoming or outgoing resolved links to other published documents. Self-links do not count. |
| `not_in_guide` | Observation | A non-guide document has no direct incoming link from a published `kind: guide` document. Indirect reachability and shared tags do not count. |

Observations are optional editorial information, not requirements. A standalone
note or a collection without guides is legitimate. Guides need not be linked
from another guide. Broken links do not establish connections; a note with only
broken links may also receive a disconnected-note observation.

The implementation reuses the reference analysis that supplies website
backlinks. It does not infer semantic relationships or change source documents.

## Read-only boundary

`check` only loads Markdown, parses it, and applies the document mappings on the
selected website’s document branch. It does not load its feed, render templates,
compile CSS, copy assets, create build transactions, or write receipts. No
Emacs process or Babel execution is invoked. Document preparation that requires
Org export is rejected rather than simulated.

For an Org project, explicitly export first:

```sh
bln build --pipeline org
bln check --pipeline site
```

The first command writes exported Markdown; `check` itself is read-only. It
inspects the **existing export**, not unsaved buffers or later Org edits, and
does not certify export freshness.

This is a document-connection check, not a complete publishing check. It does
not validate rendered fragments, ordinary URL links, external websites, feeds,
templates, route collisions, or channel-specific rendering requirements. Code
and raw HTML are not interpreted as authored document links. Use the normal
build and final-site link validation for those concerns.

## Locations

Findings include the document ID and its actual parsed source URI. Unresolved
references also include the generated passage anchor and authored plain-text
excerpt. For an Org workflow, that URI normally points to exported Markdown.
The batch exporter also writes a local navigation sidecar in
`.berlin/org-origins/`. When a matching sidecar exists, `check` adds an optional
`location.origin`: the Org file URI, its exported `source_hash`, an optional
`heading` (Org `id` and `outline` path), and a `stale` boolean. Reference findings
use their enclosing exported section; document-level observations open the file.
Low-level Org headings exported as lists use the enclosing exported heading.

Sidecars are bound to the document ID, Markdown path and exact Markdown hash.
They store project-relative Org paths, not workstation paths, and are separate
from Markdown, semantic documents and published HTML. They are disposable local
cache files; older export hashes may coexist. A missing or invalid sidecar does
not prevent checking Markdown. Export-map failures produce a warning rather than
failing publication. Berlin bundles the adapter; explicitly export to enable
source navigation. Projects with a custom `support/ox-hugo/export.el` override
must keep that adapter compatible with the origin-map contract.

If the Org bytes changed since export, the origin is marked stale and heading
navigation is omitted. Missing sources fall back to Markdown; ambiguous heading
mappings fall back to the Org file. This is file/heading navigation, not an exact
line-number source map, and does not certify that all Org inputs are up to date.
Local reports contain source paths and should not be published as website data.
See [Emacs authoring checks](emacs-authoring.md) for clickable findings.

`bln-ref-*` anchors are generated locations, not source line numbers or permanent
identifiers. The document ID remains the stable identity. Draft target titles,
paths and passages are not included in reference findings.

## Output and exit status

- Exit `0`: analysis completed without reference errors. Observations may exist.
- Exit `1`: reference errors were found, or the analysis could not complete.
- Invalid command-line arguments follow the CLI’s normal argument-error behavior.

`--json` writes one JSON object to stdout. Runtime failures also produce JSON,
while a short failure summary is written to stderr. The envelope has
`schema_version: 1`, `pipeline`, and `status`:

- `complete`: includes `report`, with `published_documents`, `excluded_drafts`,
  `guides`, and `findings`. Each finding has `severity`, `code`, and `location`;
  unresolved-reference findings also have `target`.
- `incomplete`: includes `message` instead of a report. Missing Markdown,
  malformed documents, invalid identities, mapper failures, and unsupported
  pipeline shapes cannot be presented as successful empty checks.

Preparation errors stop analysis; only unresolved-reference errors are
accumulated. This avoids misleading graph observations from partially parsed
or partially mapped collections. Findings have deterministic document-ID and
passage-anchor ordering.
