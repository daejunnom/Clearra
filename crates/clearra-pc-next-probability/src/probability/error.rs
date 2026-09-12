use super::request::PcProbabilityRequestBinding;

/// Identifies which part of a response no longer belongs to the live request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PcProbabilityBindingField {
    Owner,
    Generation,
    Profile,
    Request,
}

/// Typed rejection of a delayed or incorrectly bound response.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PcProbabilityStaleBinding {
    field: PcProbabilityBindingField,
}

impl PcProbabilityStaleBinding {
    const fn new(field: PcProbabilityBindingField) -> Self {
        Self { field }
    }

    pub const fn field(self) -> PcProbabilityBindingField {
        self.field
    }
}

/// Compares all authority-bearing fields before an opaque value is consumed.
pub fn verify_probability_binding<Profile: PartialEq>(
    expected: &PcProbabilityRequestBinding<Profile>,
    received: &PcProbabilityRequestBinding<Profile>,
) -> Result<(), PcProbabilityStaleBinding> {
    if expected.owner() != received.owner() {
        return Err(PcProbabilityStaleBinding::new(
            PcProbabilityBindingField::Owner,
        ));
    }
    if expected.generation() != received.generation() {
        return Err(PcProbabilityStaleBinding::new(
            PcProbabilityBindingField::Generation,
        ));
    }
    if expected.profile() != received.profile() {
        return Err(PcProbabilityStaleBinding::new(
            PcProbabilityBindingField::Profile,
        ));
    }
    if expected.request() != received.request() {
        return Err(PcProbabilityStaleBinding::new(
            PcProbabilityBindingField::Request,
        ));
    }
    Ok(())
}

/// Fail-closed reasons for which the dormant seam cannot serve a request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PcProbabilityUnsupportedReason {
    FeatureOff,
    UnqualifiedSnapshot,
    UnsupportedProfile,
    UnsupportedBoundary,
}

/// Typed failure kind hidden behind a request-binding check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PcNextProbabilityErrorKind<SourceError> {
    Unsupported(PcProbabilityUnsupportedReason),
    Cancelled,
    Source(SourceError),
}

/// A request-bound failure from a future probability port or snapshot adapter.
///
/// Cancellation, unsupported states, and source failures remain opaque until
/// [`Self::into_kind_for`] verifies all authority-bearing request fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcNextProbabilityError<Profile, SourceError> {
    binding: PcProbabilityRequestBinding<Profile>,
    kind: PcNextProbabilityErrorKind<SourceError>,
}

impl<Profile, SourceError> PcNextProbabilityError<Profile, SourceError> {
    pub const fn unsupported(
        binding: PcProbabilityRequestBinding<Profile>,
        reason: PcProbabilityUnsupportedReason,
    ) -> Self {
        Self {
            binding,
            kind: PcNextProbabilityErrorKind::Unsupported(reason),
        }
    }

    pub const fn cancelled(binding: PcProbabilityRequestBinding<Profile>) -> Self {
        Self {
            binding,
            kind: PcNextProbabilityErrorKind::Cancelled,
        }
    }

    pub const fn source(binding: PcProbabilityRequestBinding<Profile>, error: SourceError) -> Self {
        Self {
            binding,
            kind: PcNextProbabilityErrorKind::Source(error),
        }
    }

    pub const fn binding(&self) -> &PcProbabilityRequestBinding<Profile> {
        &self.binding
    }
}

impl<Profile: PartialEq, SourceError> PcNextProbabilityError<Profile, SourceError> {
    pub fn verify_for(
        &self,
        expected: &PcProbabilityRequestBinding<Profile>,
    ) -> Result<(), PcProbabilityStaleBinding> {
        verify_probability_binding(expected, self.binding())
    }

    pub fn into_kind_for(
        self,
        expected: &PcProbabilityRequestBinding<Profile>,
    ) -> Result<PcNextProbabilityErrorKind<SourceError>, PcProbabilityStaleBinding> {
        verify_probability_binding(expected, &self.binding)?;
        Ok(self.kind)
    }
}
