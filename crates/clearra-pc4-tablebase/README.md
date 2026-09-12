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

V-star recommendations, policy/value arrays, Krylov data, and n-PC probability
are outside this crate and are not fetched or decoded by it.
