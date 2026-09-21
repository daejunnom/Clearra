# PC4 qualification tool

`clearra-pc4-qualifier` is a local-only evidence producer. It is not packaged
with CLI, GUI, Discord, Pages, Cloud Run, or Oracle runtime artifacts, and it
cannot activate a PC4 profile.

## Indexed-domain outgoing parity

`outgoing-shard` reads one immutable generation installed by the explicit PC4
downloader. For every source ID and every tetromino in the requested half-open
range it independently enumerates exact forward locks under the named kick
profile. It compares the normalized target IDs that exist in the field index
with the complete encoded graph record.

Each receipt binds:

- repository, immutable revision, profile, reader contract, and artifact
  descriptors from `active.json`;
- the observed SHA-256 of the complete field and offset indexes;
- the exact graph byte interval and its observed SHA-256;
- a half-open source-ID interval no larger than 262,144 records;
- edge, duplicate, missing, extra, and outside-index successor counts.

An existing valid receipt makes the same shard a no-op. This is the resume
boundary; an interrupted shard never leaves a completed receipt.

```powershell
./scripts/tools/invoke-clearra-build.ps1 -Purpose experiment -Command cargo `
  -ArgumentsJson '["run","--locked","--offline","-p","clearra-pc4-qualifier","--","outgoing-shard","--dataset-root","C:\\absolute\\pc4-data","--profile","jstris-180","--start","0","--end","65536","--output","C:\\absolute\\receipts\\shard-00000000-00065536.json"]'
```

`merge-outgoing` accepts only self-hashed shard receipts for the current
generation. Their source and graph-byte intervals must form one exact,
non-overlapping cover from source ID zero through the terminal ID. The merger
re-hashes every artifact and every graph segment before writing its receipt.

```powershell
./scripts/tools/invoke-clearra-build.ps1 -Purpose experiment -Command cargo `
  -ArgumentsJson '["run","--locked","--offline","-p","clearra-pc4-qualifier","--","merge-outgoing","--dataset-root","C:\\absolute\\pc4-data","--profile","jstris-180","--receipts","C:\\absolute\\receipts","--output","C:\\absolute\\merged\\indexed-domain.json"]'
```

For the authority-bearing continuation, `outgoing-proof-shard` additionally
writes every unique exact successor absent from the immutable index to a
generation- and source-range-bound `PC4BND02` file. The scan dynamically shares
small source batches across `--workers`; metrics, mismatch ordering, and the
sorted boundary are deterministic. `merge-outgoing-proof` accepts only an
exact source/graph-byte cover of v2 receipts, independently re-hashes all graph
segments, binds the complete indexed-path receipt, and k-way merges every
boundary file. An incomplete, overlapping, mixed-generation, renamed, or
tampered set fails closed.

```powershell
clearra-pc4-qualifier outgoing-proof-shard `
  --dataset-root C:\absolute\pc4-data --profile jstris-180 `
  --start 0 --end 262144 --workers 12 `
  --output C:\absolute\shards\shard-00000000-00262144.json `
  --boundary-output C:\absolute\shards\shard-00000000-00262144.bin

clearra-pc4-qualifier outgoing-proof-run `
  --dataset-root C:\absolute\pc4-data --profile jstris-180 `
  --output-directory C:\absolute\shards --start 0 `
  --shard-size 262144 --workers 12 --max-new-shards 8

clearra-pc4-qualifier merge-outgoing-proof `
  --dataset-root C:\absolute\pc4-data --profile jstris-180 `
  --receipts C:\absolute\shards --boundaries C:\absolute\shards `
  --indexed-path-receipt C:\absolute\indexed-path.json `
  --boundary-output C:\absolute\merged\outside-boundary.bin `
  --output C:\absolute\merged\outgoing-proof.json
```

`outgoing-proof-run` keeps one compiled process alive, validates and reuses
already completed deterministic shard names, and stops after the requested
number of newly created shards. The proof merger never accumulates every shard
boundary in memory: it validates each bound input while performing a streaming
k-way merge with one current field per shard, then atomically publishes the
deduplicated result. This matters because a single maximum-size Jstris 180
shard can contain more than ten million unique outside-index fields.

The merged proof remains non-authoritative until every field in the merged
outside boundary is proven terminal-dead and the independent offline PC Search
result-family parity identity is present.

`boundary-dead-proof` classifies one bound outside-boundary shard without
generating the complete reverse state space below its anchor. It validates the
exact reverse-domain provenance chain from layer 10 through `--anchor-layer`,
binds the complete indexed-path receipt, and exhaustively enumerates every exact
ILC target only below that anchor. The implementation advances one sorted,
deduplicated layer frontier at a time and releases the previous layer rather
than retaining a recursive per-root memo. Reaching an indexed field is sound because
the bound indexed-path receipt proves terminal co-reachability; reaching the
anchor is decided by exact membership in the reverse-domain layer. Any live
boundary field fails the command, while an all-dead shard gets a separate
non-authoritative receipt. Runtime duration and worker count are diagnostics,
not receipt inputs, so the v3 receipt identity remains deterministic across
equivalent executions.

