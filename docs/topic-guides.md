# Authored guides

A guide is an ordinary document with `kind: guide` in its Markdown front matter.
Other documents retain their existing default kind, `article`. An unknown kind
is a parsing error rather than a silently ignored label.

For file-level Org export through ox-hugo, use:

```org
:PROPERTIES:
:ID:       a-stable-document-id
:END:
#+title: Publishing from Org
#+HUGO_CUSTOM_FRONT_MATTER: :kind guide

An introduction explaining the subject and who should start here.

* Read first
[[id:another-stable-document-id][An existing note]] — why it belongs here.
```

Supply the usual author, description and publication date required by the
website renderer. Guides use the same routes, document validation, rendering
and reference index as other notes. Their order and explanatory prose are
authored; Berlin does not infer a reading sequence from tags.

Website templates receive:

- `guides` on the homepage and notes index: all published guides, ordered by
  publication date, independently of the homepage’s recent-note limit.
- `kind` on each article summary, including summaries on tag pages.
- `page_kind` on document pages.

Templates choose how to present these fields; no new page family is required.
Normal ID links produce contextual backlinks on their targets. Drafts do not
appear in guide discovery, and a published guide cannot link to an unpublished
document. See [document references](document-references.md).

Use [`bln check`](authoring-check.md) to inspect direct guide coverage without
publishing. Missing coverage is an editorial observation, not a build error.
