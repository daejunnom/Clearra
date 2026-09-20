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

## Resumed indexed-domain proof infrastructure

The local-only `clearra-pc4-qualifier` tool now provides deterministic,
bounded `outgoing-shard` receipts and an exact-cover `merge-outgoing` step.
Each shard binds the current immutable generation, re-hashes the complete field
and offset indexes, hashes its exact graph-byte interval, and compares every
encoded edge in its source-ID range with Clearra's independent exact forward
lock enumeration. The merger accepts only non-overlapping source and graph-byte
ranges that cover the whole generation exactly once, then re-hashes all three
artifacts. Existing valid shard receipts are reusable, so interruption loses at
most the current bounded shard.

This evidence is intentionally weaker than
`outgoing_edge_completeness_identity`. A forward-reachable normalized target
that is absent from `field_hash_to_id.v1.bin` is counted, not declared dead.
The first live measurements on the immutable Jstris 180 generation were:

- source ID `[0, 1)`: 162 indexed edges, zero outside-index targets, exact
  parity;
- source IDs `[1, 101)`: 10,076 indexed edges and 4,289 outside-index legal
  targets, exact indexed-domain parity.

The second result rules out treating a whole-index adjacency scan as the final
completeness proof. Individually invoking the exact-cover completion oracle for
millions of outside-index successors would repeat large subproblems. The next
proof owner must instead generate the exact terminal-co-reachable four-row
domain once, by reverse lock-clear semantics under the same kick profile, and
compare that set with the immutable field index. Only after domain equality,
whole-index adjacency parity, and offline result-family parity may the target
qualification identities be minted.

## Exact-domain checkpoint and proof refinement

The independent reverse generator is now executable as a local-only,
checkpointed qualification producer. `PC4DOM02` files are bound to the
immutable dataset generation and to the SHA-256 of the exact parent layer; a
forward layer additionally binds the reverse layer used as its filter. Existing
files with a different derivation chain fail closed. Unique geometric
`(source, piece)` candidates are combined before one exact forward ILC replay,
so different targets cannot force the same reachability search to run again.
Dynamic work claiming, a sorted k-way merge, and buffered atomic output remove
the observed static-tail, full-re-sort, and 8-byte mounted-filesystem write
bottlenecks respectively.

The first optimized Jstris 180 reverse checkpoints for revision
`ea61380b31fa3dc9ffb4c8505c9a09c1b421ef31` are:

| transition | input fields | geometric `(source,piece)` pairs | exact output fields |
|---|---:|---:|---:|
| layer 10 -> 9 | 1 | 162 | 100 |
| layer 9 -> 8 | 100 | 82,556 | 24,748 |
| layer 8 -> 7 | 24,748 | 8,458,302 | 2,015,406 |

The immutable upstream index contains only 19,405 layer-8 and 752,753 layer-7
fields, whereas the terminal-co-reachable reverse sets contain 24,748 and
2,015,406. This is expected evidence that reverse completeness alone includes
fields that are not reachable from the empty root. It also means the upstream
index cannot be accepted as the reverse domain by assumption.

For the final completeness proof, enumerating every reverse-only middle-layer
field is not required if it becomes materially larger than the path domain.
An equivalent and more targeted exact proof may combine: (1) complete indexed
adjacency parity, (2) root reachability and terminal co-reachability of every
indexed node, and (3) a memoized acyclic forward proof that every exact
outside-index successor is terminal-dead. Any real root-to-terminal path that
left the index would have a first outside successor, contradicting (3); and
(2) excludes extra indexed states. This boundary-dead proof must still cover
every shard and bind its closure to the same generation. It does not waive the
separate complete offline PC Search result-family parity identity.

The indexed-path component was then executed against the complete immutable
generation. It scanned all 15,185,706 fields and 109,562,993 graph edges,
verified every edge advanced exactly one area layer, and found zero indexed
fields unreachable from the empty root and zero indexed fields unable to reach
the terminal. The layer counts were
`[1, 162, 10,191, 273,459, 2,554,536, 6,805,146, 4,769,952,
752,753, 19,405, 100, 1]`. The non-authoritative receipt identity is
`sha256:4517ead40658d5f2a884fd00719468fa5610fd6bdbebe975c9f9682c548d4647`.
This closes indexed root/terminal path membership for this generation only;
the all-shard exact adjacency and outside-boundary dead closure remain open.

