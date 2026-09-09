# Notebook plots

An opt-in style for two-dimensional PGFPlots line charts. Loading the file defines
named colors and `notebook axis`; it does not modify global axis defaults.
It is authoring support, not part of website theme resolution or automatic export.

Copy `notebook-pgfplots.tex` into your drawing's declared input directory:

```tex
\input{notebook-pgfplots.tex}
\pgfplotsset{compat=1.18}

\begin{tikzpicture}
\begin{axis}[notebook axis, xlabel={Input}, ylabel={Value}]
  \addplot table[x=x,y=y,col sep=comma] {values.csv};
\end{axis}
\end{tikzpicture}
```

The style uses notebook ink for labels and axes, a quiet horizontal grid, and a
three-series cycle: moss circles/solid, terracotta squares/dashed, blue
triangles/dotted. Labels and markers remain precise. It does not choose ranges,
ticks, transformations, smoothing, units, or which series deserves emphasis.
Those decisions belong in each drawing. Label all series when using a legend;
choose a legend position that does not cover the data.
For more than three, define additional distinct encodings rather than relying
on the cycle repeating.

Set options after `notebook axis` to override it, and use `\addplot+[color=...,
mark=...]` for an explicit series override. Keep meaningful domain colors, such
as a documented threshold or category mapping. Never recolor an existing SVG
to make it match. PGFPlots' [named styles](https://tikz.dev/pgfplots/reference-styleoptions)
and [cycle lists](https://tikz.dev/pgfplots/reference-markers) provide these mechanisms.

Colors correspond to `shared/styles/tokens.css`; tests guard that correspondence.
A CSS override cannot recolor a saved SVG: update the authoring palette deliberately
and render again. Text uses the document's LaTeX fonts with 11pt ticks/legends and
12pt labels, not the website's Fraunces webfont. No additional font package is needed.
When shrinking a figure, check it at its actual displayed size; a large or dense
plot may still require full-size viewing. This is not a general heatmap, 3D, or
scientific color-map palette.

## Render the example

From the Berlin repository root:

```sh
nix develop .#diagrams --command target/debug/bln render-diagram \
  themes/notebook/authoring/pgfplots/plot.tex \
  --input-root themes/notebook/authoring/pgfplots \
  --output /absolute/path/outside-the-input-directory/notebook-plot.svg \
  --allow-tex
```

`values.csv` duplicates the four points in the existing synthetic graphics fixture;
a test verifies they remain identical. The example's explicit ticks and ranges
are not imposed by the style. The saved website asset is
`example/static/diagrams/notebook-plot.svg`. `sample-plot.svg` remains the original
blue rendering for comparison. Normal website builds copy these files without TeX.

For Org, put the style and CSV in a `graphics-inputs` directory and use the existing
Berlin helper with `:berlin-inputs graphics-inputs` and
`:headers '("\\input{notebook-pgfplots.tex}" "\\pgfplotsset{compat=1.18}")`.
The block body contains the `tikzpicture` above. Render explicitly with
`M-x berlin-render-diagram`, review the saved result, then publish normally.
See [LaTeX graphics](../../../../docs/diagrams.md) for the complete block headers,
trust requirements, caching, and attachment behavior.

SVG output uses outlined glyphs, not selectable text. Supply meaningful alt text,
caption and a prose/data equivalent. Colors and line patterns do not replace labels.

## Verify

```sh
nix develop .#diagrams --command env BERLIN_TEST_TEX=1 \
  node --test themes/notebook/tests/pgfplots.test.cjs
```

The ordinary tests check palette correspondence, contrast and unchanged data.
The explicit TeX check compiles with pdfLaTeX and LuaLaTeX, compares an unselected
plot with its baseline, reproduces the saved SVG, and checks multiple series,
legends and an author's color override. Set `BERLIN_VALIDATION_DIR` to keep the
test renderings for visual inspection. It does not render personal diagrams.
