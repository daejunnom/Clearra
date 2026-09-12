# clearra-pc4-tablebase

This crate is the pure, transport-independent reader protocol for the future
online PC4 solution tablebase. It owns snapshot qualification, bounded Range
requests, strict response validation, helper-index lookup, and graph-record
byte decoding contracts. It performs no network, filesystem, UI, solver, or
product activation work.

The supported helper-index contract treats GOFFIDX1 values as raw byte offsets
into a graph artifact. A profile's u24/u32 target encoding is intentionally
kept separate and may only be used after its qualified graph-record layout has
delimited a target sequence.

The existing pc4-compact-exact-v12 asset remains a separate, embedded
exact-dead-state pruning accelerator until the v0.9 migration is qualified.
It is deliberately not imported by this crate, and the online reader is not
registered with the current runtime. This prevents either implementation from
silently becoming an alternative source of product truth.

Production activation is fail-closed: one immutable snapshot generation must
contain independently qualified index, graph-format, provenance, and
known-answer evidence for SRS, SRS+, SRS-X, Jstris-180, and no-kick. The
repository does not embed a Hugging Face revision or dataset digest. Discovery
may follow a moving upstream reference, but it must resolve that reference to
an immutable identity before constructing a manifest.

An otherwise complete manifest still cannot construct an `ActivatedSnapshot`
without passing the host-owned `DatasetSnapshotVerifier` port. The request
binds the exact immutable snapshot, manifest content identity, and all profile
qualifications; the returned attestation is rejected if either identity drifts.
This crate deliberately does not guess a signature encoding or cryptographic
algorithm. A future host adapter must validate the upstream signed generation
under the separately qualified format, while current tests use explicitly
named synthetic verifiers and confer no production authority.

V-star recommendations, policy/value arrays, Krylov data, and n-PC probability
are outside this crate and are not fetched or decoded by it.

The graph state domain is bounded to fields no higher than four rows. That
does not make this an exactly-four-lines-only recommendation table, nor does it
automatically authorize every shorter target. A PC or Setup accelerator for a
target in `1..=4` must separately qualify that profile, PC-or-Setup use case,
and target's terminal predicate and full outgoing-edge completeness. Targets
above four rows remain outside this tablebase. Until those target-specific
proofs and their concrete candidate bridge exist, the reader stays feature-off
and the exact offline solver remains authoritative.

The placement-materializer boundary accepts one snapshot- and profile-bound
source-field + piece -> target-field edge from a separately qualified graph
record parser. A profile-specific Clearra adapter must return every concrete
legal placement as the declared `(piece, rotation, x, y, occupied-cells)`
identity. The boundary rejects empty or incorrectly bound results, checks
cancellation and snapshot freshness before and after enumeration, and
canonicalizes only by that placement identity. It does not parse opaque graph
records, qualify profiles, prove reachability itself, or make a placement
identity into replay evidence.

Neither lookup nor materialization may use graph order as a score, tie-break,
or preferred move. Selection objectives run only after a complete concrete
candidate universe has crossed the application reducer boundary. Setup may
reuse the same complete graph traversal only with separately qualified Setup
terminal semantics; this crate does not infer a Setup reverse index.

The fixed-queue traversal foundation consumes an immutable snapshot/profile,
one start field, an exact queue, a caller-owned terminal predicate and guard,
and finite visited/frontier/path/output budgets. It asks a separately qualified
provider for complete adjacency and validates every redundant binding. Hydra
degree can contain the same target once per concrete placement, so equal raw
`source + piece + target` occurrences become one canonical field transition;
the exact materializer then recovers all concrete placements once. Traversal
still preserves paths that converge from different prior paths without global
state deduplication, and emits paths in canonical order. Empty adjacency is a
valid dead end. Cancellation, snapshot drift, callback failures, binding
violations, and budget exhaustion discard the in-progress result through typed
errors. This pure layer does not implement hold, pattern/bag semantics, graph
parsing/fetching, runtime registration, or dataset activation.

