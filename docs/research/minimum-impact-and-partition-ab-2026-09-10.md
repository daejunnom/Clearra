# Minimum search: impact ordering and residual partition A/B

## Scope

This follows [the A/B candidate review](minimum-parallel-overhead-and-algorithm-plan-2026-09-08.md)
on `codex/v0.8.0-hotfix-minimum-algorithm-ab`, starting from `2e9cdd3` and its
uncommitted H draft. The much older `codex/min-cover-performance` branch is not
the current minimum solver baseline. Production/main and deployment workflows
are outside this experiment.

The fixture is `ctk3_w0kCQBjwwAMPPAD37g`, P7, 4L, empty hold, Jstris 180:

```text
clearra pc minimals --lines 4 --board-mask 0x3c0f03c0f --height 4 --pieces 6 --patterns P7 --hold empty --rule jstris-180 --backend cpu --cpu-warmup --workers 11
```

The minimum of 25 is an output check only. No known answer, selected candidate
IDs or cached result enters the solver. Original-ID canonical selection and
lazy subsequent alternatives remain required. The Qnia 3–5 second reference
includes a different first-witness endpoint; it is not a fresh Qnia measurement
or a reason to remove Clearra's canonical proof.

## Changes reviewed

- **V0:** an active search profile incorrectly counted as a competing executor
  owner, rejecting serial job start/advance. Profiling now observes that job;
  distributed/transfer owners still conflict, and governed-output admission
  continues to account for profiling. This fixes the invalid W1 comparison.
- **H:** complete rarest-pivot children are ordered by descending uncovered
  coverage gain, with original ID breaking ties. Child `i` excludes exactly its
  predecessors in that permutation. Every child remains present, negative
  closure still needs every actual receipt, and final canonical ID order does
  not change. Sorting uses existing storage.
- **O (O1 experiment):** target queue depth is capped at one task per four
  remaining candidate rows. This is an input-derived scheduling hypothesis,
  not an admissibility cutoff. Splitting still emits all children even above
  the target. Compare it separately from H before choosing a default.

Previously rejected repair deletion, rounded-component bound and canonical
bisection remain off. Incremental learned canonical queries and stronger
integer reasoning (B1/B2) are separate algorithm changes; this experiment does
not claim to implement them.

## Measurement boundary

Windows, Intel Core 5 210H, 12 logical processors; 11 compute workers plus the
existing reserved-controller policy. Chromium uses an isolated profile for each
serial block, and a fresh page/product worker graph for each sample. Browser
and IndexedDB state can persist within a block; order reversal reduces but
does not eliminate that source of timing bias. The one HTTP experiment
server is `127.0.0.1:4195`, with a 45-minute lease and no automatic restart.
The app Browser runtime failed to initialize twice with an OS path error, so
the existing local harness was adapted to an owned Chromium process.

The actual production `ProductResultPager` renders the returned first set.
`first_product_paint_ms` ends after Svelte's update and two animation frames;
all 25 `.solution-board[role="img"]` elements must exist. This measures the
production result component inside a benchmark shell, not the entire app's
input/bootstrap interaction. Module download/prewarm is recorded separately.
No next-alternative loader is supplied and no hidden tie enumeration is run.
Builds and benchmark runs do not overlap. Samples run sequentially, including
order reversal; this is not a dedicated thermally controlled laboratory.

Same-binary comparison artifact:

- Rust 1.96.0, wasm-bindgen 0.2.126, release wasm32-unknown-unknown.
- Features: `stage-profiling,minimum-hotfix-ab`.
- Source contract SHA-256: `0b1819e805af333a3e162e275ce5bf165bfcd896b96b54501bd3ac515b0c8735`.
- WASM SHA-256: `cdb3491787976fd3a5c5b5018d9080b169d1baa7b547058959ee5be0bc40b30a`.
- Runtime identity is `unverified-local-build`; this artifact is not release acceptance.
- Flags: M=1 (baseline overlapping repair), H=9, O=17, H+O=25.

## Correctness and exclusions

The compiled modified product solver was checked in wasm32 against independent
exhaustive subset enumeration: all 4,096 four-candidate/three-constraint
matrices, all five cardinality limits, for M/H/O/H+O. All **81,920 decisions**
agree; every feasible subset belongs to exactly one partition. This is bounded
exhaustive regression evidence, not a proof for every possible input.

Each successful P7 set is checked against the source-bound original matrix
`63de33e1d86077c179a38f6311df893ba9abcc13c9b18fa433384f5961eeee91` (246 candidates,
5,040 queues), including matrix, candidate and queue bindings. Every queue must
be covered; selected original IDs and normalized keys must match across arms.
Expected canonical-members SHA-256 is
`7314db27276521fe547b76236fd196726c7a40cbc4ef2af2935ea2363df04d8c`.
Known alternatives stays 1, total alternatives stays null, enumeration_complete
stays false.

