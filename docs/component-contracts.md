# Publication component contracts

Component keys have the form `namespace.name@positive_version`. Keys and
contracts are registered explicitly and frozen before composition. Unknown
versions, fields, slots and unsupported channel capabilities are errors.

| Version-1 component | Properties | Slots |
| --- | --- | --- |
| `berlin.portfolio@1` | none | required `body` |
| `berlin.section@1` | `title: String` | required `body` |
| `berlin.introduction@1` | `text: String` | none |
| `berlin.featured_work@1` | `source: ContentRef`, optional `annotation` | none authored |
| `berlin.excerpt@1` | `source: ExcerptRef`, optional `annotation` | none authored |
| `berlin.related_work@1` | `sources: Vec<ContentRef>`, optional `title` | none |
| `berlin.callout@1` | `kind: note/warning/tip`, optional `title` | required `body` |

All seven have HTML and direct text interpretations. Required bodies contain at
least one child. Optional strings default to `None`, represented by `()` in
Rhai. Editorial strings are plain text, not HTML. Paragraph breaks in
introductions/annotations are preserved by their channel presentation.
Source descriptions are semantic Markdown, interpreted independently by each
renderer. Metadata cards do not embed the source article's body.

The semantic excerpt bridge recognizes source `Block::Component` callouts only
when they have the registered `berlin.callout@1` interpretation. It does not
reinterpret arbitrary legacy components by rendering only their children.
Portable excerpts reject raw HTML. The ordinary document renderer's historical
trusted-HTML behavior is unchanged outside this opt-in path.

## Identity and revisions

`ContentRef` names an original source by content ID. It may pin its source hash
or prepared-document digest. `ExcerptRef` adds an explicit section ID. An
occurrence key identifies one placement; using the same source twice is valid,
while duplicate sibling occurrence keys fail. Without explicit keys, positional
identity is deterministic but can change after insertion/reordering.

Publication metadata supplies a stable user-assigned ID, title and optional
description. Canonical UTF-8 JSON sorts object keys, preserves array order and
does not normalize Unicode or admit floats. Versioned SHA-256 domains separate
composition, occurrences, configuration and rendering. Manifests record original
source revisions once and placements separately. Public manifests omit local
source paths and unselected source content.

## Extend deliberately

The [derive](view-macro.md) creates a typed contract from property fields and
explicit slot declarations. `RegistryBuilder::register::<T>()` registers it;
`RhaiPipelineLoader::with_components` accepts that frozen registry for native
Rhai construction and final resolution. Scripts cannot register executable code.

HTML bindings belong to `berlin_publication_html::HtmlBindings`: map a versioned
key to a logical template and required CSS entries. Bindings are separate from
the channel-neutral schema. A channel declaration is not an implementation:
provide an actual renderer interpretation. The shipped text renderer handles
the seven built-ins and rejects other components explicitly, even when a custom
schema declares text support. There is no child-only fallback.

Duplicate registration fails. A registry can be assembled from the required
built-in types plus explicit extensions; no global inventory discovery is used.
Property types opt into `ComponentProperty`; arbitrary JSON serialization is not
accepted as a contract. Resource limits still apply to typed Rust trees.
