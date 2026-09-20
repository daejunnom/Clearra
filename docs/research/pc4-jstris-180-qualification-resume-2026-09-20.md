# Jstris 180 4L qualification resume

## Decision

The current generation cannot honestly receive a PC Search target
qualification receipt. The immutable generation and its three canonical
artifacts are now exhaustively structurally bound, and the tracked root plus
nonempty known-answer observations are bound to those exact bytes. That closes
neither of the two semantic proofs required to claim exact target authority:

- `outgoing_edge_completeness_identity` remains absent. Parsing every encoded
  edge and proving that every target is in the indexed domain does not prove
  that the upstream producer omitted no PC-capable transition.
- `offline_exact_parity_identity` remains absent. The bounded materializer/KAT
  corpus and the thirteen omitted-transition negative proofs are not a complete
  offline-vs-tablebase PC Search result-family comparison.

Accordingly `pc_search_target_lines` must remain empty. Setup Search remains a
separate qualification problem and was not evaluated or enabled here.

## Deterministic structural and KAT audit

`scripts/release/pc4/audit-local-pc-search-generation.mjs` opens the existing
explicit benchmark dataset through its managed pointer and scans the complete
field index, complete offset index, and complete graph. It:

- streams and re-hashes every artifact byte against the immutable discovery
  identities;
- validates every ordinal field ID and strict field ordering;
- validates every offset, record boundary, source-field hash, piece group, and
  target ID;
- checks the full four-row terminal and its empty outgoing set;
- checks the tracked empty-root and five indexed nonempty graph records exactly,
  every referenced target's ID/hash pair, and the one tracked index miss;
- emits only `clearra.pc4.structural-kat-audit.v1`, with
  `authority=non-target-qualification-evidence`,
  `qualification_status=not-qualified`, and
  `pc_search_target_receipt=null`.

The exact emitted receipt is preserved in
`pc4-jstris-180-structural-kat-audit-2026-09-20.json`. Its audit identity is
`sha256:53c1343ca37f195429fa1e945762aa5b622996b17c312b71498ea330bdd53225`.

The complete scan covered:

- 15,185,706 field index records;
- 15,185,707 graph offsets;
- 15,185,706 graph records;
- 109,562,993 encoded target edges;
- all 693,145,959 artifact bytes;
- six tracked source records, 361 tracked field identities, 356 target
  references, and one indexed miss.

All three recomputed artifact identities matched the discovery generation at
`ea61380b31fa3dc9ffb4c8505c9a09c1b421ef31`. The bounded KAT identity is
`sha256:b5e90cd4ca7f71dc84ecb4d9f6715148a4c8b163a412f8546f6ef51c7164b3db`.

## Exact omitted-transition oracle

The prepared ignored Rust oracle was rerun through the documented managed build
owner, without bypassing the repository build guard:

```text
cargo test --locked --offline -j 2 -p clearra-core-executor --lib \
  classify_all_hf_omitted_pc4_targets_with_exact_completion_receipts \
  -- --ignored --nocapture
```

Exactly one selected test passed. All thirteen cases were exhaustively
classified dead, with live 0, unknown 0, and no budget exhaustion; the oracle
test itself completed in 11.21 seconds. This reconfirms the narrow
`all-omissions-dead` component. It does not turn that component into either
missing whole-generation semantic identity above.

## Regression boundary

The audit test suite includes an explicit attempt to submit the structural
receipt as `targetQualificationReceipts`; the existing qualifier rejects it as
`pc4_online_target_qualification_invalid`. Corrupt target IDs, KAT adjacency
drift, fixture revision drift, and artifact digest drift also fail closed.

The next authority-bearing work is therefore not another bounded byte sample.
It must produce a deterministic whole-domain outgoing-edge completeness proof
for this exact graph/index generation and a complete PC Search offline parity
proof for the same profile, target, queue/hold semantics, and canonical result
identity. Only those outputs may populate the two currently null identities and
allow the separate exact target receipt builder to run.
