# Forward Search Native Parallel and Performance Record

Date: 2026-08-04

This record covers forward spin search, forward maximum-damage search, the native CLI boundary,
and the Discord forward-command input seam. PC search, build-probability search, setup search,
and their reverse-search pruning were deliberately left unchanged.

## Measurement Contract

- Build: WSL release profile, eight total workers, product batch request 256. The coordinator's
  fixed/layered batch safety clamp remains 32.
- The standalone `forward_layer_benchmark` example fixes the measurement method in source. It
  separates query preparation, persistent worker initialization, search, and final materialization.
  Every search depth also records wall time, producer time, summed worker CPU time, coordinator
  absorption time, wait time, batch/work counts, wire bytes, visited states, generated locks, and
  peak frontier. Production code is not instrumented.
- Searches at or below approximately one second use five runs. Longer searches use two runs. The
  near-boundary damage fixture was conservatively run five times after optimization. Peak RSS is
  sampled by `/usr/bin/time -f %M` outside the search process.
- Outcome identity is an order-independent FNV-1a digest over every semantic report and path field.
  Final parallel reports were also compared with one-worker serial reports.
- Fixture: board `0x280f8ffff8f`, height 8, queue `IOTSZ`, hold enabled, SRS+. The damage fixture
  uses the fixed `OLJTT` queue without hold and expects maximum damage 8.

## Same-Contract Results

| Search | Baseline | Final | Change | Baseline peak RSS | Final peak RSS |
|---|---:|---:|---:|---:|---:|
| T-Spins+, five-run median | 250.175 ms | 246.602 ms | 1.43% faster | 22,968 KiB | 23,180 KiB median |
| Maximum damage `OLJTT`, mean | 3,402.611 ms (2) | 1,057.444 ms (5) | 68.92% faster | 30,246 KiB mean | 33,988 KiB median |
| All-Mini+, mean | 6,817.190 ms (2) | 2,251.333 ms (2) | 66.98% faster | 74,098 KiB mean | 72,990 KiB mean |

The final damage median is 1,022.030 ms. Damage RSS rises by 12.37%, which is retained because the
search-time reduction is material. T-Spins+ RSS is effectively flat (+0.92%). All-Mini+ is both
faster and 1.50% smaller by peak RSS.

The expensive terminal depth is now distributed instead of being executed by the coordinator:

| Search | Former terminal coordinator work | Final terminal-depth wall time |
|---|---:|---:|
| Maximum damage `OLJTT` | 3.20--3.33 s | 0.918 s median |
| All-Mini+ | 6.34--6.39 s | 1.786 s mean |

Exact final identities:

- T-Spins+: 62,660 states, 3,279,899 locks, 58,505 peak frontier, 12,242 outcomes,
  digest `a9e371b46f69498e`.
- Maximum damage: 277,954 states, 12,910,269 locks, 268,399 peak frontier, one outcome,
  maximum damage 8, digest `1fe5712dc5a2d515`.
- All-Mini+: 693,089 states, 37,987,051 locks, 659,505 peak frontier, 12,242 outcomes,
  digest `7e5cb6f6d215c798`.

Serial validation returned the same counts and digests. Before deterministic merge, T-Spins+ kept
the same counts but produced a different representative-path digest across parallel runs. Every
final run now matches the serial digest.

## Sfinder Reference

Sfinder 1.43 was measured twice with:

```text
spin --field-path scripts/benchmark/fixtures/forward-spin-field.txt --patterns IOTSZ --line 1 --format csv --split false
```

Its internal search times were 209 ms and 255 ms; process walls were 667.224 ms and 721.280 ms.
It returned 340 solutions. This is a reference, not an equal-output race: Sfinder reports its own
spin solution contract while the Clearra fixture retains 12,242 exact forward outcomes and full
paths. The numbers must not be ranked without normalizing those semantics first.

## Accepted Changes

- Task results are merged in task-ID order. This restores serial representative-path determinism
  while workers may still finish in any order.
- The former coordinator-only terminal-layer shortcut was removed from the distributed driver.
  Serial terminal fusion remains in the serial search and the search/pruning predicates are
  unchanged.
- Forward action wire version 8 carries only fields consumed by the trace merge. Full board,
  placement mask, cleared-row mask, and cumulative damage are materialized once in the final path.
  The actual placement rotation is retained. On the damage terminal layer, output fell from about
  829 MB to 335,124,107 bytes.
- Reordering is bounded to four batches per verifier worker. Normally a ready worker is immediately
  reused; only a sustained earlier-task gap applies backpressure. Cancelled absorption exits before
  wire decoding or backlog draining.
- Native AppContext/CLI Damage and Spin now use the same coordinator/worker protocol as WASM. The
  caller owns the coordinator and exactly `workers - 1` persistent threads own worker state. One
  worker or a query below the existing worthwhile threshold stays serial. CLI JSON and text expose
  the actual `workers_used` value.
- Discord `/damage`, `$damage`, and `>damage` canonical CTK3 masks now reach the native forward
  parser through `--board-mask-v1`, preserving all 240 bits and inferring height without converting
  or recoloring the input.

## Rejected and Deferred Work

- Raising the fixed/layered batch cap from 32 to 128 produced no useful damage-search speed gain
  and raised peak RSS from roughly 33,000 KiB to 42,976 KiB. It was reverted immediately and the
  pre-candidate `parallel.rs` SHA-256
  `44d0884ce89fe6fb389e8aac61a01e1e5708e9ca73e105d4c83a91dac0d291f0` was verified exactly.
- The previously rejected 64-entry reachability cache was not repeated.
- T-suffix hoisting, generation-stamped frontier indexes, and earlier scoring dedupe remain possible
  larger experiments. They were deferred because this pass already removed the measured dominant
  terminal bottleneck and those changes would touch exact pruning or broader state ownership.

## Validation and Source Identity

- `clearra-forward-search`: 24/24 tests passed, including reversed result arrival, compact placement
  rotation, bounded reorder backpressure, cancellation, and serial/parallel equality.
- `clearra-cli-command`: 91/91 tests passed.
- Discord bot: 336/336 tests passed.
- Native AppContext focused tests: 6/6 passed; CLI renderer focused tests: 2/2 passed.
- Native `clearra-app`, `clearra-cli`, and `clearra-cli-command` all-target checks passed.
- `clearra-wasm --features stage-profiling --target wasm32-unknown-unknown` check passed.

Initial core SHA-256 values were
`parallel.rs=6f9db0bcdb526cea653a5141b50b85e7624ded662f8f8978becd727b5bfda105` and
`search.rs=e48a74d2b0dc6e29ce4cc25e9e8cb6eff0e5dd16e8e66678a7965623ce9fc8ef`.
Final values are
`parallel.rs=b1d44af42160b4c642ad7e577183017a72749f5eba8c8c092b258643e8cbdd65` and
`search.rs=484f0551b70934bdf7b44c3e2279422b76a85910873c8b0e0abb45d74a1f818c`.