The first W1 smoke mistakenly used `I,P6`, which generated a much wider search
than intended. It was explicitly stopped and retained as `fixture_error`, not
a timing success. Corrected fixed queue `IOTSZJL` completed in 83.320 ms and
rendered its one-field result in 133.460 ms, confirming profiling no longer
blocks serial execution. These smoke results are not P7 speed samples.

Existing ClearraWasmRuntime and DistributedWasmJobRunner TypeScript contracts
pass. No new CI gate or native product/test execution was added for this A/B.

## Results and decision

The sequential orders were `M H H M`, `M O O M`, and `H HO HO H HO H H HO`.
All sixteen P7 runs succeeded and rendered 25 fields.

| Policy | Samples | First result paint median (s) | Range (s) | Remote task median |
| --- | ---: | ---: | --- | ---: |
| M baseline | 4 | 26.642 | 25.978–27.812 | 928.5 |
| H impact order | 6 | 17.756 | 17.607–18.118 | 894.5 |
| O residual budget | 2 | 25.072 | 25.014–25.130 | 802.5 |
| H + O | 4 | 17.303 | 17.245–17.794 | 816 |

Compared with the M median, H+O reduces first result time by **35.05%** (median time ratio 1.540:1). H alone supplies most of the gain;
O adds about 2.55% relative to H's median. Both order-reversed H/HO blocks favor
the combination. Ranges overlap and samples are few: this is not a claim of
statistical significance or a universal speedup.

All arms use 29 proof/canonical waves. Median source, drain and verifier-finish
stages remain roughly 258–284 ms, 43–48 ms and 68–71 ms. Finalize falls from
26.176 s to 16.868 s. Controller query preparation remains about 46–47 ms;
renderer handoff adds roughly 40–60 ms. Thus most of the improvement lies in
proof search after changing disjoint root order, not in rendering or WASM load.
Nested/parallel clocks are not added together as CPU time, and these existing
profiles do not identify every wave as K-proof versus canonical purpose.

**Decision:** enable H and O in the branch's ordinary compiled defaults; keep
M2 overlapping repair and leave C/G disabled. The local A/B setter remains
feature-gated. Two existing protocol tests explicitly request their multiple
root obligations, so queue-depth tuning does not silently turn those tests into
single-shard cases. Both experimental and ordinary-default test source compile
for wasm32. Ordinary-default wasm32 also passes a separate 20,480-decision
exhaustive matrix check, for 102,400 decisions including the four A/B arms.

The **3–5 second reference and the previous 3-second first-canonical target are
not reached**. B1/B2 remain the next distinct work-reduction candidates. No
known answer, arbitrary optimal witness or weaker stopping condition was used
to manufacture a faster result.

### Every valid P7 first-paint sample (ms)

- M: 27812.155 / 25977.590 / 27087.340 / 26197.405
- H: 17728.845 / 17729.510 / 17607.370 / 17783.445 / 18117.715 / 17982.150
- O: 25130.115 / 25013.730
- HO: 17285.915 / 17320.175 / 17244.525 / 17794.365

### Ordinary-default artifact confirmation

The rebuilt artifact has `stage-profiling` for measurement and **no
`minimum-hotfix-ab`**. Its actual WASM exports were inspected and the A/B setter
is absent. It uses the selected H+O defaults without host policy injection.

- Source contract SHA-256: `e048a6dd58eeeae8ce6dc69986d202b1a953a245146ee286176107a77b9a287d`.
- WASM SHA-256: `345d3bc2e5839eab0b4ec78c40d6530037507265b631a7e9aee4f5f99c3ccaa9`.
- Fixed-queue W1 smoke: 101.815 ms terminal / 153.355 ms result paint, one field.
- **P7 W11: 17,095.060 ms terminal / 17,138.945 ms result paint**, all 25 fields.
- The default P7 result passes the same canonical-members hash, complete original
  5,040-queue coverage and lazy-alternative checks as every A/B result.

These ordinary-default confirmations are not mixed into same-binary A/B
statistics. The result remains local verification, not deployment acceptance.
No remote CI polling, main merge or deployment is part of this experiment.


Local raw records: root checkout `_local/reports/minimum-browser-ab-20260910/`.
The local server, browser driver, summary checker and tiny WASM property driver
remain under root `_local/research/`; external reference code and raw artifacts
are not copied into the product or CI.
