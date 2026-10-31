# LaTeX papers

Berlin processes saved LaTeX articles in the publishing pipeline, producing a
semantic web document and a downloadable PDF together. Authors do not run an
import command or manage intermediate bundles.

## Configure the pipeline

`load_latex` snapshots sources and their declared includes, class wrappers,
bibliographies and figures. `compile_papers` produces a document collection and writes content-addressed PDFs and figure assets
into the pipeline's staged output.

```rhai
fn notebook(c) {
    let local=c.select(#{origins_any:["local"],order:"published_desc"});
    let result=site("papers",#{title:"Papers and notes"});
    result.add_page("home","index.html",home_page(local,c.select(#{origins_any:["external"]})));
    result.add_document_pages(local,"notes");
    result.add_search(local);
    result
}
pipeline site {
    let notes = documents(load_markdown("content/notes/*.md"));
    let papers = compile_papers(load_latex("papers/*.tex"), "_site/papers");
    let sources=catalog([notes,papers],publication_options(#{source_site_url:"https://your-site.example"}));
    output render_html(compose(sources,Fn("notebook")),"_site",html_config(#{
        theme:"themes/notebook",url:"https://your-site.example"
    }));
}
```

Sources may use a configured named root, such as `@papers/*.tex`, to keep authoring
files outside the website. Sources and bibliographies must remain within that
root, including after resolving symbolic links.

`compile_papers(source, output)` uses `/papers` as the public PDF URL prefix.
For another location use `compile_papers(source, "public/downloads", "/downloads")`.
The filesystem destination must be inside the website's declared output root.
Production validation checks PDF links, including GitHub Pages subpaths.

## Write and save

Use a literal ISO date, `\date{2026-09-09}`, for publication metadata. An optional
single JSON comment sets a stable ID and can override the publication date
independently of the typeset date:

```tex
% berlin: {"id":"cost-model","published":"2026-09-09"}
\date{September 9, 2026}
```

Without an explicit ID, Berlin derives one from the source's logical path and
named root. Set an ID before renaming a published source to retain its identity.
Dates are never inferred from build time.

`\bibliography{references,more}` loads both local `.bib` files. No bibliography
setting is needed in the pipeline. Literal `\input{sections/model}` and
`\include{sections/results}` load `.tex` files; all relative paths are resolved
from the main document's directory, including paths inside included files.
Select **main documents** in `load_latex`, not a recursive glob that also selects
their included sections. Cycles, missing files and paths escaping the declared
source root fail before compilation. Includes are flattened for both editions;
`\includeonly` and separate compilation units are not supported.

## Build and watch

Build `bln` normally. The optional `nix develop .#papers` shell supplies the pinned
TeX, BibTeX, Pandoc and PDF-inspection tools. From a configured website project,
with Berlin and these tools on PATH:

```sh
bln build --allow-tex --production
bln serve --watch --allow-tex
bln release --allow-tex
```

As with other website releases, `release` requires a configured deployment
destination (see [website releases](website-releases.md)); it seals files locally
and does not publish them.

The watcher observes source directories, including new files and bibliography
edits. Authorization applies to that build or watch session, not to a permission
embedded in a downloaded pipeline configuration.

`--allow-tex` is an explicit execution decision, **not a sandbox**. Only use sources,
bibliographies and installed TeX packages you trust. Compilation uses
`-no-shell-escape` and restrictive TeX file-access settings; these do not provide
machine isolation. Pipelines without paper compilation need neither this flag
nor the TeX toolchain.

Compiler scratch files are cleaned up automatically. Sources are not modified;
no auxiliary files appear beside them. PDF timestamps use a fixed reproducibility
epoch. Build receipts record consumed source and bibliography hashes. Only
rendered website files and PDFs are published, not the source or bibliography.

Each compiler/converter invocation has a 120-second deadline and a combined
16 MiB stdout/stderr limit; toolchain probes have 10-second/64 KiB limits. On
Unix, interruption, timeout and completion terminate the owned process group
before pipe readers are joined. This prevents ordinary descendants from keeping
builds alive. It is not isolation against deliberately escaping processes, nor a
CPU, memory or disk quota. Windows descendant cleanup is not implemented; the
supported paper execution workflow is currently Unix with the Nix toolchain.

HTML and PDFs share the website output transaction: conversion or rendering
failure leaves the previous website intact.

## Reusing unchanged papers

Paper results are cached per source under `.berlin/cache/artifacts-v1/`. A key
covers the main source, expanded includes, class, bibliography and image bytes,
publication identity and date, source
URI, PDF URL prefix, Berlin executable, and verified toolchain identity. Updating
one paper does not invalidate the others; editing templates does not recompile
unchanged papers. Logs distinguish `Compiling paper` from `Reusing paper`.

Reuse currently requires the pinned Nix paper toolchain. Berlin checks executable
paths and versions, TeX search roots, the format and bibliography style, and the
compiler's recorded inputs. Mutable toolchains, populated user TeX trees, custom
search overrides, or failed verification disable reuse and compile normally.
Toolchain verification still runs on a warm build; `--allow-tex` is still required.

Cache entries contain the semantic document, PDF and public figure assets, with integrity checks.
Malformed, incomplete or missing entries are cache misses. Cached documents are
validated again, and their PDFs are copied into the current output transaction.
Cache files are disposable local build state, not publication artifacts or
authenticated packages. They must not be supplied by an untrusted party.

