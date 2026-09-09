# Mathematics

Berlin preserves TeX equations as typed inline and display math. The website
renderer produces native MathML at build time: no JavaScript, CDN, Node runtime,
or TeX installation is required to typeset equations. This also works in static
GitHub Pages output.

## Authoring in Org

Use ordinary Org math, with Berlin's batch ox-hugo exporter:

```org
The indexed observation is \(x_i\).

\[
\bar{x} = \frac{1}{n}\sum_{i=1}^{n} x_i
\]

\begin{align*}
2x + y &= 7 \\
x - y &= 2
\end{align*}
```

For multi-line expressions containing nested environments, use an outer
`equation*` environment so Org treats the entire expression as one unit:

```org
\begin{equation*}
\begin{bmatrix}2 & 1 \\ 1 & -1\end{bmatrix}
\begin{bmatrix}x \\ y\end{bmatrix}
= \begin{bmatrix}7 \\ 2\end{bmatrix}
\end{equation*}
```

Keep the default `tex:t` export option. `tex:nil` omits equations. Berlin rejects
image/verbatim TeX export modes rather than invoking an external TeX toolchain.
Source blocks and saved results follow literal `:exports` choices; publishing
does **not** execute Babel blocks or executable header values.

Berlin overrides ox-hugo's inherited Blackfriday equation escaping during export.
It preserves fragment delimiters and emits complete environments in `math`
fences. Existing Markdown exported with the older escaping must be re-exported;
Berlin does not guess which doubled backslashes or escaped underscores to undo.
If a project supplies its own `support/ox-hugo/export.el`, update that copy and
its companion `references.el` and `code-results.el` files too,
or remove the override deliberately to use Berlin's bundled exporter.

Run the Org pipeline, then its consuming website pipeline as usual. No pipeline
operation or website flag is necessary. A complete synthetic source is available
in [the mathematics fixture](../support/fixtures/mathematics/article.org).

## Authoring in Markdown

Inline `\(x_i\)` or `$x_i$`, display `\[...\]` or `$$...$$`, and fenced math are
recognized by the Markdown parser. Display math may stand alone or separate two
passages of prose, but cannot be nested inside emphasis, a heading, or a link
label. Inline math works in those contexts, lists, and table cells.

````markdown
The coefficient is \(\alpha_i\).

```math
\begin{aligned}
a &= b+c \\
d &= e
\end{aligned}
```
````

Code spans and ordinary code fences are never interpreted as math. A `latex`
fence shows source code; a `math` fence typesets an expression. Escape literal
dollar signs as `\$` when text might otherwise look like a math delimiter.

## Equation references and citations

An explicit label inside display math gives the equation a stable target.
In Org, write:

```org
See equation \eqref{eq:sum}.

\begin{equation}
x = a + b \label{eq:sum}
\end{equation}

#+bibliography: sources.bib
This follows [cite:@example p. 7].
```

The same `\label`, `\ref`, `\eqref` and `\cite{example}` syntax works in Markdown;
put an environment in a `math` fence. Labeled equations are numbered in document
order. `\eqref` adds parentheses; `\ref` uses the number alone. Forward references
work. Labels must be unique, references must resolve, and labels are only valid
inside display equations. References/citations belong in prose, not headings,
existing link labels or image descriptions. Use code spans to show literal commands.
Unlabeled, unstarred environments may still display the math engine's own numbers;
use starred environments when a formula does not need a reference.

For Markdown, declare bibliographic records in front matter:

```yaml
references:
  - id: example
    text: Example Author. A synthetic reference. 2026.
    url: https://notebook.example.org/reference
```

Use `\cite{example}` or `\cite{first,second}` in prose. Only cited entries appear,
in first-citation order, in a linked References section at the end. An optional
`label` replaces the numeric citation label. IDs use ASCII letters, digits,
periods, underscores, colons and hyphens. Bibliography links must use HTTP(S).

