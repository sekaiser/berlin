# Synthetic composed publications

Copy this directory into a temporary project. Set `BERLIN_THEME_DIR` to this
checkout's absolute `themes` directory, then use `BERLIN_DIR` to select the copy.
See [the runnable commands](../../../docs/composable-publications.md).

| Pipeline | Output | Demonstrates |
| --- | --- | --- |
| `portfolio` | `_portfolio` | Tag/date query, eight metadata cards and editorial introduction |
| `curated` | `_curated` | Explicit source order and annotations |
| `excerpts` | `_excerpts` | Repeated stable section, code/line anchors, cross-document and outside-section links, footnotes, original assets and attribution |
| `text` | `_text` | Same query composer, independent plain text |
| `excerpt_text` | `_excerpt_text` | Semantic excerpts and LinkedIn draft diagnostics |

The twelve documents include a draft and an unrelated-body sentinel. Source URLs
use reserved `example.test`; builds do not fetch them. Source assets remain
original-site links, so the example image/download are intentionally not hosted
by the portfolio. No deployment destination is configured.

To try the second presentation, copy `alternate/pages/.` into `pages/` and
`alternate/styles/.` into `styles/` in the temporary project. Rebuild `portfolio`.
The template and CSS both change; selected work and editorial composition remain
the same. Remove the overrides to restore the notebook presentation.
