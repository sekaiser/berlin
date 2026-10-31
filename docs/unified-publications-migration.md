# Moving to unified publications

This is a breaking publishing API change. Update pipeline files and themes before
building. Berlin does not silently translate an old program or rewrite saved
release state.

| Removed surface | Replacement |
| --- | --- |
| `parse_markdown` / `parse_documents` | `documents(load_markdown(...))` / `documents(load_data(...))` |
| `Feed`, `FeedItem`, `parse_feed` | Authored external-resource schema and `resources(load_data(...))` |
| `WebsiteAssembly`, `assemble_website` | `catalog([...])`, then a composer returning an explicit `SiteSpec` |
| `compose_publication` | `compose(catalog, Fn("composer"))` |
| `website_config` | `html_config` |
| `render_website`, `render_publication_website` | `render_html` |
| `render_linkedin`, `render_publication_text` | `render_text` with an explicit draft profile |

Old function names fail during planning. The old enum variants and runtime
artifacts are removed, as is the fixed website task. Prepared semantic documents,
Org export, paper/diagram compilation, independent CSS/assets operations, source
roots, document mappers, staging and explicit sealed releases remain supported.

External resources require stable authored IDs, kinds, titles and validated
original URLs. The new CSV schema has explicit metadata columns and JSON-array
list fields; a legacy bookmark export is not automatically accepted. Convert
that data deliberately or use the documented versioned YAML schema.

Define home/notes/reading/about pages in the composer, followed by document, tag,
search and redirect entries as appropriate. A page omitted from the composition
is absent from the site. Use a complete structural excerpt for an article-based
text draft. Do not depend on the former one-text-file-per-document behavior.

Themes insert content through owned slots, such as
`{{ render_slot(owner=view.id, name="body") }}`. The old global document `render()`
callback is gone. Ordinary text is escaped; `safe` is not an alternative slot API.
Keep supporting theme files where needed, but update selected page/component
bindings. Selected CSS is captured and compiled, rather than compiling every
legacy entry unconditionally. Effective static assets already belong to the
render operation; remove duplicate copy outputs and use `with_assets` for extra
inputs.

Original-source URLs belong to catalog options; destination URLs belong to HTML
configuration and are required for rendering. Local source-document pages may use
an explicit prefix. Do not infer original destinations from a selected subset.

Existing sealed release files, identities and publication ledgers are not migration
inputs. Verification and comparison continue to read supported historical bundles
as saved. Importing a prepared directory records its bytes without inventing a
catalog or document history. No rebuild, branch update, or remote publication is
part of this migration.

Use [the unified guide](unified-publications.md) and the maintained fixtures under
`support/fixtures/{publishing,composable-publications,unified-resources}` as working
examples. Browser, keyboard and independent source review are separate
qualification gates; automated build success does not establish them.