Before an outside field enters the expensive exact frontier, two independent
necessary conditions may prove it dead. A completely occupied column is a
permanent wall through every row clear, so each separated vacancy strip must
contain a multiple of four cells. Fields that survive that check are tested
against an optimistic inverse-clear projection that lifts tetromino rows while
ignoring supply, timing, kicks, and reachability. Exhausting every projected
exact cover proves impossibility; finding a cover proves nothing and retains
the field. Reaching the fixed projection work limit is recorded as unknown and
also retains the field, so a budget can never manufacture a negative proof.

```powershell
clearra-pc4-qualifier boundary-dead-proof `
  --dataset-root C:\absolute\pc4-data --profile jstris-180 `
  --boundary C:\absolute\shards\shard-00000000-00000101.bin `
  --reverse-layers C:\absolute\reverse-layers --anchor-layer 7 `
  --indexed-path-receipt C:\absolute\indexed-path.json --workers 8 `
  --workspace C:\absolute\qualification-work\jstris-180-shard-00000000-00000101 `
  --output C:\absolute\dead\shard-00000000-00000101.json
```

Workers own deterministic contiguous source ranges within each layer and
publish fixed-size sorted runs into a bounded-fan-in external merge tree. The
next layer is streamed from that file rather than retained as one resident
vector. `--workspace` is optional; when supplied it must be a dedicated
absolute directory whose parent already exists. After each completed layer, a
self-hashed checkpoint records every future frontier's count and SHA-256 plus
all cumulative proof metrics. A matching workspace resumes from its newest
valid checkpoint after deleting only unreferenced partial runs; a mismatched or
tampered marker, checkpoint, or run fails closed. A completed proof removes the
workspace. This bounds both unsorted candidates and the unique frontier while
preserving every exact successor. The proof is exact for that boundary file but
does not by itself establish the whole source cover or the product-family parity
identity.

## Offline family and end-to-end tablebase parity

`offline-family-proof` does not read graph adjacency. It runs the ordinary exact
CPU solver for empty-board 4L Jstris 180 `P7P4`, requires the independently
supplied known count, checks that the returned canonical identities form one
strict sorted set, and recomputes the normalized family hash from those
identities. The receipt is generation-bound evidence, not activation authority.

```powershell
clearra-pc4-qualifier offline-family-proof `
  --dataset-root C:\absolute\pc4-data --profile jstris-180 `
  --workers 12 --expected-count 456459 `
  --output C:\absolute\qualification\jstris-180-offline-p7p4.json
```

`offline-family-materialize` performs the expensive ordinary solver run once
more, requires exact agreement with that receipt, and atomically checkpoints
every canonical identity in fixed-width `PC4FAM01` order. A separate self-hashed
receipt binds the artifact SHA-256, byte length, dataset generation, input
identity, exact count, and normalized set hash. Every later load re-hashes and
fully decodes the artifact, checks strict ordering, and recomputes the family
hash. The cache is therefore reusable computation, not additional authority and
not an inference from the receipt's 64-bit display hash.

```powershell
clearra-pc4-qualifier offline-family-materialize `
  --dataset-root C:\absolute\pc4-data --profile jstris-180 `
  --workers 7 --expected-count 456459 `
  --offline-proof C:\absolute\qualification\jstris-180-offline-p7p4.json `
  --family-output C:\absolute\qualification\jstris-180-offline-p7p4.bin `
  --output C:\absolute\qualification\jstris-180-offline-p7p4-materialization.json
```

If the canonical offline proof JSON is lost while its validated materialized
family and self-hashed materialization receipt remain, `offline-family-recover`
fully re-hashes and decodes that family, recomputes strict ordering and the
normalized family hash, reconstructs the canonical proof bytes, and requires
their receipt identity to equal the identity already bound by the
materialization. This is recovery of prior evidence, not a new solver proof or
an authority shortcut.

```powershell
clearra-pc4-qualifier offline-family-recover `
  --dataset-root C:\absolute\pc4-data --profile jstris-180 `
  --expected-count 456459 `
  --family C:\absolute\qualification\jstris-180-offline-p7p4.bin `
  --materialization C:\absolute\qualification\jstris-180-offline-p7p4-materialization.json `
  --output C:\absolute\qualification\jstris-180-offline-p7p4.json
