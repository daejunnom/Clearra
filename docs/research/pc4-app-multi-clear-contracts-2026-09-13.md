# App range-to-candidate multi-clear contracts

This adds synthetic composition coverage; it is not HF profile qualification,
a whole-solution completeness certificate, or a production release receipt.

## Coverage added

`online_pc4_fixed_queue_candidate_session_path_tests.rs` owns four tests, one
for each target from 1L through 4L. Each runs against all five typed profiles.
The tests use the public disclosure-ready request factory, a pinned synthetic
generation, byte-range admission, the actual lookup/materializer/path runtime,
and the completed reducer boundary. Existing lifecycle tests share only their
test fixture helpers, not a second product implementation.

In the 2L through 4L cases the top row clears first, followed by the lower rows.
The graph index is sorted by occupancy hash, not traversal order. This makes
the test exercise both a hash-decreasing normalized transition and nontrivial
field IDs. Concrete observations are limited to one path per page.

Assertions cover:

- no reducer input before complete exhaustion and a bounded fixture step count;
- one canonical field identity, with no overlap between initial cells and
  placements and exact coverage of the original target rectangle;
- original-row occupied masks across each clear while physical x/y and
  rotation stay appropriate for replay;
- every retained replay's graph IDs, placement order, explicit line packing
  and final empty board, including paging across different rotation witnesses.

The small synthetic graphs are intentionally not evidence that their profile
labels match real uploads or that the real graph contains every valid path.
They do not resolve the 13 missing-edge completeness questions in the
[nonempty observation audit](pc4-hf-nonempty-differential-2026-09-13.md).
Actual 1L through 4L terminal and outgoing-domain qualification stays open.

## Local validation boundary

The managed row-normalization helper reran the core materializer suite:
**7 passed, 0 failed, 0 ignored**, 532 filtered, 0.07s test runtime. The next
TB test executable was blocked before execution by Windows application
control (`os error 4551`). A separate compile-only App test check was also
blocked before compiling App, at the `proc-macro2` build script, with the
same policy error. Neither is a product assertion failure or a local App pass.
No policy bypass, alternate execution mode or trust-setting change was made.

Rustfmt parsed/formatted the changed Rust modules, `git diff --check` passed,
and the isolated workflow boundary tests passed all nine cases. The PC4
full-solution authority validator mutation suite also passed. Executable
validation of the new four App tests is delegated to the existing read-only,
non-publishing Linux `pc4-contracts` CI job on the exact submitted source.
Do not mark the materialization/completeness checklist closed from this entry.

## First hosted result and correction

Run `34737351001` executed the new tests: 1L passed, but the 2L through 4L
cases stopped at the fixture's accidentally one-edge traversal budget. The
materialization budget also incorrectly allowed only one edge. Both now equal
the fixture's exact line/path length; one-observation-per-page is unchanged.
The initial hosted result was 61 App tests passed and three failed, not a
multi-clear parity pass. Core 7 and TB 156 passed on that source.

This also identified an App diagnostic loss: the session retained only a
generic budget reason. It now retains the original typed stage, limit and
attempted count, with a focused mapping regression. This does not raise a
product budget or turn an incomplete family into reducer input.
