# Column-mod-four Geometry A/B (2026-09-12)

## Scope

This is local-only algorithm evidence for the v0.8.1 additive residue filter. It
does not form release authority and is not part of the product test matrix.

The harness exercises 10-wide PC targets from 1L through 6L, including odd-line
targets with normalized non-empty initial fields, residue-rejecting fields, and
mixed-piece 4L/6L searches. It uses one deterministic worker so scheduling noise
does not change the explored search tree.

## Method

Windows Application Control rejected a separately built historical-baseline
test executable. The accepted comparison therefore uses one test binary and a
test-only atomic switch around only the column-mod-four necessary-condition
check. Product builds compile the switch out and always execute the filter.

The arm order is A-B-B-A. Every arm warms every fixture before recording 15
samples. A is the existing exact column/checker projection without the new
column-mod-four test; B enables it. Candidate identities and final normalized
solution-set hashes are asserted stable within every arm and compared across
arms.

Command:

```powershell
cargo test -p clearra-core-executor benchmark_column_mod_four_residual_filter -- --ignored --nocapture --test-threads=1
```

Toolchain: Rust 1.96.0 debug test profile. Absolute timings therefore describe
this diagnostic binary, not a release-WASM service-level objective.

## Result

All ten fixtures produced the same Geometry identity hash, final solution hash,
Geometry count, and final solution count with the filter off and on.

The two material mixed-piece fixtures were:

| Fixture | Expanded nodes A | Expanded nodes B | Node reduction | Geometry median A | Geometry median B | Median reduction | Total median A | Total median B | Median reduction |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| mixed 4L IOTJL | 748 | 611 | 18.3% | 14,114 us | 10,035 us | 28.9% | 19,422 us | 14,779 us | 23.9% |
| mixed 6L IOTSZJ | 16,336 | 13,917 | 14.8% | 507,667 us | 381,687 us | 24.8% | 600,045 us | 448,465 us | 25.3% |

Each timing cell is the mean of the two arm medians, not a pooled-sample median.
The node count is deterministic and is the stronger algorithmic signal. Small
fixtures remained at 1-32 expanded nodes; their microsecond timings were too
small and noisy to support a regression or improvement claim. No fixture gained
expanded nodes.

## Decision

Keep the column-mod-four necessary-condition filter enabled in the v0.8.1
candidate. Keep the A/B switch and ignored harness on this research branch only.
Promote only the extended-board correctness regression to the product branch.
Release acceptance still requires the plan's broader rule-profile differential
and release-WASM performance evidence; this local run does not replace it.
