# Canonical ReleaseAcceptance Parallelism

## Measured baseline

Two successful Windows canonical runs showed why the tail must be split at job
boundaries. The eight stage bodies took 930 seconds and 1,105 seconds
respectively, a runner-to-runner variation of 18.8%. The second canonical step
was 18 minutes 35 seconds. Both runs then stored a new exact-SHA 1.57 GB cache;
compression dominated the additional 189 to roughly 214 second post-job tail.

The longest measured stage families were independent only at the process and
filesystem level. They were not safe to background in one job:

- `NoProductDebt` establishes the release-mode architecture evidence consumed
  by `DesktopHost` and defers complete-required and renderer evidence to later
  owners.
- `RustExactTests`, `ProductE2E`, and `RenderGolden` share the native-link
  fingerprint and Cargo target state.
- `WasmBuildTest` owns the Pages build output.
- `CSanitizer` owns its sanitizer-specific C build tree.

## Canonical DAG

The local command remains unchanged and serial:

```powershell
powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ExecutionSurface Trusted
```

GitHub Actions selects one of four internal shards only through the tracked
`-ReleaseAcceptanceShard` parameter:

| Job | Ordered stages | Cross-job input |
| --- | --- | --- |
| Foundation | `NoProductDebt`, `AdversarialCorrectness`, `DesktopHost` | exact source/run/attempt |
| Sanitizer | `CSanitizer` | exact source/run/attempt |
| Rust | `RustExactTests`, `ProductE2E`, `RenderGolden` | exact accepted CTK3 distribution |
| Pages | `WasmBuildTest` | exact source/run/attempt and Pages base path |

The shard selector is rejected for every task other than one explicit
`ReleaseAcceptance` request. No shard is a standalone release pass. Each job
writes one canonical shard report, and the fan-in accepts exactly the four
tracked filenames. The report set must have one source SHA, run ID, attempt 1,
workflow path, command, ordered stage list, passing status, and consistent
overlapping toolchain versions. The fan-in then seals the original eight-stage
order, four surface reports, and these deferred-owner edges:

- complete-required evidence: `NoProductDebt` to `RustExactTests`
- PNG/GIF renderer evidence: `NoProductDebt` to `RenderGolden`
- real desktop request: `NoProductDebt` to `DesktopHost`
- adversarial Rust evidence: `AdversarialCorrectness` to `RustExactTests`

Missing, duplicate, renamed, reordered, cross-run, cross-SHA, or hash-tampered
shard evidence fails before canonical acceptance evidence is materialized.

## Cache ownership

All four jobs use `actions/cache/restore`, never `actions/cache` and never
`actions/cache/save`. An exact-SHA key is tried first and the prior canonical
dependency prefix is the fallback. The restored archive can include the prior
Clearra build root, but extraction happens independently on each hosted runner;
the jobs do not share a writable directory. No modified shard cache is written
back. This preserves the measured warm-build opportunity while eliminating the
189-to-214-second compression/save tail entirely. When the retained cache is
absent or expires, every shard performs a correct cold build and the evidence
contract is unchanged.

## Expected effect and verification boundary

From the measured second run, the isolated stage-family maxima are about 395
seconds for Foundation, 328 seconds for Pages, 192 seconds for Sanitizer, and
191 seconds for Rust. Runner setup, cold compilation, and artifact transfer
remain, so the release-ready expectation is an
8-to-11-minute warm-cache canonical tail rather than the unsafe sum of those
tasks. A future GitHub run must provide both warm- and cold-cache wall-time
comparisons; local focused tests prove the task mapping, evidence closure, YAML
wiring, and restore-only cache contract but cannot claim hosted-run performance.
