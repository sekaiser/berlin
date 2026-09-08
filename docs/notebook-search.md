# Publication search

Add `pages/search.tera` to opt a website into search. Without that template,
Berlin generates neither a search page nor an index and existing themes are
unchanged. The [publishing fixture](../support/fixtures/publishing/pages/search.tera)
contains a minimal integration.

The website renderer writes these files in the same output transaction:

- `search.html`: your search template, with the usual base context and `page_path`.
- `search/index.json`: versioned search data for this publication.
- `search/client.js`: the dependency-free search client.

Base template context includes `has_search` for conditional navigation. The client
expects a container with `data-search-index` pointing to the index, a hidden form
with a search input (`name="q"`, `maxlength="200"`), an element with
`data-search-status`, and a list with `data-search-results`. Load the client with
`defer`. Give the input a label and the status `role="status"`. Include a
`noscript` explanation and a normal Notes link; browsing must not depend on search.
Index and script URLs should use `config_site_url` so project Pages prefixes work.
Append `?v={{search_index_version}}` to the index URL; this content hash changes
with the index so browsers do not retain search results from an earlier build.

## Scope

The index is derived from `WebsiteAssembly` after document mappings and draft
exclusion. It includes published notes and guides: titles, tags, section headings,
prose, lists, tables, code and captions. It never serializes provenance, source
paths, component properties, or raw HTML. Authored text already visible in a
published document remains public, including code. The index is not a secret
store and must never be built from a private authoring collection.

Saved-reading entries retain their separate Reading-page filter. About pages,
template-only copy, text embedded in images and arbitrary HTML are not indexed.

## Matching and navigation

Matching is case-insensitive with Unicode normalization. Whitespace separates
literal terms; every term must occur in the same section or its document title
and tags. This is not regex, a quoted-phrase language, fuzzy search, or semantic
search. Title and heading matches rank above tag and body matches. Each document
appears once, linking to its best matching section, with an excerpt of authored
text. At most 30 results are displayed; the status reports the total count.
Excerpts end on whitespace-delimited token boundaries; the length target is soft
for unusually long identifiers. Matching tokens in titles, section labels and
excerpts receive semantic `mark` elements without altering the original spelling.
Themes can style these without adding font weight. The index includes `tag_paths`
derived from actual tag routes; results use `paper-tag` links with `data-tag`
labels, matching the notebook's tag styling. Older indexes without these paths
retain noninteractive tag labels.

The query is kept in `?q=` for shareable links and reloads. Search runs entirely in
the browser after fetching one publication-local JSON file; there is no hosted
search service, analytics, or persistent storage. Normal hosting logs may still
record requested page URLs, including query parameters.

The browser receives plain text and constructs result elements with `textContent`.
Source headings use their rendered IDs; snippets and generated source locations
are not new permanent identifiers. The index is rebuilt with the site rather than
maintained in an independent database. This small-collection implementation scans
its in-memory data on each query; reassess indexing strategy if the corpus grows
enough for measured search latency or download size to become a problem.
