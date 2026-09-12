use super::{
    error::{verify_probability_binding, PcProbabilityStaleBinding},
    request::PcProbabilityRequestBinding,
};

/// A request-bound, opaque result from a future probability implementation.
///
/// This crate does not define the numerical representation. The value is only
/// exposed by [`Self::into_probability_for`], which rejects a stale binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcNextProbabilityResult<Profile, Probability> {
    binding: PcProbabilityRequestBinding<Profile>,
    probability: Probability,
}

impl<Profile, Probability> PcNextProbabilityResult<Profile, Probability> {
    pub const fn new(
        binding: PcProbabilityRequestBinding<Profile>,
        probability: Probability,
    ) -> Self {
        Self {
            binding,
            probability,
        }
    }

    pub const fn binding(&self) -> &PcProbabilityRequestBinding<Profile> {
        &self.binding
    }
}

impl<Profile: PartialEq, Probability> PcNextProbabilityResult<Profile, Probability> {
    pub fn into_probability_for(
        self,
        expected: &PcProbabilityRequestBinding<Profile>,
    ) -> Result<Probability, PcProbabilityStaleBinding> {
        verify_probability_binding(expected, &self.binding)?;
        Ok(self.probability)
    }
}

/// A request-bound, opaque snapshot produced from caller-supplied material.
///
/// No raw accessor is provided. A consumer must validate owner, generation,
/// profile, and request before taking the snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcKrylovSnapshotResult<Profile, Snapshot> {
    binding: PcProbabilityRequestBinding<Profile>,
    snapshot: Snapshot,
}

impl<Profile, Snapshot> PcKrylovSnapshotResult<Profile, Snapshot> {
    pub const fn new(binding: PcProbabilityRequestBinding<Profile>, snapshot: Snapshot) -> Self {
        Self { binding, snapshot }
    }

    pub const fn binding(&self) -> &PcProbabilityRequestBinding<Profile> {
        &self.binding
    }
}

impl<Profile: PartialEq, Snapshot> PcKrylovSnapshotResult<Profile, Snapshot> {
    pub fn into_snapshot_for(
        self,
        expected: &PcProbabilityRequestBinding<Profile>,
    ) -> Result<Snapshot, PcProbabilityStaleBinding> {
        verify_probability_binding(expected, &self.binding)?;
        Ok(self.snapshot)
    }
}
