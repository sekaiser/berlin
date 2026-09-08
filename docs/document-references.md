# Document references and backlinks

Write connections in the source, where their meaning is visible:

```org
This experiment builds on [[id:target-document-id][the earlier investigation]],
but uses a different input dataset.
```

Use ordinary Org ID links. The batch exporter preserves a file's ID as the target;
links to a heading with an ID become a document ID plus the exported heading
fragment. Keep the file-level ID unchanged when moving or renaming a document.
The adapter must be updated in each publishing project's `support/ox-hugo/` copy.
Publishing continues to disable Babel execution.

The Markdown interchange spelling is `[label](id:target-document-id)`, optionally
followed by a `#fragment` inside the destination. Berlin parses this as a typed
`Inline::DocumentLink`, not a finished website URL. Existing ox-hugo `relref`
shortcodes are upgraded when the target Markdown has an explicit ID. Legacy
targets without IDs retain their existing URL rendering but do not gain backlinks.

## Publication scope

`WebsiteAssembly` excludes drafts before deriving its ordering, tags and reference
index. Only documents in that publication can supply backlink titles and excerpts.
A reference from a published document to an absent or draft target fails assembly;
the error identifies the source URI, generated reference anchor, and target ID.
It does not include the excluded target's title or content. References inside
draft documents do not contribute entries or cause unresolved-target errors.

The index is rebuilt from the assembled semantic blocks, after document mappings.
It does not infer relationships from tags or copy links into `Document.relations`.
A link means only “references.” Self-links remain functional but are omitted from
incoming backlinks. Repeated references to one target within one passage produce
one entry; distinct passages remain distinct. Ordering is deterministic by source
ID and occurrence anchor.

Paragraphs, headings, list paragraphs, table cells, code captions and footnotes
can provide excerpts. Code text, raw HTML and external URLs are not indexed.
Excerpts are plain authored text, with whitespace normalized; the website clips
them at 280 characters and escapes them. No new explanatory prose is generated.
For a meaningful excerpt, put the link in the same paragraph as its explanation.

## Website rendering

The website task resolves target IDs to current article routes, including the
configured site prefix. Article templates receive `page_backlinks`, an array of:

- `title`: the referencing document's current title;
- `path`: a site-relative article URL with a source-passage fragment;
- `excerpt`: the linking passage as text.

An explicit `slug` keeps an article's public URL independent of its title.
`previous_slugs` can redirect earlier public URLs to it. ID links and backlinks
always resolve to the current route; see [stable article URLs](stable-urls.md).

Prefix `path` with `config_site_url` in templates. The publishing fixture includes
a minimal “Referenced in” section. Themes may choose their own placement and
styling, and should omit empty sections. Ordinary links and reference targets work
without JavaScript.

Generated `bln-ref-*` source anchors are rebuilt together with backlinks. They are
not permanent author-assigned IDs: inserting links may renumber them. Existing
heading and code anchors are retained. Target fragments are preserved, but the
reference index validates document membership, not arbitrary rendered HTML
fragments; use final-site link validation to check those fragments.

Standalone semantic HTML rendering without document routes, and LinkedIn text
rendering, retain the link label without emitting an unusable `id:` URL. Other
channel renderers can resolve the typed target according to their own publication.

## Verification

Run `cargo test --workspace` for parsing, identity, index, draft-isolation and
rendering tests. Test the exporter separately with:

```sh
nix develop --command emacs --batch --load support/ox-hugo/tests.el --funcall ert-run-tests-batch-and-exit
```

This uses temporary Org files, including a Babel block that must never execute.
