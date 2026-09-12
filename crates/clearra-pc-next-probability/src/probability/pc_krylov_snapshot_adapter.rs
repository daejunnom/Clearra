use super::{
    error::PcNextProbabilityError, request::PcProbabilityRequestBinding,
    result::PcKrylovSnapshotResult,
};

/// Request-bound response produced by a [`PcKrylovSnapshotAdapter`].
pub type PcKrylovSnapshotAdapterResponse<Profile, Snapshot, AdapterError> = Result<
    PcKrylovSnapshotResult<Profile, Snapshot>,
    PcNextProbabilityError<Profile, AdapterError>,
>;

/// Pure adapter contract for opaque, caller-supplied Krylov snapshot material.
///
/// `Source` and `Snapshot` deliberately have no byte, path, URL, repository,
/// revision, or numerical-layout contract here. Implementations must only adapt
/// material already supplied by their caller; discovery and I/O are outside
/// this boundary. This crate supplies no production implementation.
pub trait PcKrylovSnapshotAdapter {
    type Profile: Clone + Eq;
    type Source;
    type Snapshot;
    type Error;

    fn adapt_supplied(
        &self,
        source: &Self::Source,
        binding: &PcProbabilityRequestBinding<Self::Profile>,
    ) -> PcKrylovSnapshotAdapterResponse<Self::Profile, Self::Snapshot, Self::Error>;
}
