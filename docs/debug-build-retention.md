# Experimental and product build retention

The authoritative layout is in [Build System](build-system.md). The September 13
policy replaces per-executable pruning with **one whole experimental build per
canonical source root** and **five completed product builds**, all under one
platform-resolved physical root.

- Experiments use `experiments/<source-root-id>/current`. A new independent
  owner replaces the old completed/failed slot, including dependencies,
  fingerprints and incremental variants. Nested commands reuse the slot;
  competing independent owners of the same purpose are rejected.
- Products use `products/<UTC-timestamp>-<session>`. Only successful owner
  completion enters the five-generation history. Normal failed exits remove
  the failed generation. Force-killed/stale leases require explicit maintenance.
- Unknown ownership and reparse escapes are rejected without cleanup. Never
  recover by deleting a workspace or the whole application-data root.
- `CARGO_INCREMENTAL=0` is owner-set. Independent experiments rebuild cleanly;
  commands needing compiler reuse belong in one transaction. This is a storage
  retention rule, not a byte quota or release attestation.

Use `scripts/tools/invoke-clearra-debug-cargo.ps1` for temporary Cargo work or
`scripts/tools/invoke-clearra-build.ps1` / `.mjs` for a complete transaction.
Conflicting output overrides fail before mutation. Unmanaged Cargo fails before
compilation; this does not prevent deliberately replacing build configuration.

The old `retain-clearra-debug-builds.ps1` remains a manual legacy diagnostic.
It is no longer invoked after every Cargo command: pruning selected files
invalidates nested consumers, while keeping all shared dependencies does not
bound whole-generation history.

Cleanup permanently removes generated files, not to the Recycle Bin. Rebuild
from source to regenerate them. Sources, worktrees, credentials, user inputs,
reports, accepted release exports and the published 4194 WASM are excluded.
Published WASM retains its separate five-generation runtime publication
contract. Global dependency downloads and toolchains are not build generations.
