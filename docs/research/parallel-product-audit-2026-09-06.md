# Product parallel performance audit — 2026-09-06

Status: implementation and focused verification in progress. No release Go.

Latest user clarification: accept approximately0.4s as the ordinary Build/PC
performance stopping point and focus improvement on minimum solutions before
release. The earlier0.1s Build criterion below is historical, no longer a gate.
Keep all246 solutions; a summary-only shortcut is still not authorized. The
20s minimum target comes from the user's Qnia sfinder_wasm result (~19s).
Qnia used HiGHS; other settings are unknown, so compare public defaults. The
user also requested a follow-up audit of the0.129s raw-source vs0.4s GUI timing
boundary; do not mislabel that difference as measured gallery rendering time.

## Frozen user criteria

### Latest implementation / nonpublishing gate checkpoint

- Cross-width RNG selection was fixed at all eight blocking/cooperative sites.
  Same-binary native diagnostic old32→portable64: exact proof20.234→6.775s,
  first canonical28.584→9.995s, total48.818→16.771s, identical exact25 and
  canonical keys. This is not a GUI20s pass; see the Qnia comparison report.
- Grow-only idle prewarm now preserves all10 remote cached workers between
  auto11 runs. Actual published512 GUI verification returned246 solutions and
  5,040/5,040 success; the observed repeat was0.6s with no new worker creation.
  The remaining enqueue wait is not explained by JSON/gallery formatting.
- Local Full gate attempt1 stopped in static validation (Oracle stale fixture
  hash and seven missing SRP explanations). These errors were corrected.
  Attempt2 passed all static checks and NoProductDebt execution probes, then
  stopped before C test generation/execution because Windows UMCI is enforced:
  `E_WINDOWS_GENERATED_EXECUTION_REQUIRES_APPROVED_PACKAGE`. No policy bypass,
  signing/unblocking, runtime substitution, or full-gate success is claimed.
- The95 static warnings are the existing >1,200-line cohesion-review notices;
  all95 files have permanent SRP explanations, with no temporary exemption.
  Marker presence is not evidence of an individual full cohesion audit.
- Published4194 remains512 at this checkpoint; current frozen Rust/source
  contract is361b042e73a05bef6f67664baa4b053e47981bcb4b738001eb585b6c345360d9.
  Native diagnostics and earlier512 GUI runs do not validate that source.
- Read-only current-source audit reconfirmed that Build minimum (BuildV2),
  PC score-minimals and Build score-minimum still lack the PC-minimum guarded
  cooperative finalizer/task-driver wiring. RNG is shared, but these routes
  must not be described as newly parallelized or cancellation/peak-guard fixed.
  Later portfolio pages are bounded serial cooperative work after quiescing
  parallel queries, not a dangling-query infinite-wait defect.

No final commit, push, tag, actual deployment or new WASM publication has occurred
at this checkpoint. A separate no-publish CI gate is being prepared because the
existing canonical main workflow can trigger actual Discord candidate deployment.

- Current P0: pattern-based complete replay fails during finalization; unexpected
  local4194 refresh can destroy a running search. Retain all prior correctness
  and cancellation protections while fixing both.
- PC all-solutions regression and Geometry/BuildUp underutilization must be checked
  with actual runnable WASM, not source-only claims.
- Minimum solutions: CTK3 `ctk3_w0kCQBjwwAMPPAD37g`, P7, exact minimum 25.
  Target: at most 20 seconds with multiple workers, including exact proof and
  first canonical portfolio (not merely finding any 25-row witness).
- Build probability: approximately0.4s accepted (supersedes0.1s). User clarified existing field
  `ctk3_w0kCQBjwwAMPPAD37` and final field `ctk3_w0kCQBAANGI`, P7.
  The existing code is missing the final `g` from the previously supplied valid
  fixture; that explicit assumption was reported. Valid base is `0x3c0f03c0f`,
  final is full 4L `0xffffffffff`; target-only CLI mask is `0xfc3f0fc3f0`.
  Hold empty, all solutions, buildability, no mirror (base is asymmetric).
- Active/max worker count belongs only left of elapsed time; stage cards do not
  duplicate it. Geometry node progress must also work on intentional serial paths.
