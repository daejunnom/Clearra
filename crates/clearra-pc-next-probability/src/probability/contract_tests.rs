use core::{cell::Cell, convert::Infallible};

use super::{
    error::{
        PcNextProbabilityError, PcNextProbabilityErrorKind, PcProbabilityBindingField,
        PcProbabilityUnsupportedReason,
    },
    pc_krylov_snapshot_adapter::{PcKrylovSnapshotAdapter, PcKrylovSnapshotAdapterResponse},
    pc_next_probability_port::{PcNextProbabilityPort, PcNextProbabilityPortResponse},
    request::{
        PcNextProbabilityRequest, PcProbabilityGeneration, PcProbabilityOwnerId,
        PcProbabilityRequestBinding, PcProbabilityRequestId,
    },
    result::{PcKrylovSnapshotResult, PcNextProbabilityResult},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyntheticProfile {
    Srs,
    SrsX,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SyntheticBoundary(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SyntheticProbability;

fn owner(value: u64) -> PcProbabilityOwnerId {
    PcProbabilityOwnerId::new(value).expect("non-zero synthetic owner")
}

fn generation(value: u64) -> PcProbabilityGeneration {
    PcProbabilityGeneration::new(value).expect("non-zero synthetic generation")
}

fn request_id(value: u64) -> PcProbabilityRequestId {
    PcProbabilityRequestId::new(value).expect("non-zero synthetic request")
}

fn binding(
    owner_value: u64,
    generation_value: u64,
    profile: SyntheticProfile,
    request_value: u64,
) -> PcProbabilityRequestBinding<SyntheticProfile> {
    PcProbabilityRequestBinding::new(
        owner(owner_value),
        generation(generation_value),
        profile,
        request_id(request_value),
    )
}

struct FeatureOffPort;

impl PcNextProbabilityPort for FeatureOffPort {
    type Profile = SyntheticProfile;
    type Boundary = SyntheticBoundary;
    type Probability = SyntheticProbability;
    type Error = Infallible;

    fn next_probability(
        &self,
        request: &PcNextProbabilityRequest<Self::Profile, Self::Boundary>,
    ) -> PcNextProbabilityPortResponse<Self::Profile, Self::Probability, Self::Error> {
        Err(PcNextProbabilityError::unsupported(
            *request.binding(),
            PcProbabilityUnsupportedReason::FeatureOff,
        ))
    }
}

#[test]
fn dormant_port_reports_request_bound_feature_off_without_computation() {
    let expected = binding(1, 2, SyntheticProfile::Srs, 3);
    let request = PcNextProbabilityRequest::new(expected, SyntheticBoundary(4));
    let error = FeatureOffPort
        .next_probability(&request)
        .expect_err("the seam has no active provider");

    assert!(matches!(
        error
            .into_kind_for(request.binding())
            .expect("matching feature-off error binding"),
        PcNextProbabilityErrorKind::Unsupported(PcProbabilityUnsupportedReason::FeatureOff)
    ));
}

#[derive(Debug)]
struct SyntheticSource {
    marker: u64,
    adaptations: Cell<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SyntheticSnapshot(u64);

struct SyntheticAdapter;

impl PcKrylovSnapshotAdapter for SyntheticAdapter {
    type Profile = SyntheticProfile;
    type Source = SyntheticSource;
    type Snapshot = SyntheticSnapshot;
    type Error = Infallible;

    fn adapt_supplied(
        &self,
        source: &Self::Source,
        binding: &PcProbabilityRequestBinding<Self::Profile>,
    ) -> PcKrylovSnapshotAdapterResponse<Self::Profile, Self::Snapshot, Self::Error> {
        source.adaptations.set(source.adaptations.get() + 1);
        Ok(PcKrylovSnapshotResult::new(
            *binding,
            SyntheticSnapshot(source.marker),
        ))
    }
}

#[test]
fn adapter_preserves_binding_while_source_and_snapshot_stay_opaque() {
    let expected = binding(1, 2, SyntheticProfile::Srs, 3);
    let source = SyntheticSource {
        marker: 9,
        adaptations: Cell::new(0),
    };
    let result = SyntheticAdapter
        .adapt_supplied(&source, &expected)
        .expect("synthetic caller-supplied source");

    assert_eq!(source.adaptations.get(), 1);
    assert_eq!(
        result.into_snapshot_for(&expected),
        Ok(SyntheticSnapshot(9))
    );
}

#[test]
fn every_authority_field_rejects_a_stale_probability_result() {
    let expected = binding(1, 2, SyntheticProfile::Srs, 3);
    let stale_cases = [
        (
            binding(9, 2, SyntheticProfile::Srs, 3),
            PcProbabilityBindingField::Owner,
        ),
        (
            binding(1, 9, SyntheticProfile::Srs, 3),
            PcProbabilityBindingField::Generation,
        ),
        (
            binding(1, 2, SyntheticProfile::SrsX, 3),
            PcProbabilityBindingField::Profile,
        ),
        (
            binding(1, 2, SyntheticProfile::Srs, 9),
            PcProbabilityBindingField::Request,
        ),
    ];

    for (received, expected_field) in stale_cases {
        let stale = PcNextProbabilityResult::new(received, SyntheticProbability)
            .into_probability_for(&expected)
            .expect_err("stale result must not expose its opaque probability");
        assert_eq!(stale.field(), expected_field);
    }
}

#[test]
fn cancellation_and_source_failures_are_bound_before_they_are_observed() {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum SyntheticSourceError {
        Rejected,
    }

    let expected = binding(1, 2, SyntheticProfile::Srs, 3);
    let cancelled = PcNextProbabilityError::<_, SyntheticSourceError>::cancelled(expected);
    assert_eq!(
        cancelled.into_kind_for(&expected),
        Ok(PcNextProbabilityErrorKind::Cancelled)
    );

    let old_generation = binding(1, 1, SyntheticProfile::Srs, 3);
    let delayed = PcNextProbabilityError::source(old_generation, SyntheticSourceError::Rejected);
    let stale = delayed
        .into_kind_for(&expected)
        .expect_err("old-generation error must not affect the live request");
    assert_eq!(stale.field(), PcProbabilityBindingField::Generation);
}

#[test]
fn zero_is_reserved_for_uninitialized_authority_identities() {
    assert_eq!(PcProbabilityOwnerId::new(0), None);
    assert_eq!(PcProbabilityGeneration::new(0), None);
    assert_eq!(PcProbabilityRequestId::new(0), None);
}
