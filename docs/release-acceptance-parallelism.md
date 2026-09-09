# Canonical ReleaseAcceptance Parallelism

## Measured baseline

The latest successful canonical run measured before this change is GitHub
Actions run `34303596352`. Its accepted-WASM producer job lasted about 22 minutes
8 seconds. The producer step itself lasted about 20 minutes 25 seconds:

- terminal-supply Rust contract: about 3 minutes 49 seconds;
- JavaScript terminal contract: about 0.31 seconds;
- `build-clearra-wasm.mjs --verify`: about 16 minutes 20 seconds;
- cache restore: about 56 seconds; and
- cache save: about 20 seconds.

The Pages consumer took about 1 minute 44 seconds after the accepted WASM
artifact became available. The critical path is therefore the producer, not the
Pages tests or artifact consumption. A separate Windows candidate build-only
job previously took about 15 minutes 56 seconds, so removing duplicate
verification from the producer alone cannot account for the whole improvement.
An older, warm-cache Linux Pages build completed in about 2 minutes 21 seconds,
but it used older source and is directional evidence only.

The stage families are independent only at process and filesystem boundaries.
They are not safe to background inside one job:

- `NoProductDebt` owns static architecture evidence and delegates specific
  executed evidence to later owners.
- `RustExactTests`, `ProductE2E`, and `RenderGolden` share the native-link
  fingerprint and Cargo target state.
- `CSanitizer` owns its sanitizer-specific C build tree.
- WASM source/host contracts use native Windows test binaries.
- Accepted WASM artifact compilation uses a dedicated Linux target tree.
- `WasmBuildTest` consumes the accepted WASM bytes to produce the Pages-ready
  Web output.

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

Two sibling prerequisite jobs replace the former serial producer:

1. `release-acceptance-wasm-contracts` runs on Windows. It combines compatible
   package checks into one Cargo invocation and compatible integration tests
   into one Cargo invocation, while retaining the terminal-supply and host
   contracts.
2. `release-acceptance-wasm-build` runs on Linux. It only builds the accepted
   WASM artifact, seals the closed receipt, and uploads the source/run/attempt-
   bound artifact. It does not repeat the native source/host contract suite.

The closed receipt binds:

- the exact source commit, workflow run ID, and run attempt;
- the WASM manifest digest and a digest of the complete regular-file set;
- every file name, size, and SHA-256 digest, including canonical aliases and
  content-addressed JS/WASM files; and
- the Cargo, CMake, Node, npm, PowerShell, Rust, and wasm-bindgen versions used
  by the producer.

The Pages shard depends on both siblings, downloads the artifact, verifies the
receipt before and after copying it into the Pages staging tree, and does not
install Rust targets, restore a build cache, or invoke a WASM build. Its probes
and frontend tests therefore exercise the exact producer bytes. A missing file,
extra file, symlink/reparse point, changed digest, partial generation, mismatched
source/run/attempt, or mismatched runtime identity fails closed.

The Pages shard report inherits the producer's full toolchain set from the
receipt. Because the producer is Linux and the consumer is Windows, final
evidence independently compares the portable Node and npm versions; producer-
host Rust, Cargo, CMake, and PowerShell remain receipt-bound but are not
incorrectly required to equal the consumer host tools. The accepted Pages
identity includes the receipt as a deployable file. The final acceptance fan-in
still consumes exactly six shard reports and reconstructs the original
eight-stage order. The two prerequisite jobs are required job evidence, not new
release stages.

The shard selector remains invalid for every task other than one explicit
`ReleaseAcceptance` request. No shard, contract prerequisite, or artifact
producer is a standalone release pass. Missing, duplicate, renamed, reordered,
cross-run, cross-SHA, or hash-tampered evidence fails before canonical
acceptance evidence is materialized.

## Compiler parallelism and the `workers 1` log

The top-level `clearra.ps1` progress line reports one orchestration task as
`workers 1`. It does not report Cargo's compiler job count. The hosted workflows
now set `CARGO_BUILD_JOBS` from the runner's logical processor count and emit
both values explicitly:

```text
task_workers=1 cargo_jobs=N
```

Cargo schedules independent crates and code-generation work in parallel; the
explicit setting makes that contract observable and prevents the outer progress
value from being mistaken for compiler serialization. The release profile stays
at `codegen-units = 1` with thin LTO. Increasing codegen units could reduce
compile time but may change output quality or runtime performance, so this
change parallelizes the dependency graph without weakening the shipped profile.

Moving the `wasm32-unknown-unknown` build from Windows to Linux is not expected
to slow the shipped program: target, source identity, Rust profile, and
wasm-bindgen version remain fixed, and the output is WebAssembly rather than a
host-native Linux binary. This is an architectural expectation, not hosted
performance evidence; receipt verification and product probes establish
correctness, while a branch CI run is still required before claiming a build-
time or runtime-performance result.

## Cache ownership

The caches are split by host and purpose:

- Windows native acceptance uses `release-acceptance-native-v3`. Foundation and
  WASM-contract jobs are restore-only readers; the Rust shard is its one
  verified optional writer.
- Linux accepted-WASM compilation uses `release-acceptance-wasm-v4`. It caches
  the Linux wasm-bindgen executable, Cargo registries/Git sources, and only the
  dedicated Cargo target directory. The WASM producer is its one verified
  optional writer.
- Sanitizer uses its source-bound C-build cache and remains isolated from Cargo
  and wasm-bindgen data.

Every writer runs only after its owning verification succeeds, uses the exact
primary key exposed by the restore step, is bounded to two minutes, and remains
non-authoritative if cache publication fails. Parallel jobs never write the
same immutable cache key. The Pages consumer has no Cargo/toolchain cache. If a
cache is absent or expired, its owner performs a correct cold build; product and
evidence contracts are unchanged.

## Expected effect and verification boundary

This design removes native contract compilation from the accepted artifact's
serial critical path, lets both parts start together, uses the Linux WASM
producer path, and makes the real Cargo parallelism visible. Candidate focused
WASM feedback uses the same Linux cache family, while candidate native
regressions continue as an independent Windows sibling. Artifact upload and
download remain real critical-path costs.

Focused tests prove the split task dispatch, receipt closure and tamper
rejection, platform-aware toolchain collection, exact dependency and artifact
names, cache isolation, Pages no-rebuild behavior, six-shard fan-in, and final
evidence binding. A local eight-logical-processor WSL check completed the merged
`cargo check` in 32.22 seconds, linked and ran both contract binaries in 2
minutes 6 seconds, and completed a cold release WASM artifact build in 6 minutes
21 seconds. Those measurements prove the Linux commands and expose the final
thin-LTO tail, but they do not establish hosted performance. The first branch CI
run must compare contract time, producer time, Pages-consumer time, artifact
transfer time, and total tail with run `34303596352` before this branch is merged
or a wall-clock improvement is claimed.
