# RSS subscriptions

Enable RSS on a multipage website through its existing presentation settings:

```rhai
pipeline site {
    output render_html(compose(content, notebook()), "_site", html_config(#{
        theme: "@notebook",
        url: "https://example.com",
        title: "My engineering notebook",
        description: "Notes about building software",
        rss: true
    }));
}
```

RSS is disabled by default. When enabled, Berlin writes `rss.xml` at the website
root. The notebook theme adds an RSS subscription icon to the footer and an
alternate-feed link in each page's HTML head, so feed readers can discover it
from the website URL. A site hosted below `/notebook/` uses
`https://example.com/notebook/rss.xml`. Custom themes can use the optional
`rss_url` template context value for the same links.

## What enters the feed

Entries come from the resolved website's local document pages. Drafts, unselected
documents, external reading-list resources, redirects and composed collection
pages do not become entries. Repeated cards or excerpts do not duplicate an item.
A local guide with its own published document page is eligible too.

Each item contains its title and absolute article URL, an opaque stable ID, and
the available authored metadata: publication date, summary, authors and tags.
Descriptions are plain-text summaries encoded for RSS readers, not full article
bodies or executable HTML. Missing summaries or dates are omitted rather than
invented. RSS generation does not fetch external resources.

## Identity and dates

Entry IDs are derived from the public website base URL and document identity.
Changing a title, slug, body, theme or modified date leaves the ID intact. Keep
explicit document IDs, such as Org-roam IDs, when moving source files; a changed
document identity or public website base URL creates a different feed identity.
The feed never exposes a private source path as an ID.

Items sort by authored publication date, newest first, with document identity as
a deterministic tie-breaker. Undated entries follow dated entries. Berlin stores
calendar dates, so RSS represents them at midnight UTC on that authored day.
RSS publication dates must have a year of at least 1900.

Rebuilding does not stamp items with the current time. This first version offers
subscriptions to published work; it does not announce every edit as a new post
or implement a separate update-notification policy. Reader behavior when an
existing entry changes varies by application.

## Build and release behavior

The feed shares the website's output budget, staged installation, input
fingerprint, receipt and sealed-release inventory. Invalid XML metadata, a
conflicting `rss.xml` output path, or an exceeded output budget fails the build
before replacing the installed website. Disabling RSS removes the old generated
feed on the next successful build.

Production URL preparation keeps subscription and discovery links stable:
`rss.xml` does not receive an asset cache-busting query string. Other assets keep
their normal versioned URLs. The production configuration must use the public
website URL, as it does for canonical article links.

RSS applies to multipage website compositions, including `notebook()`. Enabling
it on a single-publication render is an error rather than an ignored setting.
