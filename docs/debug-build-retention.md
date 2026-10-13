# Experimental and product build retention

The authoritative layout is in [Build System](build-system.md). The September 13
policy replaces per-executable pruning with **one whole experimental build per
canonical source root** and **five completed product builds**, all under one
platform-resolved physical root.

- Experiments use `experiments/<source-root-id>/current`. A new independent
  owner may reuse a completed schema-4 compiler cache with matching OS,
  architecture, toolchain and compiler context. Non-cache scratch is retired;
  failed, legacy and mismatched generations cannot seed the next cache.
  Nested commands reuse the slot; competing owners are rejected.
- Products use `products/<UTC-timestamp>-<session>`. Only successful owner
  completion enters the five-generation history. Normal failed exits remove
  the failed generation. Force-killed/stale leases require explicit maintenance.
- Unknown ownership and reparse escapes are rejected without cleanup. Never
  recover by deleting a workspace or the whole application-data root.
- Managed experiments set `CARGO_INCREMENTAL=1`; products set it to `0`.
  Cache seeds carry source/context provenance but never count as tests or release
  evidence. This is a generation-retention rule, not a byte quota.
- Transient slots bind path, OS, architecture and an exact lease token. Unknown,
  interrupted or replaced leases are never stolen based on PID disappearance.
  Normal drained release removes scratch; retained completed scratch is replaced
  on the next admission. Cleanup failure preserves unresolved ownership.

Use `scripts/tools/invoke-clearra-debug-cargo.ps1` for temporary Cargo work or
`scripts/tools/invoke-clearra-build.ps1` / `.mjs` for a complete transaction.
Conflicting managed output overrides fail before mutation. Ordinary direct Cargo
is supported and uses the declared `build/cargo/default` cache. Its single cache
does not create managed product generations. Review an official `cargo clean`
profile cleanup separately; never prune dependencies during an active build.

The old `retain-clearra-debug-builds.ps1` remains a manual legacy diagnostic.
It is no longer invoked after every Cargo command: pruning selected files
invalidates nested consumers, while keeping all shared dependencies does not
bound whole-generation history.

Cleanup permanently removes generated files, not to the Recycle Bin. Rebuild
from source to regenerate them. Sources, worktrees, credentials, user inputs,
reports, accepted release exports and the published 4194 WASM are excluded.
Published WASM retains its separate five-generation runtime publication
contract. Global dependency downloads and toolchains are not build generations.
