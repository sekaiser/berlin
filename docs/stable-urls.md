# Stable article URLs

An article's content ID, title and public URL have different responsibilities.
Keep the ID for references, change the title as the writing develops, and use an
explicit `slug` to keep the public URL unchanged:

```yaml
id: 914af83d-80c9-49a8-89f3-8c4302a3cb94
title: Publishing from Org
slug: publishing-from-org
```

Berlin publishes this document at `notes/publishing-from-org.html`, relative to
the configured website URL. Renaming its source file or changing its title does
not change this route. Documents without `slug` retain title-derived URLs; their
URLs can still change when their titles change. To pin an existing URL, copy its
current filename without `.html` into `slug` before changing the title.

Explicit slugs contain lowercase ASCII letters and digits, optionally separated
by single hyphens. They are exact identifiers, not text Berlin silently rewrites.
Empty values, spaces, uppercase letters, paths and repeated hyphens are rejected
by the website build. Article titles remain required.

## Org authoring

Use ox-hugo's native file-level property:

```org
#+TITLE: Publishing from Org
#+HUGO_SLUG: publishing-from-org
```

Run the Org export pipeline before building the website. The slug does not change
the export adapter's Markdown filename; Berlin reads it from the front matter.
Keep the file-level Org ID unchanged.

## Deliberately moving a published article

If a URL must change, list all previously published slugs alongside the new one:

```yaml
slug: publishing-from-org
previous_slugs: [org-publishing, my-publishing-workflow]
```

In an Org file, add the history to its existing custom front matter:

```org
#+HUGO_SLUG: publishing-from-org
#+HUGO_CUSTOM_FRONT_MATTER: :previous_slugs '("org-publishing" "my-publishing-workflow")
```

Every previous slug produces a small HTML redirect directly to the current URL.
Keep older entries when moving again; Berlin does not infer publication history.
An explicit current slug is required when declaring previous slugs. Self
redirects, duplicate entries and collisions with another article or redirect
fail the build before staged output replaces the previous website.

Redirects work on static hosting, including GitHub Pages. They are HTML pages
served with a normal success response, **not HTTP 301 or 308 redirects**. They
provide a canonical link, `noindex, follow`, a JavaScript redirect, a meta-refresh
fallback and an ordinary link. JavaScript preserves the incoming query and
fragment. Without JavaScript, the fallback reaches the article but does not
explicitly preserve those parts. Keep old heading IDs if incoming deep links
must continue to work; a redirect cannot restore a removed section.

Set the website `url` to the deployed origin and optional repository prefix for
absolute canonical destinations. Without a configured URL, redirects use a
same-directory destination.

## Derived links and publication scope

Document ID links, backlinks, article lists, tag pages and search use the current
route. Previous URLs are not additional documents or search results. Drafts
produce neither public pages nor redirects. Existing literal URL links remain
literal links and may follow a redirect; prefer ID links between authored notes.

These fields control article routes under `notes/`, not tag or collection URLs.
Route validation runs during website rendering, not the narrower `bln check`
authoring report. A deployment tool generating a sitemap should omit redirect
pages, identifiable by their `berlin-redirect` metadata, and list canonical
articles only.