Use `--no-cache` with `build`, `serve`, or `release` to bypass both cache reads and
writes. Removing `.berlin/cache/artifacts-v1/` also forces recompilation without
removing authoring sources or published output. Entries are not automatically
evicted yet.

This is operation-result reuse, not a graph-wide incremental scheduler. The
pipeline still runs. CSS also uses the [operation cache](build-caching.md), while
website assembly and template rendering execute each run.

`build --dry-run` checks source metadata and bibliography discovery without
executing TeX or writing output. It cannot establish that a paper compiles.
`bln check` remains read-only and does not execute compilation pipelines.
`documents(load_data(...))` remains available for independently prepared
semantic JSON. `merge_documents` validates combined IDs in either workflow.

## Supported paper profile

- Standard `article`, `report` and `book` classes. Local `.cls` wrappers may load
  one of these classes, supported packages, and literal command definitions;
  arbitrary publisher class logic is rejected. Standard class differences still
  apply: for example, `book` does not define an `abstract` environment.
  Berlin supplies `hyperref` if it is absent. Hyperref's
  `hidelinks` and `pdfusetitle` options are supported. The latter also supplies
  title/author metadata to the PDF. Basic font-size and paper-size class options
  are accepted. Supported packages are `amsmath`, `amssymb`, `amsfonts`, `graphicx`,
  `tikz` and `hyperref`; arbitrary package/preamble behavior is not supported.
- Literal `\newcommand`, `\providecommand` and `\renewcommand`, including
  positional arguments and optional defaults. Pandoc expands these for HTML;
  TeX expands them for PDF. Structural and file-access commands cannot be hidden
  in or redefined by these macros. This is not a general macro compatibility layer.
- Title, authors, abstract, paragraphs, emphasis, ordinary lists, links,
  footnotes, and labeled chapters/sections through `subsubsection`.
- Inline math and supported display math; each numbered `equation` needs one
  `\label{...}`. Use `aligned` within an equation, or unnumbered `align*`, for
  multiple rows. Independently numbered alignment rows are not supported.
- `\ref` and `\eqref`, using the numbers emitted by the same PDF compilation.
  Every compiled label must survive conversion. Every section needs a label.
- Simple rectangular tables with inline cells, captions and labels. Row/column
  spans, fixed-width columns and nested block cells are rejected.
- BibTeX `plain`, `unsrt`, `abbrv` and `alpha`, `\cite{...}`, and compiled bibliography
  text. No second citation style is applied to the web edition. Citation
  prefixes/locators are retained. Author-year styles, BibLaTeX, natbib and custom
  `.bst` files are not supported.
- Captioned, labeled figures containing one PNG, JPEG, PDF or inline TikZ picture.
  Raster files retain their bytes; PDF figures use their first page as SVG.
  TikZ is compiled for the PDF and separately to SVG for HTML, with declared
  libraries and validated macros. The caption supplies descriptive alternative
  text when no separate image description is available. Provide a substantive
  caption; complex diagrams still need an explanation in the prose.
  `width`, `height`, `scale` and `keepaspectratio` are accepted; HTML sizing follows
  the reading surface rather than reproducing TeX dimensions. Cropping, rotation,
  page selection, subfigures and externalized TikZ are not supported.

The abstract's first paragraph supplies the notebook description as plain
text; papers without abstracts receive an empty description. It is not an
AI-generated summary. The full abstract remains in the document body. Publication
identity and date follow the source metadata rules above.

The web edition uses semantic sections, equations, cross-references, lists and
tables—not an embedded screenshot or raw HTML paper. Table and bibliography
groups use the existing semantic component type. Typography, floating-table
placement, page breaks and line breaks need not reproduce the printed layout.
The PDF is the print edition; Berlin does not claim universal LaTeX fidelity or
produce a tagged accessible PDF. Review both outputs before publishing.

Unsupported AST nodes, raw TeX, unpreserved labels, invalid math and unsupported
source features fail explicitly. The converter targets Pandoc API `1.23.1.1`
from the pinned paper shell. It deliberately reads Pandoc's JSON representation:
[Pandoc documents that raw inline TeX can disappear in other output formats](https://pandoc.org/MANUAL.html#extension-raw_tex).

The source closure is bounded to 64 MiB/256 files, includes to 32 levels, and the
expanded document to 1 MiB. Each image and generated SVG is limited to 32 MiB;
one paper may contain at most 32 TikZ pictures and 64 MiB of generated assets.
Existing [Org-authored TikZ diagrams](diagrams.md) and ordinary PDF attachments
remain separate supported workflows.

## Verification

The [synthetic paper](../support/fixtures/papers/paper.tex) and its explicitly
fictional bibliography exercise equations, section/table references, a footnote
and a bibliography note. Frozen AST fixtures allow normal tests to run without
TeX. The [advanced synthetic paper](../support/fixtures/papers/advanced/paper.tex)
adds a local class, an included section, math macros, a captioned TikZ graph and
two bibliography files. Integration tests also cover standard classes, all four
styles, raster/PDF figures, cache invalidation and live rebuilding. Toolchain
tests are opt-in:

```sh
nix develop .#papers --command cargo test -p berlin --test papers -- --ignored
```

The browser/PDF preview work uses temporary synthetic projects, not personal
content or a remote publication destination.
