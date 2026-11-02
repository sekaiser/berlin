# Local lifecycle state

Berlin has an opt-in SQLite store for durable website publication state,
announcement approvals, user-reported social publications, build attempts,
catalog observations, sealed manifests, document revision history and the
foundation for metric observations. Runtime catalogs, output recovery and artifact
caches retain their existing execution model.
Source files and sealed release bundles remain files; their identities do not change.

## Activate a project

With the usual `BERLIN_DIR` selecting your publishing project:

```sh
bln state status
bln state migrate
bln state status
```

These commands need no pipeline evaluation, credentials or network access.
Status prints JSON. Migration serializes with website publishers and announcement
writers, validates the existing JSON records with their domain rules, and imports
all records into a private temporary database. It activates `.berlin/state.sqlite`
only after successful import and validation, using a no-clobber file installation.
An unsuccessful import leaves legacy records authoritative. Rerunning migration
validates the active database and upgrades older supported schemas transactionally.
Each schema imports its legacy records once; later migrations do not reimport them.

After activation, publication and announcement commands read and write SQLite.
Successful sealing also records the verified manifest and its document inventory.
`bln build` attempts append receipts to SQLite rather than replacing the JSON receipt file.
Existing JSON directories remain an untouched archive, never a second writer.
Do not run older Berlin binaries against a migrated project: they do not know
about this authority switch. New projects currently use JSON until explicitly
migrated. Migration does not approve, publish or refresh anything remotely.

The activation marker `.berlin/state-storage.json` prevents silently falling back
to stale JSON if the database is subsequently lost. Missing, corrupt or unsupported
state fails explicitly. Restore a verified backup; deleting the database is not a
supported rollback procedure. State maintenance shares the publisher lock without
running output transaction recovery or changing the website.

## Backup and restore

```sh
bln state backup /absolute/path/to/new-backup.sqlite
# On a destination project with no state.sqlite or recovery sidecars:
bln state restore /absolute/path/to/new-backup.sqlite
```

Backup uses SQLite's online backup API, validates the snapshot, and installs a
self-contained database without overwriting the destination. Restore validates
schema/application identity, SQLite integrity, foreign keys, record hashes, domain
records and build projections before installation. It refuses to overwrite an existing database
or discard recovery sidecars. Paths are project-relative unless absolute.

Keep database backups private. They contain exact approved text, account labels,
publication destinations and remote identifiers. Also preserve `.berlin/releases`
(and authoring sources) separately: a database backup does not include artifact
files. Do not copy only a live `state.sqlite` file, put an active database on a
network filesystem, or synchronize its WAL/SHM files between machines. Continue
using one authoritative publisher per project.

## Storage and recovery contract

Schema version 1 introduced separate approval, reported-publication, current website
publication and website-publication-history tables. Payloads retain the existing
versioned domain serialization, with SHA-256 checks; approval IDs and release IDs
are preserved. Foreign keys bind reports to approvals. Website status changes and
history entries commit in one transaction. Imported website records provide the
history starting point; earlier transitions cannot be reconstructed.

Schema version 2 adds append-only `bln build` runs, input/output hashes, diagnostics and
per-run catalog observations. Catalog entries retain their descriptor, source
identity/hash, semantic document revision where applicable, and original URL.
Catalog snapshots also retain consumed source evidence, including empty sources.
These observations do not include document bodies, replace runtime catalogs, or
constitute a complete dependency graph. Composers still consume captured in-memory
inputs. Uncaptured transitive Org/environment dependencies remain a provenance
limitation; recording a receipt does not make the build hermetic.

Schema version 3 indexes verified sealed manifests and their existing document
inventories. Document identity remains the authored content ID; content and
metadata fingerprints retain the release inventory's versioned definitions.
Placements are keyed by release and document ID. A changed theme can produce a
new release without producing a new content revision. Imported website bytes and
older manifests without an inventory never acquire inferred article metadata.

Schema version 4 adds immutable metric definitions, publication identities and
append-only observations for future collectors. Definitions include units and
reporting semantics; observations preserve source provenance and measurement time.
No collectors, imports or provider definitions are enabled. See the
[metrics storage contract](metrics.md).

Schema version 5 adds completed release-preparation attempts. Their captured
inputs, catalogs and diagnostics link successful pipeline executions to the exact
sealed manifest. Existing releases acquire no invented build provenance.

Schema version 6 adds immutable article-review source snapshots and append-only
reviewer/author dialogue. Unlike build catalog observations, these private review
records contain full draft text. The four-step [article review](article-review.md)
does not invoke an AI provider, verify reviewer assertions or grant publishing
approval. Backup and restore include this history; keep the database private.

The store uses bundled SQLite, WAL, foreign-key enforcement, FULL synchronization
and a bounded busy timeout. Publication transactions end before network requests.
The existing prepared/unknown/uploaded/live reconciliation protocol remains in
force; a SQLite commit does not make a remote write atomic. Existing process locks
continue to protect multi-step domain operations. Database/WAL files remain outside
source watching and are not website output.

## Build history

```sh
bln state builds --pipeline site --limit 20
bln state build 42
```

The list returns summaries, newest first (default 20, maximum 1,000). The detail
command returns the recorded receipt and catalog observations. IDs identify local
attempts, not content identities or remote publications. Repeated identical runs
receive separate IDs. There is currently no automatic history pruning.

