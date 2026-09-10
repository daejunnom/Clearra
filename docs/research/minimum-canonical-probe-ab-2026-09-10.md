# Canonical query preparation and bounded local evaluation

The source baseline is `b12307d` on `codex/v0.8.0-hotfix-minimum-algorithm-ab`.
The previous fastest 11-worker combination was TNF: median 14.046 seconds,
range 13.675–14.341, six samples in
[the section 6 evaluation](minimum-sections6-evaluation-2026-09-10.md).
That number is a median, not a mean. TNF, rather than the slower ordinary TF
policy, is the final comparison baseline. Historical profiling-enabled samples
are not pooled with this profiling-disabled experiment.
Qnia's 3–5 second GUI result was actually measured at
https://qniapc.vercel.app/sfinder/minimals. The user confirmed this provenance;
it is a measured result, not merely a proposed external target. This experiment
does not remeasure or test Qnia. A new SCP formulation/backend is deferred.

The owned change is in the existing original-ID canonical continuation.
Before publishing an unassisted canonical query, an ordinary exact cursor may
spend a bounded cooperative work budget on it. A completed positive or negative
answer decides the same original query without remote initialization/receipt
fanout. Exhausting the budget discards the probe and releases the unchanged
exhaustive partition frontier. It produces no negative authority or synthetic
receipt. The query ID is allocated only once and no task can issue during the
probe. Cancellation, transactional cloning, whole-live memory accounting and
lazy successor behavior remain part of the existing continuation contract.

This is a scheduling change around the current solver, not cross-query conflict
learning. It still prepares a partition frontier and can duplicate some root
work on a miss. The low-parallelism measurements below show why admission must
restrict the combination instead of forcing it globally.

The initial TF control retains M2/H/O/T/F (flags 185). The final TNF baseline
also disables redundant idle assistance (flags 249), while preserving the
positive-only global warm repair. Independent probe budgets are 8/32/128 total
cooperative work units, including preparation. No board, known K or canonical
identity selects these policies. The experimental flag width is extended to
16 bits; the policy setter remains absent from ordinary artifacts.

Verification follows `docs/test-policy.md`: use only the owned focused boundary,
not a full release matrix. This user-requested browser/WASM experiment is not a
fallback for a failed native test and grants no release authority. Existing
small-matrix oracle/tie checks will be extended only to cover the new bounded
handoff, clone, cancellation and output ordering. No new CI gate is required.

A/B uses the private GUI on4195, P7, `ctk3_w0kCQBjwwAMPPAD37g`,4L,Jstris180,
empty hold; the endpoint is actual first ProductResultPager paint after prewarm.
Batch count starts at3; runs are serial and no build/other solver overlaps them.
Device/browser/worker count/GUI/WASM/admitted memory and visibility stay bound
to each sample. Worker changes on this machine do not emulate another CPU.
Diagnostic profiles and timing runs without active profiling are separate.

## Adoption decision

Keep the existing M2/H/O/T/F behavior for requests below 32 partitions. For
requests of at least 32 partitions, use TNF plus 32 cooperative probe steps.
The existing browser scheduler requests four partitions per compute worker
when local execution is available. Thus its measured 4-worker case retains TF,
while 8/11/12 workers use the new combination. This is a rule about requested
scheduler work, not a fixed CPU model, a claim about actual core placement, or
a change to worker-count authority. Five through seven workers and different
CPUs have not been measured; smaller requests conservatively retain prior
behavior. Serial execution also retains its prior path.

The requested partition count is retained with each oracle so later scheduling
changes cannot change that query's idle-assistance policy. A/B flags deliberately
allow forcing a candidate below this admission threshold to reproduce the
regression. The ordinary D artifact applies the admission rule.

## Focused correctness evidence

The local WASM harness compares all optimal portfolios, in original row-ID
order, against an independent exhaustive subset oracle. It checked 510
coverable generated inputs under each of flags 185/441/697/1209 and
249/505/761/1273, spanning bit
positions 0, 63, 64, 65, 128 and 129. It also checked the entire 64-member
optimal family of a duplicate-row fixture. Receipts arrive in reverse order;
periodic transactional cancellation, rejected guarded clones and successful
clones must preserve the remaining ordered family. All eight policies passed.
The duplicate-row fixture delegated 45 queries with B and one with each probe
policy. Those small-fixture elapsed times are correctness-harness observations,
not product performance results.

