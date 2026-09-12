use super::{
    error::PcNextProbabilityError, request::PcNextProbabilityRequest,
    result::PcNextProbabilityResult,
};

/// Request-bound response produced by a [`PcNextProbabilityPort`].
pub type PcNextProbabilityPortResponse<Profile, Probability, PortError> = Result<
    PcNextProbabilityResult<Profile, Probability>,
    PcNextProbabilityError<Profile, PortError>,
>;

/// Pure ownership boundary for a possible future n-PC probability provider.
///
/// The associated types intentionally leave boundary and probability semantics
/// to a separately approved product contract. This crate supplies no concrete
/// implementation, registration, transport, fallback, or activation switch.
pub trait PcNextProbabilityPort {
    type Profile: Clone + Eq;
    type Boundary;
    type Probability;
    type Error;

    fn next_probability(
        &self,
        request: &PcNextProbabilityRequest<Self::Profile, Self::Boundary>,
    ) -> PcNextProbabilityPortResponse<Self::Profile, Self::Probability, Self::Error>;
}
