# PC all-solutions worker scheduling audit — 2026-09-12

Status: implementation and local A/B complete. This branch is research and
performance work; it does not authorize a production deployment or release.

## Conclusion

The current PC all-solutions slowdown was not caused by Discord command parsing.
The browser worker pool was ready, but the coordinator generated and serialized
the complete geometry candidate stream before workers could verify it. At the
current source baseline this left only 6.49 of 8 workers active on average while
the producer was still running. The v0.8 search also performs substantially more
BuildUp/reachability work than the older exact artifact, so restoring scheduling
alone does not recover the complete historical runtime.

The implemented path sends canonical PC multiset roots to browser workers.
Each worker independently enumerates geometry and performs exact verification;
the coordinator merges a compact, replay-safe summary per root. Work is assigned
to the next ready worker, so an idle worker steals the next unclaimed batch rather
than waiting for a fixed shard owner.

Natural roots are never split into synthetic sub-roots. One requested compute
slot remains the coordinator and the rest are verifier workers. The browser
chooses a root batch size dynamically as

```text
ceil(root_count / (active_verifiers * 4)), capped at 64 roots
```

This gives P7 with 140 roots and 8 requested compute slots seven verifiers and a
five-root batch. Seven requested slots use six verifiers and a six-root batch.
Both leave about four dispatch waves per verifier for load balancing while
preserving enough work inside each durable worker transaction to amortize
transport and verification overhead.

## Benchmark boundary

- Host: WSL2 Ubuntu 24.04 with `.wslconfig` explicitly assigning 8 logical
  processors; every accepted result reports `hardware_concurrency = 8`.
- Browser: headless Google Chrome 153.0.8010.36.
- Browser isolation: cross-origin isolated. WSL Chrome required the benchmark's
  explicit `--no-sandbox true` opt-in because its sandbox imposed an 8 GiB
  renderer data limit and terminated this workload. This opt-in is confined to
  the local benchmark runner.
- Command: `clearra pc --lines 4 --count unique --source-pieces 11` with P7,
  `--max-patterns 5040`, `--max-candidates 100000000`, CPU warmup, and the worker
  count shown below.
- Each row is one complete run. Other local computation was active, as requested,
  so absolute time varies between later repeats. Exactness fields are checked
  separately from elapsed time.

## Measurements

| Source / policy | Requested slots | Time | GUI active slots while searching | Worker batches | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| pre-v0.8 exact artifact `b10b4356` | 8 requested, 7 compute | 104.73 s | 7.00 mean | legacy | exact |
| current baseline `c97090b` | 7 | 241.76 s | 5.72 mean, 2–7 | 14,580 | exact |
| current baseline `c97090b` | 8 | 233.81 s | 6.49 mean, 2–8 | 14,581 | exact |
| root-worker v1, dynamic six-root batches | 7 | 153.46 s | 7.00 mean, 7–7 | 24 | exact |
| root-worker v1, dynamic five-root batches | 8 | **142.17 s** | 7.95 mean, 7–8 | 28 | exact |
| root-worker v2, one root per batch | 8 | 159.74 s | 8.00 mean, 8–8 | 140 | exact |
| final-policy repeat, dynamic six-root batches | 7 | 176.44 s | 7.00 mean, 7–7 | 24 | exact |
| final-policy repeat, dynamic five-root batches | 8 | 168.28 s | 8.00 mean, 8–8 | 28 | exact |

For the current root-worker path, the GUI active count includes the coordinator
until production completes. The 7-slot runs therefore use six verifier workers;
the 8-slot runs use seven. After the 140 roots have been dispatched, only the
remaining verifier work appears in the drain count.

The best comparable optimized sample is 142.17 seconds: 39.2% faster than the
233.81-second current-baseline 8-worker run. Seven to eight workers improved the
paired v1 samples by 7.4% and the later same-build samples by 4.6%. The baseline
improved only 3.3%, which is consistent with its coordinator bottleneck.

The one-root A/B is important despite its full 8/8 utilization. It shortened the
final drain to about 2.76 seconds, but raised durable consume transactions from 28
to 140 and the summed worker consume interval from 843.82 to 1,067.05 seconds.
Total elapsed time regressed by 12.4% against the fastest five-root sample. Full
occupancy by itself is therefore not a sufficient optimization target.

One final-source 8-worker attempt was excluded before the accepted repeat because
the WSL VM restarted after about 83 seconds and left zero-byte stdout/stderr. It
did not produce a Clearra failure event or search result. The restarted VM again
reported 8 logical processors and sufficient free memory.

The accepted final-policy benchmark WASM is
`6e3f6b20368144b41d27368f102467097a39a583c2f06b8fae2970ac69cefaa2`.
After those runs, `rustfmt` changed only the source layout of the
`root_task_parallel` match expression. The final provenance build has identical
size and bindings but WASM hash
`5606d0fb0d695401d632903bbb29b73ea2779597d64f7f712a506a6b2b812fa0`.
No scheduling or search expression changed, so the long-running performance
measurements were not repeated for that formatting-only binary difference.

## Exactness checks

Every accepted current-baseline and optimized run produced:

- 456,923 unique solutions;
- normalized solution-set hash `cts1:98ebe8726537b29f`;
- 29,856,840 geometry candidates;
- a complete, untruncated result.

The optimized path uses an order-independent candidate digest
`d6715a89054ef642` because roots may finish in any worker order. The baseline
digest `3cee33ca5c75ccd0` is traversal-order dependent. The matching normalized
solution-set hash and count provide the cross-policy result identity check.

The current baseline and optimized runs both report 637,208,005 total build-order
nodes. The older artifact reports 174,829,288 under its older search pipeline.
Source history places the large work increase in the v0.8 stabilization changes,
which expanded exact BuildUp, reachability, finesse, hold, and resource checking.
Discord request parsing is outside this geometry/verification hot loop. Reverting
those checks wholesale would change correctness and resource semantics, so this
branch fixes the independent scheduling bottleneck instead.

## Implementation boundary

- The root-worker route is limited to the plain CPU PC `Unique + CountUnique`
  path whose result semantics permit the order-independent digest. Score,
  probability, observation, constraint, tablebase, and other specialized paths
  retain their existing producers.
- Candidate rank remains deterministic by combining the canonical root ordinal
  with the worker-local ordinal. The final normalized family is independent of
  completion order.
- Each root returns a compact summary containing its exact candidate count,
  digest, geometry metrics, and completion evidence. Candidate identities are
  not copied back through the coordinator.
- The coordinator accepts only one matching terminal commit per root. Duplicate
  replay is idempotent; mismatched replay, a missing root, an invalid ordinal, or
  an incomplete transcript fails closed.
- The existing ready-worker queue supplies work stealing at batch boundaries.
  A worker remains independent until it finishes its current batch, then receives
  the next unclaimed batch if one exists.

## Local evidence files

The raw JSON and stderr files remain outside Git under:

```text
C:\Users\강민수\AppData\Local\Clearra\benchmarks\pc-all-worker-20260912
```

The accepted result files are `legacy-b10b4356-w8-valid.json`,
`baseline-c97090b-w7-on-demand.json`, `baseline-c97090b-w8-on-demand.json`,
`optimized-v1-w7-on-demand.json`, `optimized-w8-on-demand.json`,
`optimized-v2-w8-on-demand.json`, `optimized-v3-w7-on-demand.json`, and
`optimized-v3-w8-on-demand.json`.