The resumable outgoing shard was subsequently extended with a v2 boundary
artifact. On source IDs `[0,101)`, the exact scan reproduced 10,238 indexed
edges, observed 4,289 outside-index transition occurrences, and reduced them to
4,257 unique generation-bound boundary fields. Its receipt identity is
`sha256:05b2615c4edde0b801db5b93e33c9fc48b24ba95931f286fbf764f7d2fa760cc`.
The v2 merger requires an exact source and graph-byte cover and binds the
complete indexed-path receipt before producing one sorted unique boundary.
This small shard validates the boundary contract only; the remaining source
ranges have not yet been scanned and no dead-proof identity has been minted.

The first two maximum-size continuation shards made the scale explicit. Source
IDs `[2,101,264,245)` produced 5,184,472 indexed edges, 12,404,817 outside
occurrences and 11,044,906 unique boundary fields in 10,817 ms of exact scan.
`[264,245,526,389)` produced 3,027,996 indexed edges, 9,717,490 outside
occurrences and 8,574,189 unique boundary fields in 10,005 ms. Their receipt
identities are respectively
`sha256:c312e95b3edffd497cfb563530840858fdde5be93a6eb42c77228d1fdc52f0d4`
and
`sha256:2020e0691680ae09458a05dd1ad7d85de0b5dc58274f7f9fc6d3952fa33176c2`.
The data disproves any assumption that all partial boundaries can safely remain
resident during merge. The merger now re-hashes and k-way merges the sorted
files as streams, with memory proportional to shard count rather than boundary
cardinality. Full source coverage and terminal-dead classification remain open.

The exact outgoing scan was then completed as 61 deterministic shards covering
`[0,15,185,706)` exactly once. The complete generation produced 109,562,993
encoded graph edges and the same 109,562,993 independently reconstructed
indexed edges, with zero piece-record mismatches and zero duplicate graph
edges. Exact ILC enumeration also observed 322,355,650 outside-index target
occurrences. The 61 receipts and their 61 bound `PC4BND02` files occupy about
1.931 GiB. This closes the all-shard indexed-adjacency axis, but it does not
classify the outside-boundary union and therefore does not mint the outgoing
completeness identity.

The first boundary-dead worst-case probe uses the `[0,101)` shard's 4,257
unique fields: 554 at layer 2, 3,277 at layer 3 and 426 at layer 4. A candidate
that retained unbounded raw successors exceeded 5.4 GiB RSS on the 16 GiB host
and was safely interrupted. Its replacement retains one layer at a time and
feeds fixed-size sorted worker runs through a deterministic merge tree. On the
same input it completed layer 2 in 25 ms and layer 3 in 2.29 s; layer 4 expanded
2,470,858 fields through 123,819,650 exact targets into 44,958,830 unique
successors in 85.642 s. Runtime duration and worker count remain diagnostics,
not inputs to a proof-receipt identity. Layer 5 then produced hundreds of
millions of raw successors and made clear that bounded storage alone did not
make this a viable whole-generation algorithm; that run was terminated with its
build lease released normally.

The replacement first applies two independent necessary conditions already
validated by the product-side omission audit. A fully occupied column remains
a wall under every clear, so a separated strip with a non-multiple-of-four
vacancy count is terminal-dead. The second oracle enumerates a deliberately
optimistic inverse-clear row projection and streams an MRV exact-cover search.
No projected cover is a negative proof, while a cover or its fixed-budget
`unknown` result stays in the exact ILC frontier. On the same 4,257-field input,
the retained layer-2 frontier fell from 554 to 51 and layer 4 from 2,470,858 to
139,073. The complete run expanded 2,829,797 frontiers and 40,633,340 exact
targets in 94.644 s, with 8,258,075 permanent-strip negatives, 5,500,620
projected-cover negatives, zero projection unknowns, and 4,842,562 exact
reverse-anchor checks. Its deterministic v3 receipt is
`sha256:ff096bf7204610e9acadf005713e8fa7128b8c2bba9ebd3b9a7bd3d700546afc`.

