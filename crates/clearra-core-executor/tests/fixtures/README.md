# Bounded PC4 upstream observations

`pc4-hf-root-20260913.json` contains the 162 outgoing target IDs and occupancy
bitmaps of the empty field in the canonical `graph.bin` from
https://huggingface.co/datasets/muse918/tetris-4lpc-mdp-vstar-policy .
Dataset data is used under the stated MIT license and the user's confirmed
permission. No external solver implementation was copied.

The resolved revision is recorded only to reproduce this historical test input.
The fixture is included under `cfg(test)` and is not a production manifest,
runtime revision pin, graph-completeness certificate, or rule-profile approval.
The source record was reused from a previous bounded Range observation. Target
bitmaps required 2,336 additional index bytes in 79 exact HTTP 206 requests;
adjacent IDs were coalesced and requests used a maximum concurrency of four.
No complete graph/index file was downloaded.

The test compares each piece's entire sampled outgoing set in both directions
with independent empty-board shape placements, and verifies every edge through
the actual materializer. This covers the empty field only. It cannot distinguish
kick profiles there or validate nonempty fields, row clears, a whole dataset,
or complete user solution/replay families.

## Nonempty and row-clear observation

`pc4-hf-nonempty-20260913.json` is another test-only observation of the same
MIT dataset revision. It records five indexed source fields with 194 outgoing
edges and one explicitly missing initial field. The latter is not a zero-degree
node and is never used as evidence that the user's PC query is unsatisfiable.
The local probes `nonempty-targets.mjs` (ordinary and `--upper-only` runs) read
3,130 additional binary bytes in 316 exact HTTP 206 requests, with at most four
requests in flight and explicit total/per-request limits. Index search is
included in those counts. No complete graph/index was downloaded.

The test uses exhaustive forward motion for the expected locks and an explicit
forward row packer, separately from the inverse-edge materializer. Comparing
the raw one-step sets alone is not a completeness test: the graph may remove
physically legal transitions which can never finish a four-line PC. Profile
qualification requires a negative completion proof for every omitted forward
edge, or an expanded graph which includes each viable omission. The regression
records 109 dead strips, 48 projected-cover failures and 13 unresolved omissions
explicitly. Those 13 are an unqualified observation, not a production allowlist.
The test never silently intersects the graph and local sets. These bounded
cases do not qualify the complete graph or any production profile.
