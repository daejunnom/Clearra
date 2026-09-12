# clearra-pc-next-probability

This unpublished `no_std` crate owns the dormant contract seam for a possible
future n-PC probability product. It has no filesystem, network, runtime,
serialization, CLI, GUI, Discord, capability-registry, or tablebase dependency.
No current product crate depends on it.

The seam deliberately keeps profile, boundary, probability, Krylov source, and
Krylov snapshot representations opaque. `PcKrylovSnapshotAdapter` can only
adapt a caller-supplied source; it does not discover, locate, fetch, or decode an
upstream artifact. In particular, this crate defines no Krylov byte layout and
performs no projection or probability arithmetic.

Every request, successful result, cancellation, unsupported outcome, and
source failure carries an owner/generation/profile/request binding. Consumers
must validate that binding before taking an opaque result or snapshot. A typed
stale-binding error distinguishes owner, generation, profile, and request drift.

There is no production implementation, registration factory, activation
manifest entry, build feature, environment switch, or fallback. Synthetic
in-memory unit tests are the only executable use of the contracts in this
release line.