Each completed layer now writes a self-hashed checkpoint that binds cumulative
metrics and the count/SHA-256 of every future sorted frontier. A controlled
interruption after layer 4 resumed at layer 5, discarded only the unfinished
layer-6 worker runs, and reproduced the same receipt identity exactly. The 61
boundary files were also streaming-merged into 203,247,116 unique fields; the
non-authoritative merge receipt is
`sha256:529a6d02e17864a5c628262566758254ca41c60ac26e4a75a1092cb449b8bbee`.
The merged boundary's terminal-dead run and the independent offline complete
result-family parity remain required before an outgoing completeness identity
can be minted.

The generation-wide boundary-dead run subsequently completed. It consumed the
exact 203,247,116-field merged boundary, expanded 27,161,047 surviving
frontiers, generated 381,862,902 exact ILC targets, and performed 48,969,609
reverse-anchor checks. The final frontier was empty; no outside-index successor
could re-enter the exact terminal-co-reachable domain. Its deterministic receipt
is
`sha256:de7b85a603797a063cc327f96144c1ffde833e49a368aa1c30c3d2dab9febd02`.
Together with the 61-shard exact cover and indexed-path receipt, this closes the
outgoing-completeness proof inputs for this immutable generation. It does not
by itself qualify a product target.

The graph-independent ordinary CPU solver was then run for empty-board 4L,
Jstris 180, `P7P4`, hold enabled and empty, unique objective. It returned one
strictly sorted and deduplicated family of **456,459** identities with normalized
set hash `cts1:2fcbc950acdffa31`; count/probability completeness were true and
resource truncation was false. The generation-bound receipt is
`sha256:89f55a561287b00dbaf7c82ffb0693331f65fefa580092e7222a36de4f6b0f5b`.
This confirms the user-provided Jstris 180 count and also demonstrates why the
older generic/SRS count cannot be reused for this kick profile.

The expensive family was rerun once and atomically materialized as 456,459
fixed-width canonical records (`40,168,432` bytes). Its artifact identity is
`sha256:d4a726a522d101845d992d4c1254a6a9a9886829c906a0ccc51b29462333448a`
and the generation/input/count/set-hash-bound receipt is
`sha256:c8d8873ffb03f1aec2454f924e1139b6f3bb70c3a93ce317f3cb2da08beebc46`.
Every reuse re-hashes and fully decodes the file, verifies strict canonical
ordering, and recomputes `cts1:2fcbc950acdffa31`; it avoids another 1,022,353 ms
ordinary-solver run without becoming separate qualification authority.

The first end-to-end local tablebase replay failed closed before returning a
family because the breadth-layer key embedded the general sixteen-placement
`StandardBoard64TilingIdentity` in every four-line partial state and crossed the
product's 256 MiB retained-frontier authority. The correction does not raise
that authority or merge colored fields. A PC4-private identity now ranks each
four-cell mask bijectively in the `40 choose 4` domain and stores at most ten
piece-plus-rank values; terminal states are reconstructed into the unchanged
canonical product identity before candidate admission. Full tablebase/offline
identity-sequence parity remains required before a target receipt is minted.

The first packed rerun exposed a separate whole-layer ownership cliff. It
attempted to reserve the entire promoted layer before releasing the source hash
table and jumped from a measured 242,880,108-byte peak to a 432,538,688-byte
request. Promotion now streams at most the bounded resident-work window while
the old table remains the owner. The next exact rerun progressed farther, then
failed closed at 284,806,808 bytes: the 1,835,008-bucket source layer and the
growing 917,504-bucket successor layer legitimately overlapped, while nested
supply frontiers retained 86,594,200 bytes. This diagnostic showed that the
remaining excess was key representation, not a reason to raise the 256 MiB
authority. `Pc4RowFrame` is an order-preserving subset of four original rows,
so its five-byte array/length representation was replaced by the exact four-bit
survivor mask. Compile-time size assertions now require the 64-bit partial key
to remain at most 32 bytes. The final full-family rerun remains the authority;
none of these intermediate failures issues a parity or target receipt.

That compact-key rerun crossed the former cliff, but a later nested-frontier
admission failed closed by exactly 136 bytes (`268,435,592 > 268,435,456`). The
diagnostic had only 63 ready tasks but a 128-slot deque: one cache admission had
woken every waiter before the 64-task resident window could consume them. Cache
wake-up now moves only the available resident-window slots and leaves the rest
inside their already-accounted waiting owner. This preserves all work and the
64-way execution window while preventing an idle doubled backing allocation.
The subsequent full-family rerun, rather than the 136-byte estimate, remains the
acceptance evidence.
