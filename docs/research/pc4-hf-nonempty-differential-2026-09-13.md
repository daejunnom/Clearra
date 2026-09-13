# Nonempty PC4 graph observations and the completeness boundary

This is a source/test research record, not an activated profile or a release
receipt. It extends the [root observation](pc4-hf-root-differential-2026-09-13.md).

## Observed data

The local-only `nonempty-targets.mjs` probe (normal and `--upper-only` runs) used
the same historical HF observation revision as the root fixture. It requested
only exact HTTP 206 ranges with bounded streamed bodies and maximum concurrency
four. The two runs read 3,130 binary bytes in 316 requests, including binary
search of the field index. No whole graph/index was downloaded. The revision
is retained only in test evidence, not pinned in production code.

| Source case | Indexed node | Outgoing edges |
| --- | ---: | ---: |
| Two horizontal I pieces | 13 | 103 |
| Prior synthetic upper-clear counterexample | index miss | not an empty adjacency |
| One cleared row and six live cells | 10,664 | 48 |
| Vertical I completing four rows | 10,538,099 | 1 |
| Three cleared rows and a final horizontal I | 14,996,338 | 1 |
| Tileable initial field allowing an upper-row clear | 10,958 | 41 |

The missing counterexample does not invalidate its previous physical-coordinate
regression, nor prove a user's PC query unsatisfiable. Arbitrary initial fields
need the explicit miss/fallback contract, even when a particular lock is legal.

## Exact edge reconstruction

For all 194 observed edges, independently enumerated forward reachable locks
and an explicit row packer agree with the full inverse materializer output:
rotation, physical x/y, four occupied cells, and cleared-row mask. The sample
contains 11 clearing edges, five involving upper rows, and four non-monotone
source/target occupancy transitions. No additional edge-coordinate mismatch was
observed. This does not yet prove request-wide reducer/replay identity parity.

## Physical placements are not the same set as PC-capable graph edges

The [upstream format/limitations documentation](https://github.com/muse918/hydra-optimal/blob/856b67b079ea3e6d2648eb4f4e03025c226130f6/README.md#compatibility-and-limitations)
describes the graph domain as four-line-PC-capable fields reachable from empty
under its placement model. Therefore requiring every one-step physically legal
lock to occur in the graph is too strong. Conversely, silently intersecting
the two sets would conceal a missing viable solution.

The comparison instead accounts for all 170 additional local forward edges:

- 109 have a fully occupied column separating strips with non-multiple-of-four
  vacancy counts. The wall remains solid as rows clear and target height
  shrinks, so a tetromino cannot cross it: this is a negative completion proof.
- 48 have no exact cover even in a generous inverse-clear relaxation. The
  independent test oracle lifts each tetromino's occupied rows into every
  increasing target-row subset and ignores supply, timing, kicks and
  reachability. Actual inverse-clear placements are contained in that domain;
  its exhausted exact-cover failure proves impossibility. A feasible cover
  would not prove a playable completion.
- 13 remain unresolved. Their exact case/piece/target identities are recorded
  by `observed_nonempty_edges_are_exact_but_profile_completeness_is_unqualified`.
  That regression preserves an unqualified observation; it is not an allowlist
  for dropping those transitions in production or a completeness pass.

Two bounded forward-PC proof experiments stopped at 50,000 states (17.49s and
51.97s). An optimistic clear/reachability relaxation did not close the problem.
Budget exhaustion is unknown, not an impossibility result. The experiments are
preserved only under the local report directory, not compiled into product or
default CI; neither the normal first-success kick policy nor product search
was changed to make the comparison agree.

## Next qualification work

Resolve the 13 omissions with an efficient exact ordered-completion proof or
construct concrete counterexample completions, then compare the actual upstream
placement/kick model. Keep the canonical profile unqualified meanwhile. Check
the other four graph/index/profile combinations independently. Also finish the
multi-edge App/reducer/replay and per-target 1–4L tests before closing the plan's
materialization or profile-completeness checklist entries.

Local raw evidence and experiment source:
`C:/Users/강민수/AppData/Local/Clearra/reports/pc4-index-semantics-20260913/`.
The small MIT observation fixture is compiled only under `cfg(test)`; no
external solver implementation, V*, policy or Krylov logic was imported.

## Local regression result

Managed `cargo test --locked --offline --quiet -j 2 -p clearra-core-executor
--lib pc4_graph_materializer -- --test-threads=2 --nocapture` passed 7 tests,
zero failures/ignored tests (532 unrelated tests filtered), in 0.08s test
runtime. This includes positive and negative controls for the relaxed-cover
oracle. The comparison explicitly prints `qualification=not_qualified` and
the 13 unresolved identities. The PC4 authority validator mutation suite also
passed. These local test changes have no exact-SHA hosted acceptance receipt.