Its feature-off resumable family supplements the compatible eager API with
bounded pages. Preparation requires a target-specific completeness identity
already minted by manifest activation; this seam cannot itself qualify a
1L--4L target or infer that PC4 graph coverage applies.
Preparation owns only the immutable request; the first graph callback happens
when a page is requested. A depth-first canonical cursor retains only pending
prefix paths, applies both lifetime and per-page work limits, and commits state
only after a successful page. The cursor is bound to the exact prepared
target/snapshot/profile/request family, so cancellation, snapshot drift, callback
failure, or foreign-cursor use cannot publish a partial page. Repeated target
IDs within one complete adjacency are one identical graph transition and are
suppressed before traversal: the separately qualified ILC materializer remains
responsible for enumerating every concrete placement realizing that transition.
Distinct prefixes that later converge remain distinct paths. This rule must not
be used for any upstream edge record carrying semantics beyond
`source + piece + target`; such a format requires new qualification and fails
closed at the provider boundary instead.

The concrete-path bridge materializes each qualified graph edge exactly once,
stores only that edge's bounded concrete placement alternatives, and pages the
Cartesian product with a snapshot-bound mixed-radix cursor. It never allocates
the full product, treats a zero-edge terminal graph path as one empty concrete
realization, and discards page output on cancellation, snapshot drift, or a
cursor/family mismatch. This is still a feature-off synthetic boundary: it does
not make an unqualified graph parser or placement adapter authoritative.

The fixed-queue hold expander is a separate bounded supply state machine. It
turns a concrete current/preview queue plus disabled, empty, or occupied hold
state into every semantic placement-piece sequence while retaining canonical
hold-decision evidence for later replay. Equal current/held pieces are one
semantic branch. Empty hold consumes both current and next. It never invents a
piece after the supplied queue is exhausted and does not implement pattern or
bag revelation. Its output is only input for the still feature-off qualified
fixed-queue graph traversal; it is not tablebase activation or a solution.

The bag-draw primitive is another deliberately smaller boundary. It owns an
immutable arbitrary multiplicity profile, exact remainder and bag epoch, and
emits at most seven canonical distinct-piece transitions with exact
`multiplicity / denominator` weights. An empty remainder refills from the
profile and advances the epoch with checked arithmetic. This primitive does
not decide preview visibility, parse a pattern, apply hold, query a graph, or
register a product capability. The future `graph x hold x bag x preview` DP
may consume it only after those observation semantics are separately fixed.

The bounded reveal family composes that one-draw primitive for an explicitly
requested hidden-draw count. It memoizes exact suffix counts, then un-ranks a
bounded page in canonical piece order instead of allocating every concrete
sequence. Each emitted sequence retains its terminal remainder/epoch and a
reduced checked-u128 rational probability. Count, memo, rank, page-allocation,
absolute-depth, and cancellation failures are typed and transactional; cursors
are bound to one prepared family. This remains only an input family for the
future `graph x hold x bag x preview` DP. It does not infer preview observation
policy or implement that DP.

The graph-free observation frontier is the next feature-off composition seam.
Its input separates an exact current-plus-preview prefix from the exact bag
state after that prefix was drawn, then lazily combines canonical hidden-reveal
ranks with the fixed-queue hold branches. Each bounded page retains exact reveal
probability, hold-decision evidence, terminal bag state, and the terminal
current/preview window; work-slice limits can return a resumable partial or
empty page, while errors leave the cursor unchanged. Concrete hidden suffixes
are retained only for later path reconstruction and may not influence an
observation policy. This layer still performs no graph lookup, pattern parsing,
network/filesystem I/O, product registration, or production qualification.

The Range-fragment cache is a pure, bounded optimization seam for future
native and browser transports. It stores only an exact qualified
snapshot/profile/artifact/content/range binding, never slices a containing or
overlapping entry, and rejects a conflicting body under the same identity.
Lookup session and request IDs are reconstructed from the current request so a
cached fragment can cross sessions without making a response valid for the
wrong session. Finite FIFO entry/byte limits, checked range arithmetic,
fallible byte allocation, and transactional cancellation keep caching outside
the dataset-authority and product-activation boundaries.
