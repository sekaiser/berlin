# Tool repository and local publishing projects

Berlin does not bundle the author's personal website. Root-level `data/`,
`content/`, `pages/`, `css/`, `sass/`, `static/`, and
`berlin.pipeline.rhai`, along with `unocss.config.ts`, are ignored for local authoring. Generated `_site/` and
`_berlin/` remain ignored too. Ignoring a path does not remove earlier commits
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
BERLIN_DIR="$project" target/debug/bln build --pipeline site
BERLIN_DIR="$project" target/debug/bln build --pipeline linkedin
```

For Org export, use `nix develop` to make Emacs with ox-hugo available, then run
the same binary with `build --pipeline org` before building the site.
`support/check` exercises this workflow with a fresh temporary project.

The optional `support/web/code-controls.js` enables copying code listings.
Copy it into your own static assets and load it with a deferred script tag.
The renderer's listing and line links work without JavaScript; listing styling
belongs to the host website.

Before publishing a repository, inspect both its tree and its outgoing history.
Do not use `git push --all` or `git push --mirror` when local branches contain
unpublished content.
