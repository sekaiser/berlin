# Diagram specimen

`notebook-plot.svg` uses the opt-in style and example in
[`authoring/pgfplots`](../../../authoring/pgfplots/README.md), with the same four
points as the original plot. The notebook drawing uses filled circles, a moss
line, dark labels and a quiet horizontal grid. Its label says “Value” because
these are synthetic values, not measurements. Ticks, ranges and dimensions are
chosen in the example, not imposed by the style.

`sample-plot.svg` is a saved rendering of Berlin's existing synthetic
`support/fixtures/graphics/plot.tex`, using `inputs/preamble.tex` and
`inputs/values.csv`. The four input points are (0,0), (1,1), (2,4), (3,9).
The plot is not a performance measurement or a fitted model.

Regenerate the notebook variant deliberately from the repository root:

```sh
nix develop .#diagrams --command target/debug/bln render-diagram \
  themes/notebook/authoring/pgfplots/plot.tex \
  --input-root themes/notebook/authoring/pgfplots \
  --output /absolute/path/to/notebook-plot.svg --allow-tex
```

Regenerate the original comparison rendering:

```sh
nix develop .#diagrams --command target/debug/bln render-diagram \
  support/fixtures/graphics/plot.tex \
  --input-root support/fixtures/graphics/inputs \
  --output /absolute/path/to/sample-plot.svg --allow-tex
```

Both saved SVGs use outlined TeX glyphs. Supply alt text and a prose explanation;
the glyphs are not selectable text. Theme builds copy the file without running
TeX. The authoring example expects `notebook-plot.svg` beside `diagrams.org`; ordinary Org
attachment publication gives it a content-addressed URL.
