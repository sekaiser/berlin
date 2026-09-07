# Code listings

Berlin's semantic HTML renderer gives fenced code a light syntax theme, a listing
link, and (for multi-line examples) native line links. The source remains one code
element: highlighting retains multi-line syntax state, and copying excludes the
separate line-number gutter. A small script reveals Copy only when the Clipboard
API is available and reports permission failures without hiding the source.

Ordinary listings need no Org changes or runtime services. JavaScript is optional. These
assets can be served by a static host, including GitHub Pages.

## Link lifetime

Unnamed listing anchors use a SHA-256-derived fingerprint of the language and code text.
Unrelated insertions do not shift them. Repeated identical listings receive a
document-local suffix. Editing code changes its fingerprint. Line links append
`-L<number>`.

Named Org source blocks use the anchor emitted by ox-hugo, which Berlin associates
with the following code fence. These listing links survive edits to the code.

## Named references and explanations

Use Org's existing source-block names and coderef syntax:

```org
#+NAME: country-extraction
#+BEGIN_SRC rust -n -r
fn country_name(value: &Value) -> &Value {
    &value["countryObject"]["name"] (ref:country-name)
}
#+END_SRC

The [[(country-name)][country lookup]] extracts the nested name.
See [[country-extraction][the complete example]].
```

The `-r` switch removes the reference marker from exported code. Berlin does not
strip anything from code text itself. The explanation stays ordinary prose, and
can occur before or after the listing, including within a list. Each prose link
gets a distinct return destination; the referenced line offers a Note link back.
All navigation uses native HTML fragments and works without JavaScript.

Berlin reads `id`, `lineanchors`, `linenostart`, and `hl_lines` from Goldmark-style fenced-code
attributes, and recognizes ox-hugo's exact standalone `<a id="…"></a>` form before
a fence. Other HTML remains untouched. Legacy `highlight` shortcodes are not
supported by this adapter: use ox-hugo's Goldmark/fenced-code export mode.

The metadata is stored in `CodeBlock.references`, not HTML. Validation rejects
invalid reference IDs, duplicate semantic anchors, invalid line numbering, and
unresolved links in exported code-reference namespaces. Arbitrary raw HTML IDs
and ordinary local links are outside this validation.

Org names track the intended operation through an export. Ox-hugo's generated
coderef prefix depends on reference positions: re-exporting updates internal
links, but externally shared line URLs are not guaranteed to survive edits.
Use the named listing URL for durable external references.

## Captions and highlighting

An immediately following ox-hugo `src-block-caption` wrapper is associated with
its code block as inline caption content. The exporter’s generated “Code Snippet”
label is removed; the caption is rendered inside the listing’s `figcaption`.
Inline formatting is retained, including trusted raw inline HTML, as elsewhere
in Berlin documents. Unrecognized or nested HTML wrappers remain untouched.
The LinkedIn projection also retains the caption as text.

Use `:hl_lines 8,36,51` in an Org source-block header to emphasize those source
lines. Berlin accepts exported arrays such as `hl_lines=["8","36","51"]`,
including ranges (`"3-5"`). Positions are one-based relative to the code, not
the displayed `linenostart`. Invalid or out-of-bounds ranges fail validation.

The HTML renderer emits separate gutter and highlight-overlay elements so a host
website can style selected lines without modifying copied code. Host stylesheets
are responsible for positioning these elements and providing print styles.

## Export safety

The batch exporter disables Babel processing: publishing must not execute source
examples. Existing stored results may still appear in exported Markdown.

## Emacs environment

System Emacs alone may not include ox-hugo. Berlin's declared Nix development
environment provides both; no changes to personal Emacs configuration are needed:

```sh
nix develop
cargo run --bin bln -- build --pipeline org
cargo run --bin bln -- build --pipeline site
```

These commands require a local publishing project. See
[Local publishing](local-publishing.md) for a synthetic example and acceptance checks.

## Checks

Run `cargo test -p berlin_document_html` and
`node --test support/web/code-controls.test.cjs`.
The reusable copy enhancement lives in `support/web/code-controls.js`; copy it
into your site's assets and load it with a deferred script tag.