Upgrading from version 1 imports the latest remaining receipt for each pipeline;
previously overwritten runs cannot be recovered. Imported receipts retain their
original evidence and may have no stage or catalog observation. New execution
receipts distinguish setup failures from prepared execution. Setup-failure input
hashes describe files inspected during failure reporting, not executed snapshots.
Dry runs and authoring checks create no build-history entries. Sealed-release
preparation records a separate release-attempt receipt after schema 5 migration;
it does not add an ordinary build-run receipt or replace preview output.

Receipts remain best effort: they are recorded after the existing output commit.
An output commit and its receipt are not one transaction. If recording fails,
Berlin logs the error, preserves the build result and never falls back to JSON.
A process crash can therefore leave installed output without a receipt. Failed
attempts record diagnostics/error and an empty output list. Each receipt and its
query projections commit together. Payloads are bounded to 16 MiB per attempt;
ordinary lifecycle records retain their separate 128 KiB limit.

Before upgrading an existing database, make a backup with `state backup`. Version
1 backups remain inspectable and restorable; run `state migrate` before using the
restored database for normal operations. An invalid legacy receipt aborts the
upgrade without changing the old schema. Repair the receipt or preserve it outside
the receipt directory before retrying; migration never silently discards it.

## Release preparation history

```sh
bln state release-attempts --pipeline site --limit 20
bln state release-attempt 7
```

After migration, `bln release` pipeline executions record completed attempts. The
receipt captures the consumed inputs and runtime catalog observations. Successful
attempts identify the sealed release and its output hashes under
`.berlin/releases/<id>/site/`; they never inspect the old preview directory as
though it were the new output. Repeating the same release records another actual
execution without duplicating the sealed manifest.

Failed attempts distinguish setup, execution and sealing. Setup-failure hashes
retain the reduced evidence described above; they are not executed snapshots.
Failed attempts have no claimed release or output inventory. This does not prove
that no bundle exists: sealing can complete before a later indexing failure.
Review the recorded error and use the existing verification/indexing tools.

Attempts are best effort and recorded while the project lock is held. Recording
failure logs a warning without replacing the release outcome. A process crash,
lock-acquisition or output-recovery failure can leave no attempt record. The
attempt transaction does not make filesystem installation atomic with SQLite.

Schema-5 migration creates an empty history. It does not reconstruct attempts from
existing files, modification times, manifests or ordinary build receipts. Imports
using `release --from-directory` remain explicit imports with no captured pipeline
execution; no release-attempt receipt is fabricated for them. Unmigrated projects
retain their existing behavior and do not write JSON release-attempt files.

Article inspection includes `build_lineage` on each sealed placement: either
`not_recorded` or `captured_build`, with matching attempt IDs and a total. Follow
those IDs through `state release-attempt`. This is local execution evidence, not
proof of remote deployment or a fully hermetic authoring process. History remains
available after a database-only restore; bundles are not reverified by inspection.
Back up before migrating. Schema 1–4 backups remain inspectable and restorable.

## Article history across releases

```sh
bln state documents --limit 100
bln state document YOUR-ORG-ROAM-ID --limit 100
bln state index-releases
```

Migration verifies all completed bundles in `.berlin/releases` before importing
their manifests and article revisions in one transaction. It skips only unfinished
`.preparing-*` directories. A corrupt or changed bundle aborts the upgrade without
changing the previous schema. Existing release IDs, manifests, files and publication
records are untouched. Back up the database before upgrading; schema 1 and 2
backups remain inspectable and restorable.

Newly sealed releases register automatically. Repeating the same release does not
add duplicate observations. Bundle installation and SQLite registration are
separate operations: if indexing fails after sealing, the command reports that the
bundle exists but could not be indexed. Retry the release, or use `index-releases`
to verify and add missing bundles. Indexing is additive, all-or-nothing, and never
publishes, repairs bundle bytes, deletes history, or overwrites a conflicting index.

`documents` lists observed titles, release counts and distinct content/metadata
fingerprints by authored ID. Titles are sorted; none is assumed to be the latest. `document` returns each indexed placement with its
website URL, destination, revision, recorded website publication status, and any
user-reported social announcement associated with that exact release. Site and
account context remain visible. The top-level `metrics` block reports whether metric
observations exist, their storage coverage counts and timestamps; it does not
compute engagement totals. A sealed release is not evidence of publication;
a missing website status is returned as `null`. Social reports remain user-reported,
not remotely verified. No draft, approval or delivery is created by these commands.

Both queries are offline and read only. They use indexed, hash-verified manifests;
they do not reread bundles or current authoring files, execute pipeline code, or
refresh remote status. Results explicitly say `bundles_reverified: false` and
`remote_verified: false`. The index may outlive its bundle files, including after
restoring a database-only backup. Publishing still requires verification of the
complete bundle through the existing release path. A database backup does not
suffice to republish missing artifacts.

Queries return a total and at most `--limit` entries (default 20, maximum 1,000).
Lists sort by document ID; placements sort by release ID for determinism, not by
publication date. No chronology or most-recent revision is inferred from a release
hash or its filesystem modification time.

Future work can add collectors, delivery attempts and fuller dependency projections. They must preserve captured-input
build semantics and existing sealed-release IDs. This storage work does not add
social API publishing, scheduling, analytics collection or graph-wide caching.
