# Emacs authoring checks

Load the repository's small editor adapter explicitly; it does not install
packages or change your Emacs configuration:

```elisp
(load "/path/to/berlin/support/emacs/berlin.el")
(setq berlin-executable "/path/to/berlin/target/debug/bln")
```

If `bln` is already on Emacs's `exec-path`, the default executable is sufficient.
Run `M-x berlin-check`, select your publishing project, then press `RET` on a
source link. `n` and `p` move between links; `g` repeats the check. A prefix
argument (`C-u M-x berlin-check`) prompts for a pipeline instead of using `site`.
The check runs asynchronously and reports broken references even when the CLI
returns exit code 1. Incomplete analysis is displayed separately from findings.

The command **never saves buffers, exports Org or evaluates Babel blocks**.
Save and export deliberately before checking your current edits:

```sh
BERLIN_DIR=/path/to/project bln build --pipeline org
BERLIN_DIR=/path/to/project bln check
```

Berlin bundles the default Ox-Hugo adapter. An optional project-local
`support/ox-hugo/export.el` overrides it; keep any intentional override updated.
Exports record local-only origins in `.berlin/org-origins/`; these files are not
needed for the deployed website. The report links to the original Org file and,
for a reference finding, its nearest mapped heading. A heading's Org ID takes
precedence over its outline path. Only unique matches are followed.

Changed files, unsaved buffers, missing or ambiguous headings fall back to the
file rather than guessing a location. Missing origin maps or removed Org files
fall back to exported Markdown. Links outside the selected local project are
rejected. This first adapter supports local files and heading navigation, not
TRAMP or exact source lines.

See [the check's scope and JSON contract](authoring-check.md). Optional editorial
observations are suggestions, not requirements to connect every note.

## Article previews

Preview artwork belongs with the article's attachments, not with the theme.
Declare it at file level in the Org source:

```org
#+BERLIN_PREVIEW: ../attachments/my-article/preview.svg
#+BERLIN_PREVIEW_ALT: A description of what the illustration conveys
#+BERLIN_PREVIEW_SIZE: 320 224
```

The path is relative to the Org file. Supply the image's intrinsic width and
height as positive integers; all three keywords are required when using a preview.
The exporter copies the asset through the same content-addressed attachment
pipeline as body images. Missing files fail the export. Preview metadata does
not insert an image into the article body or generate new artwork.

The site pipeline must publish the exported `static/attachments` directory to
`_site/attachments`, as in the publishing fixture. After exporting and rebuilding,
the notebook theme shows the declared preview on the homepage, notes index and
tag pages. Articles without previews remain text-only.

Markdown authors can supply the equivalent typed front matter directly (and
must arrange publication of the referenced asset):

```yaml
preview:
  source: /attachments/my-preview.svg
  alt: A description of the illustration
  width: 320
  height: 224
```