- Additional P1: keep all admitted compute slots useful, but allow a separate
  control-only coordinator if that avoids starving management duties. Full-N
  mode must not silently add an unadmitted N+1 compute role. Remove avoidable visible dispatch/event waits.
  Measure query-wave and last-hard-shard tail separately across low/high worker
  counts. Filling one coordinator slot alone does not prove the tail is fixed.

## Baseline evidence

- 4194 initially serves old `2ad9f8f75182341a12665bee` WASM, not all current source.
- Earlier WRONG browser build fixture (base 0, target left 4x4) displayed 0.2s,
  0/1 terminal workers, Geometry nodes absent; 48 buildable tilings, 692/2520
  covered patterns, 912 candidate tilings. This is not user performance evidence.
- Corrected browser base16/target24 fixture on old WASM: displayed 0.2s,
  terminal 0/1 workers, Geometry nodes absent, 246 buildable tilings,
  5040/5040 covered patterns, 2260 candidate tilings, complete probability 100%.
- Native release-profile minimum probe, 10 threads: proof 31.978s, first canonical
  portfolio 30.298s (62.3s combined), correct 25/golden first portfolio.
  Positive k=25 feasibility calls consumed 21.176s and 19.869s; negative k=24
  proof 7.697s. This is actual algorithm cost, not an acceptable scheduling tail.
- Hint-per-shard revision: approximately 62.41s combined, still No-Go.
  The k=25 positive query alone remained 19.721s with roughly 197.186s summed
  task time over ten workers. This is busy parallel search, not idle workers.
- Global positive-only warm revision: native release probe passed in 17.25s
  combined (proof 7.246s, first canonical portfolio 9.979s), about 72% below the
  hint-per-shard revision. Exact minimum 25 and the golden first source-row
  sequence are unchanged. Remaining dominant waves are negative proofs:
  root k=24 6.989s; canonical k=24 6.486s and k=23 2.908s.
  This is native evidence only; the browser <=20s criterion is still unverified.
  Independent review identified a large-matrix serial-warm cost risk and missing
  retained Found-witness capacity; both are being addressed before publication.

## Implemented directions awaiting final measurement

- Keep unknown-family streams at 64 candidates for four initial waves per
  verifier before raising to 1024. Known counts immediately override this with
  four-wave balanced sizing (the corrected Build fixture uses 57). This avoids
  medium PC streams collapsing to two large packets without imposing small
  packets for the entire large search. Focused transition contracts pass.
- Read durable quarantine and journal head in one fenced write transaction.
  Do not free verifier leases before durable receipts are acknowledged: doing so
  retains additional receipt memory outside current admission accounting.
- Remove hidden GUI CPU warmup flags that invoke native startup barriers.
  Explicit CLI warmup remains supported; GUI worker startup is progressive.
- Preserve Build worker capacity while its producer reports provisional zero
  candidates. Treating initial Some(0) as final previously admitted only the
  coordinator and selected the serial route, despite an intact --workers 11.
- Preserve advisory positive witness hints through exact AtMost partitioning and
  versioned WASM wire. Hints cannot authorize negative proof; replay and exact
  partition coverage remain mandatory. The global repair has a checked generic
  word-work admission ceiling, skips to partitions on excessive cost, and charges
  retained positive-witness capacity without discarded decision clones.
- Remove duplicate stage worker metrics and retain header active/max display.
- Emit explicit cumulative serial Build Geometry/candidate/build counters through
  the existing stage/count protocol, adapting them to browser telemetry at the
  boundary. Unknown totals remain unknown, not fictitious progress percentages.

## Scope found during audit

The ordinary PC minimum finalizer has the cooperative AtMost worker seam.
Score-minimals and BuildV2 minimum-set finalizers still call the synchronous
canonical constructor; shared algorithm changes do not mean those finalizers
have gained browser worker dispatch. Do not report all subset products as
parallel. Migrating those owners requires preserving score eligibility, source
row remapping, memory admission, and exactly-once result projection.

## External algorithm ideas (no external solver code/dependencies)

