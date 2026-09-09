# LaTeX graphics

Berlin renders standalone artwork with the actual LaTeX compiler and converts
its single-page PDF to SVG. Unlike semantic paper conversion, this path does not
apply a package whitelist: drawings can use installed packages, macros, fonts,
shared preambles and declared data files. Readers need neither TeX nor JavaScript.
The Emacs helper, standalone CLI, `compile_diagrams` operation and paper TikZ
figures share the same graphics compiler/converter. Paper prose still follows
the separate [paper profile](papers.md).

## Toolchains

Build Berlin in the default development shell. `nix develop .#diagrams` supplies
Emacs/ox-hugo, pdfLaTeX, LuaLaTeX, TikZ/PGF, PGFPlots, fontspec/Latin Modern,
standalone/preview, dvisvgm and Poppler. The `papers` shell includes the same
graphics toolchain plus Pandoc. Ordinary builds consuming saved images need none
of these graphics tools.

pdfLaTeX is the default; select LuaLaTeX for Lua-based graph layouts or fontspec.
Missing packages fail explicitly. XeLaTeX, PSTricks and shell-escape-dependent
workflows are not configured or verified. Broad package support is not a promise
of universal LaTeX compatibility.

## Author in Org

```elisp
(setq berlin-executable "/path/to/berlin/target/debug/bln")
(load "/path/to/berlin/support/emacs/berlin-diagrams.el")
```

Emacs needs the graphics tools on its PATH/exec-path. An already-running Emacs
does not inherit a newly opened Nix shell.

```org
#+CAPTION: Input is transformed into output.
#+ATTR_HTML: :alt Two boxes labelled Input and Output connected by an arrow.
#+name: transformation
#+begin_src latex :file diagram.svg :results file graphics :exports results :eval never-export :headers '("\\usepackage{tikz}")
\begin{tikzpicture}
  \node[draw] (a) at (0,0) {Input};
  \node[draw] (b) at (3,0) {Output};
  \draw[->] (a) -- (b);
\end{tikzpicture}
#+end_src
```

Run `M-x berlin-render-diagram` and confirm trust. Org expands variables/noweb
and inserts the normal `#+RESULTS` link; Berlin compiles, caches and atomically
replaces the SVG. Review and save deliberately: the helper does not save or
publish. Failed rendering retains the previous image. `C-c C-c` remains Org's
ordinary executor; global Babel/compiler settings are unchanged.

Optional headers are `:berlin-engine lualatex` and
`:berlin-inputs graphics-inputs`. The latter declares a resource directory whose
relative layout is reproduced in the compiler workspace. For a shared preamble,
use `"\\input{preamble.tex}"` in `:headers`. Keep generated images outside that
directory. Paths are relative to the Org file; remote paths and `:dir` are
rejected. Create the output directory first.

### Code and saved results

Berlin honors literal `:exports code`, `results`, `both` and `none`, including
inherited header properties and configured language defaults. Publication never
executes Babel, resolves variables, expands noweb, runs sessions/post-processors,
or evaluates Lisp headers. Executable `:exports` expressions are rejected.
This contract covers source blocks, not full inline Babel/call-block parity.

Result-only export keeps stored tables/images/text and transfers the source's
name, caption and HTML attributes when the result has no own value. Missing
results remain absent; exporting does not generate them or establish freshness.
Use a results drawer or block for multi-block raw output so its boundaries are
explicit; unmarked paragraphs cannot reliably be distinguished from following prose.
A `noexport` heading is useful for private working material, but is not needed
merely to hide the generating source. Saved images follow the normal
content-addressed attachment pipeline; copy its exported attachments as usual.

## Pipelines and CLI

For an optional visual starting point, the notebook theme provides a
[PGFPlots line-chart style](../themes/notebook/authoring/pgfplots/README.md).
Copy it into your declared inputs and select `notebook axis` in the drawing.
Theme selection does not inject TeX styles or recolor saved images.

```rhai
pipeline diagrams {
    output compile_diagrams(
        load_data("graphics/*.tex").named("diagram_sources"),
        load_assets("graphics/inputs/**/*"),
        "_site/diagrams",
        "lualatex"
    );
}
```

The engine argument defaults to `pdflatex`. Select complete standalone documents,
not included preambles, as sources. `plot.tex` produces `plot.svg`; names allow
ASCII letters, digits, underscores and hyphens, and case-insensitive collisions
fail. Only SVGs are published, never source, input data, fonts or scratch files.
Keep the destination below the website output root for a shared atomic commit.

Use `bln build --pipeline diagrams --allow-tex`. Build, watch and release require
explicit permission; a dry-run snapshots/checks inputs without invoking tools
or cache. Assets are explicit so TeX can construct filenames without Berlin
having to reverse-engineer arbitrary macros.

Outside a website project:

```sh
bln render-diagram graphics/plot.tex --input-root graphics/inputs \
  --output plot.svg --allow-tex
```

`--engine lualatex`, `--no-cache` and `--cache-root DIRECTORY` are available.
The cache root defaults to the output directory; the editor helper uses the Org
directory. CLI directory snapshots skip hidden files and reject symlinks.
Pipeline resources are precisely those selected by `load_assets`, confined to
its root. Output must stay outside a CLI input directory to avoid feedback.

## Cache, trust and limits

SVG keys cover source text, engine, every declared resource path/hash, Berlin
and verified toolchain identity. Any declared resource change invalidates reuse.
Editor, CLI and pipelines share `.berlin/cache/artifacts-v1/`. Mutable toolchains
or unverified compiler reads disable reuse; `--no-cache` bypasses reads and writes.
Cache corruption is a miss, and failed compilation preserves published output.

Limits: 1 MiB source; 256 resources/64 MiB total; 32 MiB per resource/SVG; directory
depth 32; one PDF page per graphic. External processes use the shared 120-second
deadline, 16 MiB combined stdout/stderr bound and Unix process-group cleanup.

This is **trusted local execution, not a sandbox**. Shell escape is disabled.
pdfLaTeX uses paranoid Kpathsea read/write settings; LuaLaTeX requires restricted
reads so its font loader can open installed files by absolute path. Its font
cache lives in the temporary workspace, with paranoid output settings retained.
Neither setting isolates TeX/Lua from the host. Only run trusted documents,
packages, fonts and Babel headers; hostile documents require external isolation.

SVG glyph outlines preserve appearance, not selectable text or equation semantics.
Supply meaningful captions/alternative text and explain complex figures in prose.
Use [typed mathematics](mathematics.md) for semantic equations. SVG attachments
are not sanitized merely by passing through Berlin.

## Checks

The [Org gallery](../support/fixtures/diagrams/gallery.org) covers architecture,
state machines and mathematical drawings. [Standalone specimens](../support/fixtures/graphics/)
cover PGFPlots with CSV/shared preambles and LuaLaTeX graphdrawing with fonts.

```sh
nix develop --command cargo build --bin bln
nix develop .#papers --command cargo test -p berlin --test graphics -- --include-ignored
nix develop .#diagrams --command emacs --batch \
  --load support/emacs/diagram-integration.el --funcall ert-run-tests-batch-and-exit
```

Tests use synthetic temporary projects, never personal content. `support/check`
also exercises safe export modes, stable IDs, attachment inheritance and private
content exclusion without requiring TeX.
