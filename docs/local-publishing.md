# Tool repository and local publishing projects

Berlin does not bundle the author's personal website. Root-level `data/`,
`content/`, `pages/`, `css/`, `sass/`, `static/`, and
`berlin.pipeline.rhai` are ignored for local authoring. Generated `_site/` and
`.berlin/` remain ignored too. Ignoring a path does not remove earlier commits
or already tracked files.

A small, invented project lives in `support/fixtures/publishing/`. Tests use it
instead of depending on a personal site. It is a functional fixture, not a theme.

To try it without overwriting an existing site, create a new directory and copy
the fixture there:

```sh
cargo build --bin bln
project=$(mktemp -d)
cp -R support/fixtures/publishing/. "$project/"
mkdir -p "$project/support"
cp -R support/ox-hugo "$project/support/"
nix develop --command env BERLIN_DIR="$project" target/debug/bln build --pipeline org
BERLIN_DIR="$project" target/debug/bln build --pipeline site
BERLIN_DIR="$project" target/debug/bln build --pipeline linkedin
```

The Org producer writes Markdown and copied assets to `.berlin/generated/org/`.
Run `build --pipeline org` again after source edits; `site` and `linkedin` read
those generated inputs explicitly. `nix develop` supplies Emacs with ox-hugo.
`support/check` exercises this workflow with a fresh temporary project.

The optional `support/web/code-controls.js` enables copying code listings.
Copy it into your own static assets and load it with a deferred script tag.
The renderer's listing and line links work without JavaScript; listing styling
belongs to the host website.

Before publishing a repository, inspect both its tree and its outgoing history.
Do not use `git push --all` or `git push --mirror` when local branches contain
unpublished content.

## Keeping a project inside the tool checkout

Use an ignored subdirectory such as `local/personal-site/` for a complete
publishing project. Keep its pipeline, content, templates, assets, styling
configuration and project-specific tests together.
The root `/local/` ignore rule excludes the whole project from Berlin's repository.

Run it from Berlin's root:

```sh
BERLIN_THEME_DIR="$PWD/themes" BERLIN_DIR="$PWD/local/personal-site" cargo run --bin bln -- serve --watch
```

The Org operation uses Berlin's bundled Ox-Hugo adapter. An existing project-local
`support/ox-hugo/export.el` remains an optional override, not a required copy.
`serve --watch` injects its reload client into HTTP responses only; ordinary
builds and files on disk contain no development script. Preview responses use
`Cache-Control: no-store` and `X-Robots-Tag: noindex, nofollow`.

`_site` is the conventional website output, not a server requirement. The output
argument of `render_website` may name another confined project-relative directory:

```rhai
output render_website(website, "dist", presentation);
```

Keep any attachment-copy destinations beneath that same root. `serve` and
`serve --watch` select the output of the single website renderer in the `site`
pipeline. Missing or multiple website renderers are rejected rather than guessed.
Restart the preview server after changing the output path. Previous output trees
are not deleted automatically; ignore the new directory in version control.

Inside that project, ignore generated `_site/`, `.berlin/`, transaction files,
and `node_modules/`. Keep authored sources and presentation files available for
versioning when the project becomes its own repository. Optional GitHub Pages
destinations are declared in the pipeline, but publication requires the separate
[release and publish commands](website-releases.md).

If you use those commands, `.berlin/releases` and `.berlin/publications` are
durable state, not disposable build caches. Keep them together in backups and
use one authoritative publishing machine or persisted CI workspace.
