# Article discussions

Article front matter can opt into comments:

```yaml
id: a-stable-document-id
comments: true
```

Omitting `comments` or setting it to `false` disables comments. It must be a YAML
boolean, not a quoted string. Keep the ID unchanged when renaming an article.
The semantic document retains this flag independently of the discussion provider.

For file-based Org export, add `#+HUGO_CUSTOM_FRONT_MATTER: :comments true` to the
Org source, alongside any existing custom fields. Berlin's exporter supplies the
file-level Org ID. See [ox-hugo's custom front matter documentation](https://ox-hugo.scripter.co/doc/custom-front-matter/).

## Website configuration

Pass an optional `giscus` map inside `website_config(...)`:

```rhai
giscus: #{
    repo: "OWNER/REPOSITORY",
    repo_id: "REPOSITORY_NODE_ID",
    category: "Article comments",
    category_id: "CATEGORY_NODE_ID"
}
```

Replace the placeholders with values from the [giscus configurator](https://giscus.app/).
It requires a public repository with Discussions enabled and the giscus app
installed. An Announcements-type category is recommended. IDs are public
identifiers, not access tokens; never place credentials in the pipeline.
Berlin validates configuration syntax, not repository access or installation.

## Template contract

The website renderer supplies these values to article templates:

| Value | Meaning |
| --- | --- |
| `page_comments` | Explicit article opt-in |
| `config_giscus` | Website-level repository, category and optional theme URL |
| `page_discussion_term` | `berlin:<document-id>`; present when enabled |
| `page_discussion_url` | GitHub discussion-search fallback; present when enabled |

Enabled articles require configuration and an explicit stable ID; otherwise the
build fails. Berlin does not inject a widget into arbitrary templates. A theme
must conditionally render its discussion section and client loader.

Use giscus's `specific` mapping with `page_discussion_term` and strict matching.
This keeps the discussion independent of the title, pathname and deployment URL.
The fallback is a search, not a guarantee that a discussion exists: giscus creates
one when the first comment is posted. Keep the mapped ID and repository stable.

Load the official client only after an explicit reader action, keep a GitHub link
available without JavaScript, and display a retry action if the script fails.
The provider owns authentication and the comment iframe. Loading it connects to
third-party services; commenting requires GitHub authorization. Disabling comments
in front matter hides the embed but does not delete the discussion on GitHub.

The optional `theme` field accepts an HTTPS stylesheet URL; omit it for the light
theme. A custom stylesheet must be publicly accessible with the cross-origin
permissions required by giscus. Parent-page CSS cannot style the iframe.
See [giscus advanced usage](https://github.com/giscus/giscus/blob/main/ADVANCED-USAGE.md)
for themes, strict matching and optional allowed-origin restrictions.