The harness is local to the existing private experiment:
`_local/research/canonical-probe-properties-20260910/src/lib.rs` and
`_local/research/run-canonical-probe-properties-20260910.mjs` in the primary
checkout. Its result is
`_local/reports/minimum-canonical-probe-properties-20260910.json`.

The ordinary feature configuration separately passed the same 510-input
full-family, guarded-clone and cancellation checks at requested partition counts
4, 16, 31, 32, 44 and 48. In the 64-member family, delegated queries remained
45 below admission and fell to one at and above admission. The result is
`_local/reports/minimum-canonical-probe-ordinary-properties-20260910.json`.
These are local executed WASM checks, not a claimed native test or release gate.

## Same-binary browser A/B

The actual machine is Intel Core 5 210H, 8 physical cores and 12 logical
processors. The browser reported 12 logical processors and 16 GiB device
memory. Every accepted sample stayed visible; power, temperature and other
system activity were not instrumented. No compiler or second solver ran during
the timing batches. Each sample used fresh workers after explicit prewarm.
The endpoint includes production ProductResultPager rendering and two animation
frames. Prewarm is recorded separately in every raw sample.

All variants below used WASM SHA-256
`2b6c06052141f1686d72858e30e5e9bbfb7b2e99df1fdbe0b5da2068272d4660`,
source-contract SHA-256
`60473fc1b448272bf676ddb6abd8af68ad750313806076fe6a9cc1ab669bc1ad`.
The WASM had the experimental policy setter and no stage-profiling exports;
the product worker graph was built in production mode without transport profiling.
Only local menu/routing controls changed between server sessions. Each comparison
group retains its measurement-session identity and artifact/host/harness hashes.

### 11-worker final combination comparison

| Policy | Flags | n | Mean seconds | Median seconds | Min–max seconds | Published queries |
| --- | ---: | ---: | ---: | ---: | --- | ---: |
| TNF baseline | 249 | 6 | 15.839 | 15.781 | 14.331–17.823 | 27 |
| TNF + probe 8 | 505 | 3 | 17.207 | 17.068 | 15.454–19.099 | 27 |
| TNF + probe 32 | 761 | 6 | 12.759 | 12.868 | 11.900–13.142 | 9 |
| TNF + probe 128 | 1273 | 3 | 16.549 | 14.431 | 14.137–21.079 | 9 |

TNF and TNF + 32 each had a second batch of three in reverse order. The selected
combination reduced the median by 18.5% and the mean by 19.4%. These descriptive
statistics do not establish statistical significance or superiority on another
CPU. The larger 128 budget did not beat 32 even in combination with N; 8 did not
eliminate the later publications. Counting fewer queries alone is insufficient.

### Worker interaction and low-parallelism preservation

| Requested workers | Reference | Reference n / median seconds | Forced TNF + 32 n / median seconds | Decision |
| ---: | --- | --- | --- | --- |
| 4 | Existing ordinary TF | 3 / 43.014 | 3 / 73.094 | Retain TF; reject unconditional adoption |
| 8 | TNF | 3 / 17.863 | 3 / 15.676 | Adopt combination; median decrease 12.2% |
| 11 | TNF | 6 / 15.781 | 6 / 12.868 | Adopt combination; median decrease 18.5% |
| 12 | TNF | 3 / 18.648 | 3 / 17.225 | Adopt combination; median decrease 7.6% |

The forced 4-worker combination ranged from 53.539 to 74.603 seconds, while TF
ranged from 42.689 to 43.068. The historically fast HO policy was also refreshed:
64.885-second median, range 63.793–64.927, three samples in the current artifact.
Its older 40.732-second result is not interchangeable with this build/session.
The final admission rule preserves the current ordinary TF behavior at low fanout.

For 8/11 workers, published query counts fell from 27 to 9; for 12 they fell from
23 to 5. At four workers they fell from TF's 27 to 8 while time regressed. The
12-worker command explicitly used `--use-all-cpu-threads`: remote workers plus
a computing manager and 48 requested partitions differ from the 11-worker
control-only manager with 44 requested partitions. This is not an isolated
measurement of SMT overhead or cache misses. No universal worker count is fixed.

### Earlier TF-only screening

The first screening used the slower TF reference. Its 11-worker medians were
TF 21.076 seconds (n=6), TF + 8 16.872 (n=3), TF + 32 17.735 (n=6), and TF + 128
23.490 (n=3). TF at eight workers was 18.462 seconds (n=3). These results remain
in the raw record but did not determine adoption. Comparing combinations with
the previously fastest TNF policy changed the decision, as requested.

