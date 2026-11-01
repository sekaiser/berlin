# Metrics storage foundation

Berlin's SQLite lifecycle store has a typed persistence boundary for future
collectors. It does not collect engagement data, import exports, install website
tracking, authenticate to platforms, or provide a metrics dashboard. No platform
metric definitions or sample observations are installed in a project.

## Definitions and observations

A definition has a lowercase name, positive version, description, unit and kind.
Use namespaced names to distinguish providers' definitions. Its ID hashes the
serialized definition. Re-registering the same definition is idempotent; changing
an existing name/version fails. Change the version when the meaning, unit,
reporting scope or description changes. Collectors must describe the provider's
actual definition rather than assume that two platforms mean the same thing by
a label such as “views.”

Supported units are unsigned 64-bit counts, ratios between zero and one, and
nonnegative finite milliseconds. Counts never pass through floating-point storage.
JSON consumers must preserve integer precision when reading large count values.

The kinds distinguish lifetime totals, interval totals and gauges. Interval totals
require an explicit nonempty `[start,end)` window ending no later than the
observation time; the other kinds have no interval window. Ratio definitions must
be gauges. Times are UTC Unix seconds within the nonnegative SQLite integer range.

An observation records the exact definition ID, target reference, observation time,
optional reporting window, typed value and source. Berlin separately records when
it persisted the observation. Definitions and observations are append only through
the storage API. A later observation can be smaller or older: provider corrections,
delayed data and counter resets must not be silently discarded. Overlapping windows,
snapshots and different definitions are never automatically added together.

## Publication identity and article context

Website targets reference an article in an indexed sealed release. Berlin derives
the website URL using the existing release URL/route rules. The publication ID
hashes the article ID, channel, account and URL. Consequently, the same article URL
keeps one metric publication identity across releases, while a different route
gets a different identity. The release reference retains the context used to
resolve that identity; it does **not** assert that every observed view belongs to
that article revision. A known route is not proof that the release is live.

Social targets reference a recorded announcement review. An approval alone is
insufficient: there must be a user-reported publication URL. Its article ID,
channel, account and post URL determine the metric publication identity. The
existing announcement channel is LinkedIn. Association with a report does not
remotely verify either the post or its metrics.

This first contract supports publication-level observations associated with an
article. Revision-specific attribution and reliable cross-channel comparisons
would require additional evidence; they are not inferred from the stored context.

## Collector boundary

The Rust types and storage methods are in
[`cli/tasks/state/metrics.rs`](../cli/tasks/state/metrics.rs):

- `Definition`, `Target`, `Observation`, `Source`, `Value` and `Window` define the
  storage contract.
- `Store::register_metric` registers an immutable definition version.
- `Store::record_metric` validates and atomically appends an observation and its
  resolved publication identity.

These are crate-internal interfaces for later Berlin collectors, not a public
plugin API or an ingestion command. The two currently unused write methods are
explicitly retained for that boundary and exercised with synthetic tests.

Source provenance includes a stable collector namespace, a source record key, and
a declared collection method (`manual`, `api` or `file_import`). Those method names
describe evidence supplied by a future caller; none enables collection today.
Collector namespaces must include the relevant account/dataset scope, especially
where several analytics properties report on the same website URL.

Within one publication, definition and collector namespace, the source record key
is an idempotency key. Identical replay returns the existing observation ID without
another insert. Reusing that key for changed data fails without replacing history.
The entire request, including its target reference, participates in replay identity.
Fresh snapshots and corrections require a new source record key. Network work
must happen outside the storage transaction; callers supply the captured result.

The database stores compact, validated records and query projections. There are no
credential fields or raw-response storage. Existing backup/restore includes all
metric definitions, publication identities and observations.

## Inspection and migration

With the usual `BERLIN_DIR` selecting the project:

```sh
bln state metric-definitions
bln state metrics --document YOUR-ORG-ROAM-ID --limit 100
```

Both commands are read only, offline, and independent of pipeline execution. They
return empty collections when nothing has been recorded; missing data is never
represented as zero engagement. Results include `collectors_enabled: false`.
Observation queries also include `remote_verified: false`, a total and limit, and
sort by recording sequence, newest first. That order is not a freshness guarantee
or an assertion about the order in which events occurred. Queries return raw
observations, not rollups, trends or “latest” counters.

Article inspection also includes metric coverage:

```sh
bln state document YOUR-ORG-ROAM-ID
```

Its `metrics` block reports `no_observations` or `observations_recorded`, counts of
stored observations/publications/definitions, and the maximum stored observation
and recording timestamps. With no data, timestamps are `null`. These are storage
metadata counts, not engagement counters; no measurements are summed or averaged.
Coverage is associated with the article once, rather than repeated for every
release, so an unchanged URL across releases is not counted as several metric
publications. The latest timestamp does not establish freshness across providers.

Metric inspection verifies returned observation payloads and definition records;
it does not deserialize the complete observation history to return a limited
page. Responses identify their validation scope as `returned_observations` or
`returned_definitions`. Article coverage is marked `metadata_only`. Totals count
stored rows, not globally qualified measurements. A corrupt record outside the
selected page does not prevent reading a valid page. Use `bln state status` for
full database validation; backup and restore also validate the complete history
and refuse corrupt data.

Schema version 4 creates three empty tables: `metric_definitions`,
`metric_publications` and `metric_observations`. Use `bln state backup` before
`bln state migrate`. Upgrading an already indexed schema-3 project does not rescan
bundles, reimport archived receipts, or change article/publication history. Schema
1–3 backups remain inspectable and restorable; migrate before normal use. See
[lifecycle storage](lifecycle-state.md) for the recovery contract.