The bundled Org adapter reads local `#+bibliography` files and exports native
`[cite:@key]` citations with authored prefixes/locators. Its initial bibliography
profile uses literal BibTeX fields (author, title, journal, publisher, year, note,
URL). TeX commands, string expressions, inheritance and alternate citation styles
are rejected rather than approximated. `#+print_bibliography` does not duplicate
the final semantic bibliography. Do not also supply manual `references` metadata.
This does not require BibTeX or a TeX process; the richer paper workflow below does.

## Rendering and boundaries

- Fractions, roots, scripts, sums, common symbols, matrices, and aligned
  equations use [pulldown-latex 0.8.0](https://docs.rs/pulldown-latex/0.8.0/pulldown_latex/).
  This is a TeX **math subset**, not a full LaTeX implementation or MathJax.
- Invalid or unsupported expressions fail parsing with a source URI and line.
  Equations introduced through document transformations are checked again before
  returning HTML. Berlin does not publish error placeholders as successful math.
- Use self-contained expressions. There is no document preamble or shared macro
  registry. Local `\def` works; the current engine requires an explicit argument
  count for `\newcommand`, including `[0]` for a zero-argument command.
- One label identifies one displayed equation, including a multi-row `aligned`
  expression. Independently numbered/referenced rows are not represented.
- Source is limited to 16 KiB and 65,536 parser events per equation. The engine
  also bounds recursive macro expansion. It has no file or process access.
- Exact TeX is retained in the semantic document and escaped MathML annotations.
  Search indexes retain TeX; LinkedIn drafts use explicit `\(...\)` / `\[...\]`
  text instead of silently dropping equations. Those drafts are not typeset.

The HTML fragment includes a small stylesheet once when it contains math. It
uses installed math fonts (STIX Two Math, Cambria Math, Latin Modern Math, Noto
Sans Math, then the browser's `math` fallback); no font downloads are added.
Glyph coverage and layout can vary between systems. A theme can supply its own
licensed math font for consistent typography. Wide display equations have
keyboard-focusable horizontal scrolling without widening the surrounding prose.
Native MathML retains structure for browsers and assistive technology; a complete
screen-reader/browser compatibility audit has not been performed.

The structural stylesheet is adapted from pulldown-latex and retains its MIT
copyright and permission notice in the generated HTML.

## Numbered equations in semantic documents

The document API also has `Block::Equation(Equation { id, number, tex })` and
`Inline::CrossReference { target, content }`. These are structural values for
importers and Rust transformations, not additional Markdown or Org syntax.

An equation retains its source-assigned number, including forms such as `2.1`
or `3a`; rendering does not renumber it. Its TeX holds only the mathematical
expression. HTML displays the number beside the scrollable formula and links it
to the equation's stable anchor. A semantic equation owns one number, including
when its expression spans several rows; individually referenced rows are not
represented by this type.

Cross-references contain an explicit label and a target in the same document.
Validation checks forward references, missing targets, empty labels, unsafe
fragments, duplicate anchors, and nested links. Targets can be equations,
sections, explicitly identified headings, components, or named code anchors.
Ordinary `Inline::Link` URLs do not acquire this strict local-reference behavior.
The importer owns numbering and label consistency; validation does not infer
that arbitrary reference prose describes the correct equation number.

Search and text-only channel projections retain the equation source, number,
and reference labels. These semantics are the foundation for paper import;
the [paper importer](papers.md) uses them to retain compiled LaTeX references.
They do not make arbitrary TeX commands valid inside mathematical expressions.

The [numbered-equation example](../crates/bln-document-html/examples/numbered_equations.rs)
constructs these values and prints a standalone HTML renderer preview:

```sh
nix develop --command cargo run -q -p berlin_document_html --example numbered_equations
```

## Separate workflows

[TikZ/PGF diagrams](diagrams.md) can be compiled explicitly while authoring in
Emacs and published as saved SVG attachments. They do not pass through the math
renderer. PDF downloads can likewise be included as ordinary Org attachments.
The [pipeline paper workflow](papers.md) produces semantic notebook content,
figure assets and a PDF for its supported paper profile. Equation support does
not make an arbitrary LaTeX document convertible.
