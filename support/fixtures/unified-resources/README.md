# Unified resources synthetic project

The fixture exercises mixed catalog ingestion, selection, a multipage notebook,
a system-design portfolio, repeated local excerpts, and direct plain/LinkedIn
text. Run `support/check-unified-resources` from the Berlin checkout after building
`bln`. The runner copies this project into a fresh temporary directory.

`sources()` declares the original-site mount in catalog options. HTML pipelines
choose their destination separately; text uses the same original-resource URLs.
The site renderer owns its entire output directory. Its `.with_assets(...)`
declaration adds attachments to that transaction instead of using another writer
to overwrite part of the rendered site.

Every external record loaded into a publication catalog is publication-eligible.
Do not mix private bookmarks into these inputs. Metadata is authored, not fetched
or verified against a remote site. Reserved example URLs identify synthetic
resources; the repository link is an explicit public reference with no API calls.
