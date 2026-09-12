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

The placement-materializer boundary accepts one snapshot- and profile-bound
source-field + piece -> target-field edge from a separately qualified graph
record parser. A profile-specific Clearra adapter must return every concrete
legal placement as the declared `(piece, rotation, x, y, occupied-cells)`
identity. The boundary rejects empty or incorrectly bound results, checks
cancellation and snapshot freshness before and after enumeration, and
canonicalizes only by that placement identity. It does not parse opaque graph
records, qualify profiles, prove reachability itself, or make a placement
identity into replay evidence.

The fixed-queue traversal foundation consumes an immutable snapshot/profile,
one start field, an exact queue, a caller-owned terminal predicate and guard,
and finite visited/frontier/path/output budgets. It asks a separately qualified
provider for complete adjacency, validates every redundant binding, preserves
converging edge paths without global state deduplication, and emits paths in a
canonical order. Empty adjacency is a valid dead end. Cancellation, snapshot
drift, callback failures, binding violations, and budget exhaustion discard the
in-progress result through typed errors. This pure layer does not implement
hold, pattern/bag semantics, graph parsing/fetching, runtime registration, or
dataset activation.

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
