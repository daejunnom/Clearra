# Canonical ReleaseAcceptance Parallelism

## Measured baseline

Recent successful canonical runs confirm that the hosted tail is governed by
the slowest Windows job, not by the sum of all test bodies. Pages usually took
455 to 570 seconds. In run `33582717675`, however, Sanitizer became the longest
job at 588 seconds: 245 seconds were spent restoring the shared Cargo,
wasm-bindgen, and build cache before the C-only 293-second gate. Pages took 570
seconds in that run, including a 151-second cache restore and a 345-second main
step. Inside the old Pages main step, terminal-contract coverage and the
verified WASM build consumed most of the time before the comparatively short
runtime probes, UI tests, Web tests, and frontend build.

The stage families are independent only at process and filesystem boundaries.
They are not safe to background inside one job:

- `NoProductDebt` owns static architecture evidence and delegates specific
  executed evidence to later owners.
- `RustExactTests`, `ProductE2E`, and `RenderGolden` share the native-link
  fingerprint and Cargo target state.
- `CSanitizer` owns its sanitizer-specific C build tree.
- `WasmBuildTest` owns the Pages-ready Web output, but the verified WASM product
  bytes now have a separate one-shot producer.

## Canonical DAG

The local command remains unchanged and serial:

```powershell
powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ExecutionSurface Trusted
```

GitHub Actions keeps the existing six canonical acceptance shards selected by
the tracked `-ReleaseAcceptanceShard` parameter:

| Job | Ordered stages | Cross-job input |
| --- | --- | --- |
| Foundation NoProductDebt | `NoProductDebt` | exact source/run/attempt |
| Foundation AdversarialCorrectness | `AdversarialCorrectness` | exact source/run/attempt |
| Foundation DesktopHost | `DesktopHost` | exact source/run/attempt |
| Sanitizer | `CSanitizer` | exact source/run/attempt |
| Rust | `RustExactTests`, `ProductE2E`, `RenderGolden` | exact accepted CTK3 distribution |
| Pages | `WasmBuildTest` | exact accepted WASM build, source/run/attempt, and Pages base path |

One additional job, `release-acceptance-wasm-build`, is a producer rather than
a seventh shard. It runs the terminal-supply Rust contract, the product terminal
contract, and `build-clearra-wasm.mjs --verify` exactly once. It then seals a
closed receipt over:

- the exact source commit, workflow run ID, and run attempt;
- the WASM manifest digest and a digest of the complete regular-file set;
- every file name, size, and SHA-256 digest, including canonical aliases and
  content-addressed JS/WASM files; and
- the Cargo, CMake, Node, npm, PowerShell, Rust, and wasm-bindgen versions used
  by the producer.

The receipt and payload are uploaded under a source/run/attempt-bound artifact
name. The Pages shard depends on that producer, downloads the artifact, verifies
the receipt before and after copying it into the Pages staging tree, and does
not install Rust targets, restore a build cache, or invoke a WASM build. Its
remaining probes and frontend tests therefore exercise the exact producer
bytes. A missing file, extra file, symlink/reparse point, changed digest,
partial generation, mismatched source/run/attempt, or mismatched runtime
identity fails closed.

The Pages shard report inherits the producer's full toolchain set from the
receipt while independently checking its overlapping Node, npm, and PowerShell
versions. The accepted Pages identity includes the receipt as a deployable file.
Final canonical evidence verifies the receipt again, requires the producer job
and its two required steps to have succeeded, compares the Pages shard
toolchains with the receipt, and records the receipt SHA-256. The final
acceptance fan-in still consumes exactly six shard reports and reconstructs the
original eight-stage order; the producer is a bound prerequisite, not an
additional release stage.

The shard selector remains invalid for every task other than one explicit
`ReleaseAcceptance` request. No shard or producer is a standalone release pass.
Missing, duplicate, renamed, reordered, cross-run, cross-SHA, or hash-tampered
evidence fails before canonical acceptance evidence is materialized.

## Cache ownership

The Rust/toolchain-owning non-Pages shards and the one-shot WASM producer use
`actions/cache/restore`, never `actions/cache` and never
`actions/cache/save`. An exact-SHA key is tried first and the prior canonical
dependency prefix is the fallback. Each hosted runner extracts into its own
filesystem and no modified cache is written back. The sanitizer shard runs only
the CMake/MSVC ASan gate, so its restore is deliberately limited to
`~/AppData/Local/Clearra/build` under a sanitizer-specific source-bound key; it
does not download Cargo registries, Cargo Git checkouts, or wasm-bindgen.

The Pages consumer has no Cargo/toolchain cache at all. This avoids restoring
the large build snapshot merely to read already accepted WASM bytes. If the
producer cache is absent or expired, the producer performs a correct cold build
and seals that result; the consumer and evidence contracts are unchanged.

## Expected effect and verification boundary

This design fixes WASM production at one verified owner and removes the large
build-cache restore and all producer work from every Pages read-only consumer.
It also makes build-versus-consume ownership explicit, so adding another
read-only Pages probe cannot silently trigger another compilation. Artifact
upload/download and the producer-to-Pages dependency remain real critical-path
costs.

Local focused tests prove receipt closure and tamper rejection, producer and
consumer workflow wiring, exact dependency and artifact names, the six-shard
fan-in, Pages no-rebuild behavior, toolchain inheritance, and final evidence
binding. They do not establish hosted performance. The next successful GitHub
run must be used to compare producer time, Pages-consumer time, artifact transfer
time, and total tail against the measured baseline before claiming a wall-clock
improvement.
