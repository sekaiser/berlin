# Build caching

Berlin reuses selected operation results across builds, watch sessions and
releases. Cache entries live under `.berlin/cache/artifacts-v1/`. They are local,
disposable build state; published outputs never depend on serving that directory.

## CSS

Both `compile_css` operations and theme `styles/entries/*.css` entry points reuse
compiled stylesheets when their dependencies are unchanged. Lightning CSS itself
reports the files and imports it reads, including nested imports and CSS-module
dependencies. Berlin does not maintain a separate CSS parser for cache discovery.

Before reuse, Berlin rechecks recorded file contents and import resolutions.
Changing, adding or removing an import rebuilds its consumers. Retargeting a
symlink cannot retain the old resolved dependency. Missing imports still fail the
build, leaving previously published output intact.

Theme dependencies use logical paths within the merged stylesheet workspace, so
a fresh temporary workspace does not invalidate the cache. Adding or removing a
project override changes the effective input. Unrelated stylesheet files do not
invalidate entry points that never imported them.

The compiler mode and Berlin implementation identity are part of cache keys.
Theme path-confinement checks still apply on cache hits. Ordinary CSS that reads
outside its project or named source root remains compilable under its existing
rules, but is not cached. Remote imports preserved by the theme compiler remain
browser imports; they are not fetched or cached by Berlin.

Image/font URL bytes are not compilation dependencies when the compiler merely
preserves their URLs. Asset copying and production URL versioning still run with
the current assets. Template and article edits produce fresh HTML while reusing
unaffected compiled CSS.

## Papers

[LaTeX papers](papers.md#reusing-unchanged-papers) use per-paper source,
bibliography, metadata, URL and toolchain keys. Reuse requires a verified immutable
Nix toolchain; other toolchains compile normally. `--allow-tex` remains required.

## Control and boundaries

Standalone [LaTeX graphics](diagrams.md#cache-trust-and-limits) share the operation
cache across editor, CLI and pipeline entry points. Their keys cover all declared
resources, including plot data, fonts and preambles.

- `--no-cache` on `build`, `serve`, `release` or `render-diagram` bypasses cache reads and writes.
- `build --dry-run` does not populate the cache.
- Logs report `Compiling CSS` / `Reusing CSS` and corresponding paper operations.
- Corrupt, incomplete or missing entries cause recomputation. Cache write failures
  do not replace a successful computation with a failed build.
- Cached bytes still enter the current output transaction. A later failure leaves
  the previous published tree intact, just as with a fresh computation.
- Removing the cache directory does not remove sources or published output.
  Automatic eviction is not implemented. Do not import caches from untrusted parties.

This is operation-result reuse, not graph-wide incremental execution. Website
assembly and Tera rendering still run. Tera can use dynamic includes and functions
such as `now()` and `get_random()`; unchanged template files alone are not a safe
rendered-page cache key.
