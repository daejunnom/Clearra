# Native symbolic-family research tools

This package builds three local commands:

| Command | Purpose |
| --- | --- |
| `symbolic_family` | Compile or run an explicit board/source/rule request; optionally save its shared result DAG. |
| `symbolic_family_compare` | Run the fixed Full4L P7P4 comparison request, with an archive-only symbolic mode. |
| `symbolic_ctk4` | Inspect, verify, or read a bounded identity page from a CTK4 archive. |

The solver remains an opt-in research API. This README describes commands, not
a completed Full4 measurement or a Full6 performance result. CI is not
authorized for the current task; none of these commands dispatches CI.

## Build

Run from the repository root. Select this package directly so the executor is
a normal dependency and its example/dev-dependency feature closure is not
pulled into the executable build:

```sh
cargo build --locked -p clearra-symbolic-bench --release --bins
```

Use the repository's existing Rust manager for a large supervised build:

```sh
build/cargo/default/debug/clearra-manage runtime run \
  --producer symbolic-tools-build --profile build-test --no-timeout -- \
  cargo build --locked -p clearra-symbolic-bench --release --bins
```

The package's defaults forward `symbolic-family`, `parallel`, and the executor's
default feature identity. These commands retain the workspace release profile;
they do not override LTO, optimization level, codegen units, or incremental
settings. Do not silently mix profile/environment changes into a performance
comparison. With the repository's default Cargo target directory, binaries
appear under `build/cargo/default/release/`. An intentional `CARGO_TARGET_DIR`
override changes that location.

## Full4L P7P4: save the shared result

Choose a new output prefix under `_local/artifacts`. Its parent directory must
exist. The fixed request uses an empty 10-by-4 board, Jstris 180, 10 locks,
enabled empty Hold, FullQueueOracle, CountUnique, and the actual fresh standard
7-bag source with 11 draws. That source has 4,233,600 original queues; it is not
constructed by enumerating those queues before search.

```sh
mkdir -p _local/artifacts/symbolic-ctk4

build/cargo/default/debug/clearra-manage runtime run \
  --producer full4-p7p4-symbolic --profile benchmark-search --no-timeout -- \
  build/cargo/default/release/symbolic_family_compare \
  --backend symbolic --archive-only \
  --output-prefix _local/artifacts/symbolic-ctk4/full4-p7p4
```

`--no-timeout` removes the manager's elapsed-time deadline while preserving
memory and process-tree supervision. The command installs no internal search
deadline or candidate-verification fallback. It uses the existing automatic
logical-CPU policy.

On successful search and archive publication, this mode writes:

- `full4-p7p4.summary.json`: a search-completion checkpoint followed by the
  archive outcome and separate search/archive timing.
- `full4-p7p4.ctk4`: the shared `accepted`, `identities`, and `covered_sources`
  roots, their nodes, and the placement/source dictionary.

The archive writer validates the actual serialized graph before publishing
its owned `.partial` file. Existing outputs are not overwritten. A failed
write can leave that partial file for inspection; it is not a completed
artifact.

`--archive-only` skips complete identity export and original-source bitmap
construction. Its summary therefore does not claim complete projection or
joint comparison. Omitting this option deliberately requests the older,
output-sized post-search comparison evidence. The archive is saved first.

For general inputs, use `symbolic_family --help`; its `--archive PATH` option
saves the same representation, and its default page size is zero. A full CTK3
export is never an automatic consequence of saving CTK4.

## Inspect and decode

Inspecting reads only the manifest. Counts at this stage are declarations:

```sh
build/cargo/default/release/symbolic_ctk4 inspect \
  _local/artifacts/symbolic-ctk4/full4-p7p4.ctk4
```

Verify an unknown file without enumerating its solutions:

```sh
build/cargo/default/debug/clearra-manage runtime run \
  --producer symbolic-ctk4-verify --profile verification --no-timeout -- \
  build/cargo/default/release/symbolic_ctk4 verify \
  _local/artifacts/symbolic-ctk4/full4-p7p4.ctk4
```

Read only the requested identity page and optionally save that view as CTK3:

```sh
build/cargo/default/debug/clearra-manage runtime run \
  --producer symbolic-ctk4-page --profile verification --no-timeout -- \
  build/cargo/default/release/symbolic_ctk4 page \
  _local/artifacts/symbolic-ctk4/full4-p7p4.ctk4 \
  --offset 0 --limit 16 --cache-mib 8 \
  --ctk3-output _local/artifacts/symbolic-ctk4/full4-p7p4-page-0.ctk3
```

Omit `--ctk3-output` for JSON identities only. Page limits are at most 4,096;
offsets are exact `u128` ranks in archived ZDD order, not CTK1 sort order. A CTK3
output path must be new. Its colored pages include canonical CTK1 comments so
adjacent pieces of the same color do not lose their partition description.

## Lazy loading and trust

Each cold `verify` or `page` invocation scans all nodes and atom blocks first.
A prior standalone `verify` process does not confer authority on a later
process: running both commands performs two scans. `page` already includes
verification, so the separate `verify` command is optional.

After validation, paging fetches needed blocks, checks their hashes, and skips
counted branches outside the requested range. It does not enumerate preceding
identities or retain unrelated sibling branches. The native decoder clears
its validation cache before paging and reports verification cost separately
from additional page I/O. The block cache defaults to 8 MiB; this is not a peak
RSS bound. `--cache-mib 0` disables retained blocks and may increase reads.

An embedding application can retain one verified reader, or use the encoder's
live `ProducerSeal` to reopen the identical artifact without a cold scan.
The seal is an in-process capability, not a persistent signature or a digest
that an untrusted file can use to authorize its own counts.

Graph validation establishes exact **atom-ID set** semantics. It does not
certify gameplay legality, dictionary injectivity, solver completeness, or an
original query attached as an annotation. The archive preserves the producer's
three result roots; it does not independently store the full uncovered source
universe or certify a probability denominator. The decoder reports this scope
explicitly. Native range I/O is implemented here; an asynchronous browser or
remote-storage adapter is separate integration work.

See [the CTK4 codec contract](../../crates/clearra-ctk4/README.md) for the wire
format, validation states, limits, and ownership details.
