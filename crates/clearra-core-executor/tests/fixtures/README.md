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
