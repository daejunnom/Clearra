# Real HF root adjacency versus Clearra

## Observation and execution

Reused the canonical graph's previously read empty-field record. A bounded
follow-up resolved all 162 target IDs through `field_hash_to_id.v1.bin` at the
same observed revision. All requested records contained their expected IDs.
Read 2,336 binary bytes in 79 coalesced requests, at most four concurrently;
required exact HTTP 206/Content-Range/total length and rejected oversized or
short bodies. Local probe and raw report:
`C:/Users/강민수/AppData/Local/Clearra/reports/pc4-index-semantics-20260913/root-targets.mjs`
and `root-targets.json`.

The small MIT dataset observation is preserved as test-only JSON in
`crates/clearra-core-executor/tests/fixtures/pc4-hf-root-20260913.json`.
It does not bind production code to that revision.

The Rust test independently generates all empty-board bottom-locked shapes
from the piece registry, without using Geometry or the materializer to build
the expected set. It compares sets in both directions and then materializes
every observed edge under Jstris180. Per-piece target counts in IJLOSTZ order
are 17, 34, 34, 9, 17, 34, 17: **162 total, all equal and all realizable**.

Managed core test run: **5 passed**, zero failures, 0.03s test runtime.
PC4 full-solution authority validator mutation tests also passed. These results
cover root layout/target interpretation and local realization, not upstream
completeness, kick-profile distinctions, nonempty boards, or product activation.
No builds, HTTP probes or tests from this follow-up remain running.

## Concurrent v0.8.1 surface check

The integration checkout already contains the earlier minimum paging/Discord
projection changes. Direct Node tests returned **20 passing typed-product-result
tests**; capability-registry and GUI coverage-copy test files failed to load
because this checkout lacks `ctk3` and `esbuild` respectively. Their test bodies
did not execute. The main checkout resolves esbuild but not ctk3. This is missing
local workspace preparation, not evidence of product correctness or a confirmed
product regression. Do not mark surface parity complete from this run.

Next: prepare the integration checkout's exact workspace dependency/CTK exports
through the managed build path (without substituting main's workspace source),
then execute the two unrun test files. The v0.8.1 exact-SHA release gate remains
separate from these v0.9.0 graph observations.
