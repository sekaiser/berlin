# Composable publications

Status: accepted. The current interfaces are described in
[Composable publications](../composable-publications.md),
[Unified publications](../unified-publications.md), and
[Tera components](../tera-components.md).

Publications reference existing semantic documents and authored external
resources through one immutable catalog. Composition does not replace the
document AST. The complete eligible catalog is distinct from the work selected
for one publication, so selecting a subset does not invalidate links to other
source documents. Metadata-only placements do not render source bodies.
Occurrences identify placements, not new source works.

Rust builders, the `Component` derive and `view!`, and execution-time Rhai
composition share one registry and normalized specification. Plan construction
never executes a composer or loads documents. Native component registration is
explicit; runtime scripts cannot register executable implementations.

Tera remains at 1.x. HTML belongs in theme templates, never in view syntax.
Publication rendering uses an isolated session with explicit slot ownership,
escaped scalar properties and renderer-created fragments. Effective inputs must
be snapshotted before consumption. Text interprets the same assembly directly.
Paper compilation is a separate authoring operation; composition itself supplies
HTML and text renderers.

The dependency direction is publication core -> existing document/content types;
HTML/text adapters -> publication core and the respective semantic renderer;
CLI/DSL -> core and adapters. The proc-macro crate emits runtime calls without
depending on the runtime crate. The core has no Tera, Rhai or CLI dependency.

Publication outputs use existing confinement, staging and recoverable installation.
Website releases continue sealing exact bundles. Composition never implies
authorization to publish remotely. Changes to the composition API do not alter
previously sealed release bundles. The unified pipeline migration is documented
in the [migration guide](../unified-publications-migration.md).

Canonical encoding uses UTF-8 JSON with recursively sorted object keys, ordered
arrays, no floats and no normalization. Purpose-specific, versioned SHA-256
domains separate identities. Composition and presentation inputs have separate
fingerprints. Local paths, timestamps and credentials do not enter public
manifests. A digest pins bytes; it does not recover missing historical inputs.
