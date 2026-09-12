# PC4 Full-Solution Authority Audit (2026-09-12)

## Product authority

v0.9.0 is an exact solution-table accelerator, not a recommendation product.
For each qualified rule profile, the only permitted tablebase decision input is
the complete outgoing transition set decoded from that profile's qualified
graph artifact. Clearra follows every distinct target transition and uses its
own exact inverse lock-clear placement materializer to recover every distinct
concrete realization. V-star values, policy actions, Krylov state, and
optimal-action ordering are not lookup, pruning, scoring, probability,
tie-break, PC, or Setup inputs.

The graph transition is a field-level relation, not placement or replay
evidence. Hydra degree may repeat a target once per raw placement. Because the
qualified edge type intentionally carries no placement identity, traversal
validates every raw occurrence and canonicalizes an equal
`source + piece + target` once; exact ILC materialization then recovers every
distinct concrete placement for that transition once. This avoids multiplying
the same placement family by the raw target multiplicity. Traversal still
preserves every path that converges from a different prior path. `materializer`
canonicalizes only equal concrete placement identities, and
`lazy_materialized_path` pages the complete Cartesian product of the remaining
alternatives. An objective may select from that universe only after
complete-source count and digest evidence crosses the application reducer
boundary.

## Target scope

The graph state is bounded to fields at most four rows high. It can potentially
accelerate PC and Setup terminal targets in `1..=4`; it is not restricted by
definition to exactly four lines. Conversely, a four-row graph does not prove
shorter-target completeness. Each profile, PC-or-Setup use case, and target
needs independently qualified terminal semantics, outgoing-edge completeness,
known-answer cases, and offline exhaustive parity. Five- and six-line requests
stay on the exact offline path. Setup needs its own terminal/completeness
qualification and does not gain a reverse index merely because the forward
graph exists.

The current code deliberately exports no production completeness-evidence
constructor, so none of these target scopes is activated by this audit alone.

## Dependency and activation audit

`clearra-pc-next-probability` remains an unpublished, dependency-free `no_std`
workspace member. No CLI, App, GUI, WASM, Web, Desktop, Discord, host-contract,
UI-schema, or PC4 runtime dependency path reaches it. It has no filesystem,
network, embedded asset, subprocess, or URL access. Its synthetic opaque Krylov
adapter is therefore dormant research structure, not product authority.

The release-blocking `PC4 Full Solution Authority` architecture task now checks
that dependency closure from `clearra-core-executor` and `clearra-wasm-abi` as
well as the prior product roots. Its product-source scan covers the Rust hosts,
Web/Desktop/Discord applications, `packages/clearra-ui`, Svelte and application
configuration, release and future discovery scripts, workflow configuration,
and container recipes. The scan includes Rust, TypeScript/JavaScript, Svelte,
JSON/TOML/YAML, PowerShell, shell, Python, and extensionless Dockerfile surfaces.

Forbidden V-star/policy/Krylov or single-best semantics are checked only on
production-bearing content. Documentation and separately named test/fixture
paths are excluded, and `#[cfg(test)]` Rust blocks are removed before scanning.
The dormant probability crate is not silently allowlisted as a product source:
it is audited separately for dependency-free `no_std` and no-I/O behavior, and
any product import or configuration reference to that crate still fails. This
keeps research vocabulary and synthetic negative tests legal without allowing
them to become CLI, GUI, release, discovery, or runtime authority.

The same task pins the qualified lookup to `FieldHashIndex`, `GraphOffsets`, and
`Graph`, checks the complete-edge traversal, checks the
`clearra-core-executor` ILC materializer's all-realization loop, and requires the
application adapter to exhaust graph and concrete-materialization pages before
minting reducer completeness. The existing executed Rust cases remain the
behavioral proof; the static task is a fail-closed architecture regression and
does not substitute for upstream format qualification or exact offline parity.

## v0.9 static-beta migration switch

This branch still carries the v0.8.1 compatibility path: the tracked
`pc4-compact-exact-v12.bin`, `CLR4TB12` loader, embedded CLI copy, Web/WASM
installer, and container copy remain live. Removing them before the qualified
online replacement is ready would break v0.8.1, so their mere presence does not
fail the shared architecture task yet.

The v0.9 migration commit must add
`scripts/architecture/pc4-v090-online-authority.mode` with exactly:

```text
pc4-product-authority=v0.9-online-graph-v1
```

Once that explicit marker exists, the architecture task treats the static
binary and every production `CLR4TB12`, compact-tablebase, embedded-asset, or
WASM install/release reference as a migration blocker. A malformed marker also
fails closed. Test fixtures and documentation may retain legacy names for
rollback and negative coverage, but they cannot satisfy or defeat the product
scan. Absence of the marker means only “v0.8.1 compatibility mode”; it is not
evidence that v0.9 migration, qualification, or release is complete.

## Remaining activation evidence

Production remains blocked until every shipped profile and enabled target has
an immutable graph generation, qualified record/index format, provenance,
target-specific terminal/completeness statement, known-answer suite, and
offline exhaustive parity. Browser/native Range transports and a product
adapter must preserve that exact identity. None of the synthetic fixtures in
this branch grants release or dataset authority.
