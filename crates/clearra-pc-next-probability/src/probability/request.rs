use core::num::NonZeroU64;

/// Identifies the owner that is allowed to consume one probability response.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PcProbabilityOwnerId(NonZeroU64);

impl PcProbabilityOwnerId {
    /// Returns `None` for the reserved, uninitialized identity zero.
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Identifies one owner-controlled generation of probability work.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PcProbabilityGeneration(NonZeroU64);

impl PcProbabilityGeneration {
    /// Returns `None` for the reserved, uninitialized generation zero.
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Identifies one request within an owner and generation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PcProbabilityRequestId(NonZeroU64);

impl PcProbabilityRequestId {
    /// Returns `None` for the reserved, uninitialized request identity zero.
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// The complete identity that every response from a future provider must echo.
///
/// `Profile` remains owned by the future product contract. This crate neither
/// imports a current rule-profile type nor assumes a set of supported profiles.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PcProbabilityRequestBinding<Profile> {
    owner: PcProbabilityOwnerId,
    generation: PcProbabilityGeneration,
    profile: Profile,
    request: PcProbabilityRequestId,
}

impl<Profile> PcProbabilityRequestBinding<Profile> {
    pub const fn new(
        owner: PcProbabilityOwnerId,
        generation: PcProbabilityGeneration,
        profile: Profile,
        request: PcProbabilityRequestId,
    ) -> Self {
        Self {
            owner,
            generation,
            profile,
            request,
        }
    }

    pub const fn owner(&self) -> PcProbabilityOwnerId {
        self.owner
    }

    pub const fn generation(&self) -> PcProbabilityGeneration {
        self.generation
    }

    pub const fn profile(&self) -> &Profile {
        &self.profile
    }

    pub const fn request(&self) -> PcProbabilityRequestId {
        self.request
    }
}

/// A future probability request with an opaque boundary representation.
///
/// The request carries no command, UI, transport, dataset, or numerical-model
/// fields. The future product owner must define the boundary semantics before
/// activation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PcNextProbabilityRequest<Profile, Boundary> {
    binding: PcProbabilityRequestBinding<Profile>,
    boundary: Boundary,
}

impl<Profile, Boundary> PcNextProbabilityRequest<Profile, Boundary> {
    pub const fn new(binding: PcProbabilityRequestBinding<Profile>, boundary: Boundary) -> Self {
        Self { binding, boundary }
    }

    pub const fn binding(&self) -> &PcProbabilityRequestBinding<Profile> {
        &self.binding
    }

    pub const fn boundary(&self) -> &Boundary {
        &self.boundary
    }

    pub fn into_boundary(self) -> Boundary {
        self.boundary
    }
}
