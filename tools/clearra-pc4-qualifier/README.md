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