[HiGHS parallel documentation](https://ergo-code.github.io/HiGHS/dev/parallel/)
motivates independent search subproblems and tree-level parallelism rather than
assuming a larger thread count accelerates every memory-bound operation.
[Concurrent Cube-and-Conquer](https://arxiv.org/abs/1402.4465) motivates adapting
the split/solve boundary and preserving useful solver state.
[Dancing Links](https://arxiv.org/abs/cs/0011047) provides reversible sparse
constraint-update ideas. Exact cover is not a replacement for minimum set cover:
coverage overlaps, original row identities, canonical order and all tied
portfolios must be preserved.

Candidate directions must pass small exact parity and the same product benchmark.
No known 25-row answer may be injected as a production seed. No approximation,
timeout or incomplete proof counts as acceptance.

## Focused checks completed (not deployment acceptance)

- Web/UI TypeScript noEmit and worker pool/runner/journal contracts pass.
- GUI execution projection: 19/19; progress surfaces: 2/2, Svelte warnings 0.
- Corrected build field command contract passes: base16 + target24 = full4L,
  P7, 11 workers, no hidden CPU warmup, no asymmetric-base mirror.
- Release Rust serial build progress contracts: 2/2.
- Corrected CTK3 Build distributed capacity/progress contract: 1/1.
- Exact AtMost wire roundtrip/corruption/hint/stale-task contracts: 4/4.
- Final minimum solver focused managed-debug run: 100/100, no compiler/test
  warnings. This includes warm admission, positive replay and exhaustive negative
  authority, cancellation, retained witness guards, and exhaustive tiny matrix /
  portfolio parity. The two initial failures were reviewed and corrected:
  optional memo fallback remains safe under its explicit cap; the first-page
  fixture now genuinely enters partitioning before testing scheduler handoff.

These checks do not replace the frozen product performance criteria, the final
new-WASM browser run, or the exact-SHA canonical release gate.

## Build identity checkpoint

The first local WASM compilation finished successfully but publication was
correctly rejected because parallel test-source edits changed its source
fingerprint during compilation. The old 4194 generation was preserved; no
partial artifact was served. Freeze all Rust text, including cfg(test), before
retrying the canonical builder. Never bypass the source-identity guard.

## Actual browser A/B after publication

- Local generation `49e8e67217a299052cebae4f`, source fingerprint
  `1a8c3c2fa5efacb496c0f1f168ddd685f60f52fbc32aa940f73f0d21c3b62b7f`,
  was published and HTTP/local/source identities matched before testing.
- PC minimum: **91.5s**, exact minimum 25, 246 source solutions, 5040/5040
  coverage. Geometry reached 11/11; minimum selection showed 10/11 then 6/11.
  This candidate fails the GUI target. Later warm-admission hardening had
  suppressed useful multi-supporter repairs; the prior native 17.25s must not
  be attributed to this final admission policy.
- PC all solutions: displayed 0.8s, terminal 0/11, 246 solutions, 2260 candidates.
- Corrected Build: displayed 0.4s first run / 0.2s repeat, but terminal 0/1.
  Geometry nodes 3443 now appear; stage worker duplicates are gone. Direct raw
  WASM preparation also returns serial even for the passing native test argv.
  Actual additional root cause: wasm32 coordinator has one local compute slot,
  but Build charged all eleven separate worker replicas as local compute leases.
  The next candidate separates local compute admission from replica memory.
- Live Pages is **not identified as tag v0.7.4**: manifest reports source commit
  `91772735c3f7ec7d89ecd3e82aa4af4014995bf6`, WASM generation
  `29bd7732085706bb3f8235d0`. The v0.7.4 tag resolves to commit
  `0438d85f90b47c4ce89835f6a6d665a0415aa25a`.
- Live Pages, same base16/target24/P7 input: displayed **0.1s**, 5040/5040,
  2260 candidates, explicitly **coverage-only; solution set not computed**.
  This is a PUBLIC DISPLAY label, not proof that the old core avoided all
  solution calculation; source inspection shows its public projection can erase
  solution keys. Trace actual core retention before attributing the speed ratio
  to different work. Current GUI visibly retains 246 solutions, which remains
  required under the user's confirmed 0.1s target.

Next candidate bounds global warm cursor work itself and ends at the preferred
repair boundary before unrelated serial fallback, rather than rejecting useful
multi-supporter hints from an inflated hypothetical cost. Final diagnostics and
new browser measurements are pending. No release workflow was dispatched.

## Next candidate and new P0

- Corrected global warm admission now charges an enforced 1000 total cursor
  steps, not a hypothetical full allowance per supporter. It exits before
  PrepareForcedSupporters, leaving worker/serial heuristics unchanged. Native
  final candidate: 18.01s (proof7.362s + canonical10.620s), exact25/golden PASS.
  Primary positive warm calls use8/352/11 steps; canonical positive uses10.
  Final focused managed-debug coverage checks: 101/101 PASS.
- New user P0 reproduced on local49e8: PC CompleteReplayPaths, same CTK3/P7,
  fails at solution finalization after Geometry3452/candidates2260 and
  BuildUp4777/checks410844 complete. At0.6s it displays unsupported,
  execution-failed, resource-limit together. Diagnose actual contract/projection
  failure before deciding whether this is truly fixed-queue-only. Only if a
  fixed queue is fundamentally required should GUI move it below fixed-queue
  highest score and reject patterns before execution.
- Conditional exact-dual cache was subsequently applied and checked: 108/108
  focused coverage tests passed. Native isolated probe **14.89s** vs18.01s
  (17.3% lower elapsed), proof6.381s + canonical8.485s. Exact25 and the first
  canonical set are unchanged. This remains native evidence, not GUI acceptance.

## Latest runtime and additional P0/P1 checkpoint

- Served WASM `0709d2c608ee171ead7fbe54`, source
  `e1b1c6653c4ad0de9c6e5ab6adebf44d77f7b5ced450d884908aedfe7128fab8`,
  was fresh at publication. Actual raw WASM Build preparation now admits mode1
  and11workers for the full GUI argv. Later source changes are NOT in0709.
- A0709 GUI minimum run was still active at33.2s; a source-edit/Vite refresh
  then reset it. This is an interrupted measurement, NOT a33.2s completion.
- New minimum P1: coordinator was excluded from exact work; query waves can
  drain to1/0 and restart remote workers. A completion-owned, Arc-sharing local
  shard and bounded host yields are in implementation. Do not smooth telemetry
  to hide genuine waiting; test low/high admitted worker counts.
- ReplayP0 actual raw cause: `complete_replay_memory_limit_exceeded` during
  eager expansion, NOT an unsupported pattern. Preserve P7 support. Accurate
  primary failure classification and an exact per-geometry page source are in
  implementation; keep500ms GIF and whole-current-geometry copy.
- Refresh audit:4194 PID11592 remained unchanged across the reported interval;
  task uses IgnoreNew/no time limit and logs preserve every60seconds. Ordinary
  source HMR was enabled. Local-recovery/local-audit now disable HMR; a separate
  verified generation endpoint/poll invalidates only the next worker job, never
  reloads the page. After one explicit reload to adopt this setting, visible
  KO/P7 input survived ongoing source edits and watchdog cycles. Endpoint and
  polling contracts passed. Existing normal development HMR remains unchanged.
- No gate, release, commit or push is authorized by these partial checks.

## Coordinator / replay integration checkpoint

- Exact coordinator work now shares the immutable query with the App finalizer.
  Admission retains a real host lease and accounts for App/ABI owners, WASM
  linear-memory floor, scratch, retry task, and overlapping terminal receipt.
  A declined local shard returns the same issued task to a remote worker; it
  does not mint a negative proof. A second remote drain closes the late-retry
  race after the first remote dispatcher has already exhausted the frontier.
- Focused coverage suite passed110/110; final terminal-receipt guard checks
  passed6/6. The App/WASM/ABI integration checks and new browser performance
  remain separate and pending at this checkpoint. The old native14.89s probe
  executable has been removed by retention; a new executable is required for
  any4/8/16-partitions-per-worker comparison.
- Existing runner profiling now records bounded query-wave preparation, first
  and last receipt, admission wait, remote drain, and actual coordinator work.
  Tests cover2/11/32 workers, late retry, and a128-wave retention bound. Real
  active counters are not smoothed or replaced with allocated worker counts.
- Fixed frontier splitting is not work donation: once only one running shard
  remains, idle workers cannot split its private cursor using the current ABI.
  The current factor4 must be compared with8/16 before claiming a tail fix.
  A separate avoidable issue was found: issuing a remote task before acquiring
  an idle client can hide one unstarted task from the coordinator. A lease-first
  task factory is being implemented without changing proof ownership.
- Replay per-geometry expansion still exceeded512MiB on the P7 fixture. This
  failed experiment is not a P0 fix. The next source uses bounded geometry ×
  pattern cells, exact canonical ordering/deduplication, and100-member pages.
  The actual distributed completion now enters the same cooperative App source
  instead of silently calling the old eager terminal materializer. Counting
  still traverses all traces; both resource behavior and startup time require
  the new64MiB fixture test before publication.
- Local4194 audit mode preserves input during source edits and minutely watchdog
  checks. PID11592 stayed stable in live observations. Default watchdog ownership
  was not changed; only custom-port diagnostic mutex names were isolated, and
  both watchdog regression tests passed. No active server was restarted.

No release Go follows from this checkpoint. The currently served0709 generation
does not contain these later Rust changes.

## Idle-assistance implementation checkpoint

- User requested revisiting the Geometry work-stealing approach. Current
  Geometry uses weighted pre-splitting and ready-worker consumption of a shared
  queue, not migration of a running private DFS stack. The exact minimum queue
  now also acquires a ready executor before issuing its next task.
- A bounded, one-level idle-assistance protocol is being integrated. A complete
  canonical child fanout races the unchanged original parent cursor/cache.
  Original negative OR all child negatives closes that original root; cancelled
  or missing children never count as negative evidence. Physical issued-receipt
  drain is tracked independently of logical proof closure. A first late receipt
  is distinguished from a duplicate; contradictory validated positives fail
  closed. No private reduced-row DFS snapshot is transplanted.
- This is idle assistance with possible duplicate work, not unvisited-only
  donation. Static factor4 was not simply raised. The initial assist core and
  bounded first-pivot checks passed15/15, including512 tiny-matrix cube parity.
  A final fixed stack-scratch refinement and integrated host tests are pending.
  Native probe assistance on/off is being connected; this is not evidence that
  the native CLI product already uses the browser's portable shard scheduler.
- Pre-assist App/WASM/ABI/core-executor no-run checkpoint passed without warnings
  (source fingerprint14beac633c697741323fa09c204a0752286fd48eb884e052d0854759729647e3).
  Focused replay parity/cancellation and minimum envelope checks passed.
  The actual minimum response is Scenario/pc-scenario; the initial Pc-only heap
  projection silently declined coordinator admission. Exact matched Scenario
  support now fixes that boundary without admitting unrelated response kinds.
- The new distributed replay parity test passes with a16MiB native test-thread
  stack. Its default2MiB debug test-thread stack overflow is NOT a demonstrated
  WASM runtime fix; release WASM/browser verification remains required.
- Exact replayP0 diagnostic under64MiB: Core owner634,760 bytes,246 graphs,
  source494,621 bytes at entry. First failure was geometry1/pattern64:
  required4,114,487 > allowed4,111,200 bytes. The legacy eager16-copy reserve,
  not a missing distributed bridge, constrained the bounded cell. Replacement
  accounting measures moved trace owners, overlapping old/new inline buffers,
  first100/page100 projection and actual allocated capacities. Large fixture
  completion and raw-versus-canonical duplicate counts remain unverified.
- Canonical trk1 formatting now writes directly to one output String; comparison
  can stream against an existing key with no temporary heap allocation. Legacy
  byte format, all hold variants, empty keys, extreme counters and UTF-8 mismatch
  behavior are covered. Focused3/3 and the whole replay crate18/18 passed.
- Debug retention previously confused the distinct WASM and ABI lib-test targets
  because both have the clearra_wasm executable stem. It now resolves Cargo
  fingerprint marker names to package + target kind and retains one generation
  per semantic unit, preserving unknown owners. Dry-run collision regression
  passed; actual target dry-run found43 resolved targets, no unresolved owners
  and nothing to delete. The regression is registered in the release-test list;
  only its runner contract was executed, not the release gate.

No new WASM publication, native performance A/B, canonical release gate, commit,
push, or production deployment has been performed for this checkpoint.

### Integrated source checkpoint and observable scheduling timings

- The next shared App/Core/Coverage/Postprocess/WASM/ABI and native-probe debug
  no-run completed in 5m26s with zero compiler warnings. This compiles the final
  supporter stack-scratch revision, but compilation is not a performance pass.
- The actual 64MiB P7 replay now crosses the previously failing first-geometry,
  pattern64 cell: raw352 / canonical352, retained4,928,000 bytes, checked
  peak4,979,040 bytes, allowance65,791,099 bytes. The full exact manifest scan
  was still running after two minutes in the debug test, with about24MiB live
  working set. This is evidence for the corrected memory boundary, not full
  replay completion or an acceptable GUI completion time.
- Bounded TypeScript timings are now opt-in independently of a profiling WASM.
  The local-recovery/local-audit worker enables them; production/development
  defaults do not. The local root route exposes only the most recent completed
  numeric transport/minimum-wave profile in a collapsed panel. Inputs, field
  encodings and task/candidate identities are excluded by an explicit allowlist.
  Per-worker interval sums overlap and are not wall-clock or pure CPU time.
- The local profile contract passed. The root route compiled without warnings.
  The worker entrypoint and source imports passed their TypeScript check.
  The running4194 listener remains PID11592; these observations do not constitute
  a server-restart test. Its published WASM remains0709 pending a new build.
- The final focused Rust set passed39 tests (including the corrected external
  eager trace-identity fixture and final15 assistance tests). The full P7 debug
  scan was stopped at the eight-minute diagnostic boundary: CPU483.88s and
  working set26.1MiB. It had not completed; no full-manifest success is claimed.
- A route observation change invalidated a source-regex test that required a
  literal `return new Worker`. Its assertion now checks creation and return of
  the same local worker plus the local-mode observation guard. Pages essential
  surface3/3, progress surface2/2 and lazy replay surface1/1 pass; this is not a
  relaxation of the local-only render boundary.
- Further actual-call-path audit found Build minimum, PC/Build score minimum,
  colored/supplied Build and spin coverage still finish through synchronous
  `new_canonical`; the PC-only continuation does not parallelize those callers.
  See [continuation wiring audit](exact-minimum-continuation-wiring-2026-09-06.md).
  This is unresolved implementation work, not an intentional product exclusion.
- The [replay count-DP candidate](pc-replay-exact-count-dp-design-2026-09-06.md)
  preserves canonical-language dedup, distinguishes deterministic certified
  graphs from generic graph unions, and separates source identity from trace
  stream hashing. It remains unimplemented and does not replace release proof.

## Published candidate512225 and real GUI scheduler diagnosis

- WASM build completed in12m15s with no compiler warnings. Published SHA256:
  `512225a45cd548eeb7988ad5ab6d945dc1f4dfdcdaacc657b9a601d02d1cf365`;
  source contract `360aa5e26cb1ba640bcecfece0e3f048e7e1e80a66bde71f1cb46fc667addebe`.
  Source freshness and4194 HTTP manifest parity both passed at publication. Retention
  kept5 generations and removed2 obsolete generated files. No release gate ran.
- With other compile/benchmark processes idle, the real GUI minimum completed
  at **190.0s**, exact25 /246 candidates /5040 patterns. Active11/11 was observed,
  with0/11 at completion. This is a regression, NOT a20s performance pass.
- The visible local profile contained27 exact waves. All times below are ms;
  local compute overlaps remote work, and remote interval sums are not elapsed:

  | Wave | Elapsed | Local compute | Local slices/tasks | Remote tasks | All initialized | Drain | Max remote roundtrip |
  | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
  | 1 | 50624.2 | 49839.7 | 4424/3 | 31 | 5883.9 | 18.6 | 28223.4 |
  | 2 | 33300.4 | 32521.4 | 4136/9 | 47 | 5194.5 | 15.0 | 10864.1 |
  | 3 | 80174.4 | 40553.3 | 10666/12 | 60 | 5370.1 | 37664.1 | 62131.2 |
  | 4 | 14839.5 | 14572.0 | 7861/37 | 33 | 3724.9 | 14.5 | 3872.2 |

- Individual durable-phase maxima reached806.5ms initialize.prepare,
  774.4ms initialize.published,909.3ms consume.published and853.7ms
  consume.running_commit. Wave1 averages11.27ms per coordinator slice. The
  mixed posted-message yield visits the timer lane every8 quanta (~90ms here),
  while one authority serializes ten workers' journal mutations. This makes
  coordinator task-source starvation a supported hypothesis, not yet a proven
  sole cause. Idle assistance must not be blamed solely from aggregate runtime.
- Full Build GUI completed with all246 solutions and5040/5040 success, but at
  **0.7s cold /0.4s repeat**, missing the then-current0.1s target (subsequently
  superseded by the accepted approximately0.4s boundary). Repeat profile had8 reused
  remote workers and2 cold instances (137.2ms sum /72.9ms max). The configured
  eager total9 means8 remote, and `prewarm(size)` can trim a larger warm pool.
  Consume40 tasks used60ms summed run-grant-to-reply; durable phase maxima were
  roughly6–8ms, much lower than during coordinator exact computation.
- An A/B candidate changes only the coordinator Runner host yield to the timer
  lane each8ms quantum. Remote yield, assistance, exact semantics, WASM binary
  and durable ACK boundaries are unchanged. Focused yield/Runner/Pool contracts
  and source TypeScript checks passed.
- The timer-only real GUI run completed at **92.5s**, again exact25/246/5040.
  This single paired comparison is51.3% less elapsed time than190.0s but still
  fails20s. Wave1 all-ready fell5883.9→194.6ms; consume.published maximum fell
  909.3→31.4ms. This supports a substantial coordinator task-source starvation
  contribution; it does not prove all remaining time is host scheduling.
  Timer wave elapsed values were23.213s /12.673s /43.639s /5.059s, with local
  compute16.187s /8.283s /29.062s /3.365s. Wave3 still had a30.372s maximum
  remote roundtrip. All27 waves and minimum semantics were preserved.
- User requested dedicated-manager evaluation and isolated full-processor
  regression checks. The automatic W=n-1 topology can use W remote compute
  instances plus one control-only manager within n admitted instance roles.
  Full W=n must not silently add an n+1 role; retain shared mode unless a separate
  complete admission contract exists. UI main-thread policy is a distinct role.
  Conditional dedicated-mode guards are being implemented; not yet published.
  The512225 timer candidate full-processor12 run completed at **78.4s**, exact
  25/246/5040;12/12 active and0/12 terminal were observed. Its28 waves consumed
  77.017s in total: first four18.486/12.554/34.054/4.870s, remaining24 waves
  7.053s. Auto11's27 waves consumed90.824s (first four84.583s, rest6.241s).
  Wave time includes concurrent computation and protocol waits; local CPU sums
  must not be added to it. These profiles do not yet label proof vs canonical.
  Full mode did not regress against auto in this single fixture pair, but this
  is not a before/after test of the unpublished dedicated-manager guards.
- Full Build with timer Runner completed0.9s cold /0.5s repeat in auto11 and
  0.4s in full12, all246 solutions and5040/5040 cases. The earlier mixed-yield
  repeat was0.4s, so these one-decimal single samples cannot establish a small
  yield-overhead regression. The historical0.1s criterion was not met, but is no
  longer a release gate under the user's latest clarification. Both GUI
  full-processor checkboxes were restored to automatic after measurement.
- All above GUI measurements were isolated from other CPU benchmarks/builds.
  New manager guard source changes after publication are not in512225; do not
  describe the running artifact as currently source-fresh without rechecking.

## Qnia reference boundary

[Qnia benchmark](https://github.com/Qnia28/sfinder_wasm/blob/9f2000252a99f6e8b25a0ffbf461d894a78c3766/scripts/benchmark-hard-minimals.mjs)
uses the horizontally mirrored 4L/P7/hold field (`0xf03c0f03c0`) and starts
timing after WASM solver setup. The isolated comparison harness explicitly
regenerates and validates the requested field (`0x3c0f03c0f`); see
`qnia-minimum-cover-stage-comparison-2026-09-06.md` for the actual stage timings.
[Public wrapper](https://github.com/Qnia28/sfinder_wasm/blob/main/src/minimals-wrapper.mjs)
defaults secondary human quality to Fast.
[Primary HiGHS adapter](https://github.com/Qnia28/sfinder_wasm/blob/main/src/highs-cardinality.mjs)
requires an optimal binary minimum-cardinality cover with zero relative gap.
[Adaptive Fast policy](https://github.com/Qnia28/sfinder_wasm/blob/main/src/min-cover-adaptive.mjs)
may refine an exact-K incumbent without proving the secondary quality ordering.
This is not Clearra's exact first canonical-family contract, but it remains a
useful performance reference and does not authorize relaxing Clearra semantics.
Review kernelization and reusable recertified bounds; no external code/dependency
was copied into the product.