## Remaining bottleneck and evidence limits

At 11 workers, the median interval from the first canonical query publication
to actual paint fell from TNF's 10.666 to 7.492 seconds. Before that publication,
geometry, producer/drain work, K proof and query preparation together took a
median 5.279 seconds in the selected candidate. These are separate interval
medians; they must not be added to reconstruct the full-run median.

One selected sample published its first canonical query at 5.552 seconds. The
next canonical queries with limit 24 / 245 rows and limit 23 / 239 rows still
occupied publication-to-publication intervals of 3.484 and 1.859 seconds. These
intervals include intervening query/probe preparation, not just pure DFS time.
The remaining hard negative proofs dominate after the repeated later
publications are removed. The experiment has not reached Qnia's measured 3–5
seconds and has not remeasured Qnia or implemented the deferred SCP approach.

All 60 accepted A/B samples returned K=25, the same ordered 25 canonical members,
known alternative count 1, no claimed total count, lazy enumeration, and 25
rendered fields. Full members JSON SHA-256 was
`ca0c7b428c21ecec9728765d1d89485374af93620c14c1ffa050d02e7d1225ce`.
One unintended N128 low-worker batch was stopped and excluded in full: raw
files 55 and 56 contain one completed sample and one manual cancellation.
Its batch ID and reason remain in analysis.json; no failed/cancelled duration
was counted as a performance result.

The raw evidence and analyzer are in the primary checkout:
`_local/reports/minimum-canonical-probe-browser-20260910/` and
`_local/research/analyze-canonical-probe-20260910.mjs`.
## Ordinary artifact and post-build confirmation

The final ordinary WASM compiled successfully without either experimental
policy or stage-profiling exports. Its source-contract SHA-256 is
`5cfa5548ffafd32ec2e1515825be11277efe6a3f9a87d0447a50126876f8e9a4`;
WASM SHA-256 is
`38a2bead11346bd74e61a1aa0d7a161d6d7f49851ee5f7f704dd8fc96b08b103`.
The build verified that source contents did not change while compiling.

Absolute times changed after the lengthy build. The ordinary artifact's first
batch was around 18 seconds, so the same experimental N32 artifact and TNF
baseline were run again, followed by another ordinary batch. Both experimental
policies were also slower than in the selection cohort. No cause is assigned
to temperature, power policy or background activity because those were not
measured. The analyzer keeps this post-build cohort separate; its samples are
not silently pooled into the earlier policy-selection result.

| Post-build case | n | Mean seconds | Median seconds | Min–max seconds | Published queries |
| --- | ---: | ---: | ---: | --- | ---: |
| TNF baseline, 11 workers | 3 | 21.246 | 21.185 | 21.076–21.478 | 27 |
| Forced N32, 11 workers | 3 | 18.029 | 17.986 | 17.954–18.145 | 9 |
| Ordinary D, 11 workers | 6 | 17.626 | 17.696 | 16.785–18.418 | 9 |
| Ordinary D, 4 workers | 3 | 50.236 | 43.958 | 41.896–64.852 | 27 |

The ordinary 11-worker median was 16.5% below its contemporaneous TNF reference.
This is confirmation of the actual artifact under the later conditions, not a
claim that the ordinary artifact itself was observed at the earlier 12.868-second
median. At four workers the expected existing 27-query path was retained; the
large first-sample variation remains visible in the reported range and mean.
All 75 accepted browser samples across both cohorts passed the same output
contract; two records from the unintended/cancelled batch remain excluded.

## Portable private GUI

The complete local-only bundle is in the primary checkout at
`_local/benchmark-portable/minimum-canonical-probe-20260910/`, with a ZIP beside
the folder. It contains the private production-result GUI, the frozen A/B WASM,
the ordinary WASM, a Node server, README and the compact A/B analysis. Running
`node serve.mjs` binds only 127.0.0.1:4195 for 45 minutes and records results
locally with a fresh machine/session identity. It does not start a benchmark
automatically or send data externally. `node serve.mjs --verify-only` passed
integrity checks for both WASM/bindings artifacts and all 13 GUI assets.

Bundle identity SHA-256:
`1bbf9261bd61446715b71b562e766ddbdb68dcf3206a4b1d57cd630bfec9818d`.
Actual other-PC timing still requires running the bundle on those PCs. Reducing
the worker count on this machine does not provide such evidence.