```

`tablebase-family-proof` accepts only a complete outgoing merge, its exact
matching outside-boundary dead proof, the offline receipt, and the validated
materialization above. It drives the ordinary App tablebase path from verified
local Range slices and compares every canonical solution identity in order.
Count or the 64-bit display hash alone cannot pass this comparison. The
temporary activated snapshot exists only inside this local qualifier process;
it is never written as a production manifest. Local qualification reads use a
bounded page cache for all three immutable artifacts so repeated graph records
do not become millions of seek/read system calls; every admitted slice still
passes through the ordinary Range identity and byte validation boundary.

```powershell
clearra-pc4-qualifier tablebase-family-proof `
  --dataset-root C:\absolute\pc4-data --profile jstris-180 `
  --workers 12 --expected-count 456459 `
  --outgoing-proof C:\absolute\qualification\outgoing-proof.json `
  --boundary-dead-proof C:\absolute\qualification\outside-boundary-dead.json `
  --offline-proof C:\absolute\qualification\jstris-180-offline-p7p4.json `
  --offline-family C:\absolute\qualification\jstris-180-offline-p7p4.bin `
  --offline-materialization C:\absolute\qualification\jstris-180-offline-p7p4-materialization.json `
  --output C:\absolute\qualification\jstris-180-tablebase-p7p4-parity.json
```

Finally, `target-qualification` combines those linked proofs into the exact
`clearra.pc4.exact-target-qualification.v1` receipt consumed by generation
discovery. It qualifies only Jstris 180 PC Search at 4L. It does not infer Setup
Search authority or qualify another profile.

```powershell
clearra-pc4-qualifier target-qualification `
  --dataset-root C:\absolute\pc4-data --profile jstris-180 `
  --outgoing-proof C:\absolute\qualification\outgoing-proof.json `
  --boundary-dead-proof C:\absolute\qualification\outside-boundary-dead.json `
  --family-parity-proof C:\absolute\qualification\jstris-180-tablebase-p7p4-parity.json `
  --output C:\absolute\qualification\jstris-180-pc-search-4l.json
```

## Exact PC-completable domain

`domain-run` builds resumable reverse and forward layers without trusting the
upstream field index as the definition of the domain. Reverse generation starts
at the full four-row terminal. For each layer it first unions geometric
predecessor candidates across the complete next layer, then performs one exact
forward ILC reachability search per unique `(source, piece)` pair. Forward
generation starts at the empty board and retains only targets present in the
corresponding exact reverse layer. Dynamic work claiming prevents a few costly
boards from leaving a long static-partition tail.

Every `PC4DOM02` file binds the immutable dataset generation, its derivation
kind, the complete input-layer SHA-256, and (for a forward step) the reverse
filter-layer SHA-256. Files are written atomically. A resumed run accepts an
existing layer only when the entire derivation chain still matches; a valid
but unrelated or partially regenerated layer fails closed. `--max-new-steps`
bounds work per invocation while already-complete steps do not consume that
limit.

```powershell
# Run through the supported WSL ext4 source copy and the single managed build
# root. The example advances at most two previously missing reverse layers.
clearra-pc4-qualifier domain-run `
  --dataset-root C:\absolute\pc4-data `
  --profile jstris-180 `
  --direction reverse `
  --layers C:\absolute\qualification\jstris-180 `
  --workers 12 `
  --max-new-steps 2

# After all reverse layers exist, build the reachable intersection.
clearra-pc4-qualifier domain-run `
  --dataset-root C:\absolute\pc4-data `
  --profile jstris-180 `
  --direction forward `
  --layers C:\absolute\qualification\jstris-180 `
  --workers 12 `
  --max-new-steps 10

clearra-pc4-qualifier domain-compare `
  --dataset-root C:\absolute\pc4-data `
  --profile jstris-180 `
  --layers C:\absolute\qualification\jstris-180 `
  --output C:\absolute\qualification\jstris-180-domain-parity.json
```

The commands above illustrate the qualifier CLI. Repository builds must still
use the managed build launcher documented for the current host; copying a
binary out of its build transaction is not a supported shortcut.

`indexed-path-proof` independently re-hashes and scans all three canonical
artifacts. It verifies that every graph edge advances exactly one area layer,
then proves every indexed field is reachable from the empty root and can reach
the full four-row terminal. It emits only `indexed-path-domain-only` evidence:
outside-index exact successors still need the separate complete dead proof.

```powershell
clearra-pc4-qualifier indexed-path-proof `
  --dataset-root C:\absolute\pc4-data `
  --profile jstris-180 `
  --output C:\absolute\qualification\jstris-180-indexed-path.json
```

## Deliberate authority limit

This comparison proves adjacency parity only for exact forward targets already
present in the immutable field index. A legal target absent from that index is
counted as `outside_index_reachable_targets`; it is never assumed dead.

Consequently even a complete successful merge has
`qualification_status=not-qualified`, a null
`outgoing_edge_completeness_identity`, and a null
`offline_exact_parity_identity`. Product activation additionally requires an
independent exact generation of the PC-completable field domain (or equivalent
dead proof for every outside-index successor) and complete offline PC Search
result-family parity for the same profile and target.
