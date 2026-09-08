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

Use the current `support/ox-hugo/export.el` in the project's export adapter.
Exports record local-only origins in `_berlin/org-origins/`; these files are not
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
