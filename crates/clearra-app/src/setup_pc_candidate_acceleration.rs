// SRP rationale: this module has one behavior-level change reason: deciding
// whether one already sealed, complete PC candidate universe may cross the
// feature-off Setup acceleration seam. It performs no lookup, graph parsing,
// placement materialization, Setup evaluation, fallback execution, capability
// activation, score-guided ranking, or future probability-state computation.

mod qualification_binding;

use clearra_pc4_tablebase::{
    Pc4RuleProfile, Pc4TargetLines, Pc4TerminalUseCase, QualifiedPc4TargetIdentity,
};

use crate::{
    PcCandidateProviderKind, PcCandidateReducerInput, PcCandidateRequestIdentity,
    PcCandidateSourceIdentity,
};
use qualification_binding::{
    CandidateBinding, QualificationBinding, QualificationBindingRejection, RequestBinding,
};

#[cfg(test)]
use crate::PcCandidateSourceBinding;
#[cfg(test)]
use clearra_pc4_tablebase::Pc4TerminalFieldIdentity;

pub const SETUP_PC_CANDIDATE_ACCELERATION_CONTRACT: &str = "setup-complete-pc-candidate-input.v2";

/// Exact Setup result meaning whose candidate-source substitution was checked.
///
/// These are deliberately distinct: qualification for one ranking or detail
/// objective grants no authority to accelerate another.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SetupPcAccelerationObjective {
    RankedJoint,
    RankedBuildProbability,
    RankedConditionalPc,
    ExactPathDetail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupPcAccelerationRequestBinding {
    request_identity: PcCandidateRequestIdentity,
    profile: Pc4RuleProfile,
    initial_board_mask: u64,
    target: Pc4TargetLines,
    objective: SetupPcAccelerationObjective,
}

impl SetupPcAccelerationRequestBinding {
    /// Binds the Setup request to the shared PC4 target value. This value only
    /// validates the graph-domain range; admission still requires a qualified
    /// `SetupSearch` target identity for the same profile and snapshot.
    pub const fn new(
        request_identity: PcCandidateRequestIdentity,
        profile: Pc4RuleProfile,
        initial_board_mask: u64,
        target: Pc4TargetLines,
        objective: SetupPcAccelerationObjective,
    ) -> Self {
        Self {
            request_identity,
            profile,
            initial_board_mask,
            target,
            objective,
        }
    }

    pub const fn request_identity(&self) -> PcCandidateRequestIdentity {
        self.request_identity
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.profile
    }

    pub const fn initial_board_mask(&self) -> u64 {
        self.initial_board_mask
    }

    pub const fn target(&self) -> Pc4TargetLines {
        self.target
    }

    pub const fn objective(&self) -> SetupPcAccelerationObjective {
        self.objective
    }
}

/// Differential-qualification authority for one exact Setup substitution.
///
/// There is intentionally no public constructor. A public caller cannot turn
/// a graph hit, a mutable generation label, benchmark success, a ranked-action
/// output, or a synthetic KAT into production qualification. A future trusted
/// verifier may mint this inside `clearra-app` only after comparing the complete
/// graph-derived PC family against the existing offline Setup path for this
/// exact request/profile/field/target/objective/generation binding. The common
/// target identity is embedded so an application-local proof cannot duplicate
/// or substitute target-completeness authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupPcAccelerationCompatibilityProof {
    request: SetupPcAccelerationRequestBinding,
    source_identity: PcCandidateSourceIdentity,
    target: QualifiedPc4TargetIdentity,
    evidence_identity: String,
}

impl SetupPcAccelerationCompatibilityProof {
    pub const fn request(&self) -> &SetupPcAccelerationRequestBinding {
        &self.request
    }

    pub const fn source_identity(&self) -> PcCandidateSourceIdentity {
        self.source_identity
    }

    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub fn evidence_identity(&self) -> &str {
        &self.evidence_identity
    }

    #[cfg(test)]
    fn synthetic(
        request: SetupPcAccelerationRequestBinding,
        source: &PcCandidateSourceBinding,
        target: QualifiedPc4TargetIdentity,
        evidence_identity: impl Into<String>,
    ) -> Self {
        Self {
            request,
            source_identity: source.source_identity(),
            target,
            evidence_identity: evidence_identity.into(),
        }
    }
}

/// Candidate producer outcome before Setup acceleration admission.
///
/// Partial candidates are intentionally discarded at this boundary. A miss or
/// provider failure is not a proof that the Setup request is unsatisfiable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupPcCandidateAvailability {
    Complete(PcCandidateReducerInput),
    Partial,
    LookupMiss,
    ProviderFailure(SetupPcCandidateProviderFailure),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupPcCandidateProviderFailure {
    Offline,
    RateLimited,
    Timeout,
    ProtocolRejected,
    CorruptData,
    ResourceLimit,
    Cancelled,
}

/// Input which the existing Setup owner may consume as its complete PC
/// candidate universe. This value contains no Setup result or capability claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedSetupPcCandidateInput {
    contract_id: &'static str,
    request: SetupPcAccelerationRequestBinding,
    qualification: SetupPcAccelerationCompatibilityProof,
    candidates: PcCandidateReducerInput,
}

impl PreparedSetupPcCandidateInput {
    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn request(&self) -> &SetupPcAccelerationRequestBinding {
        &self.request
    }

    pub const fn qualification(&self) -> &SetupPcAccelerationCompatibilityProof {
        &self.qualification
    }

    pub const fn candidate_input(&self) -> &PcCandidateReducerInput {
        &self.candidates
    }

    pub fn into_candidate_input(self) -> PcCandidateReducerInput {
        self.candidates
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupPcNoAccelerationReason {
    PartialCandidateFamily,
    LookupMiss,
    ProviderFailure(SetupPcCandidateProviderFailure),
    SourceIsNotOnlinePc4,
    RequestIdentityMismatch,
    RuleProfileMismatch,
    InitialBoardMismatch,
    QualificationMissing,
    QualificationRequestMismatch,
    QualificationProfileMismatch,
    QualificationInitialBoardMismatch,
    TargetNotQualified,
    TargetProfileNotQualified,
    TargetUseCaseNotQualified,
    ObjectiveNotQualified,
    CandidateSourceNotQualified,
    CandidateTargetMismatch,
    SnapshotGenerationNotQualified,
    QualificationEvidenceIdentityMissing,
}

impl SetupPcNoAccelerationReason {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::PartialCandidateFamily => "setup_pc_acceleration_partial_candidate_family",
            Self::LookupMiss => "setup_pc_acceleration_lookup_miss",
            Self::ProviderFailure(_) => "setup_pc_acceleration_provider_failure",
            Self::SourceIsNotOnlinePc4 => "setup_pc_acceleration_source_is_not_online_pc4",
            Self::RequestIdentityMismatch => "setup_pc_acceleration_request_identity_mismatch",
            Self::RuleProfileMismatch => "setup_pc_acceleration_rule_profile_mismatch",
            Self::InitialBoardMismatch => "setup_pc_acceleration_initial_board_mismatch",
            Self::QualificationMissing => "setup_pc_acceleration_qualification_missing",
            Self::QualificationRequestMismatch => {
                "setup_pc_acceleration_qualification_request_mismatch"
            }
            Self::QualificationProfileMismatch => {
                "setup_pc_acceleration_qualification_profile_mismatch"
            }
            Self::QualificationInitialBoardMismatch => {
                "setup_pc_acceleration_qualification_initial_board_mismatch"
            }
            Self::TargetNotQualified => "setup_pc_acceleration_target_not_qualified",
            Self::TargetProfileNotQualified => "setup_pc_acceleration_target_profile_not_qualified",
            Self::TargetUseCaseNotQualified => {
                "setup_pc_acceleration_target_use_case_not_qualified"
            }
            Self::ObjectiveNotQualified => "setup_pc_acceleration_objective_not_qualified",
            Self::CandidateSourceNotQualified => {
                "setup_pc_acceleration_candidate_source_not_qualified"
            }
            Self::CandidateTargetMismatch => "setup_pc_acceleration_candidate_target_mismatch",
            Self::SnapshotGenerationNotQualified => {
                "setup_pc_acceleration_snapshot_generation_not_qualified"
            }
            Self::QualificationEvidenceIdentityMissing => {
                "setup_pc_acceleration_qualification_evidence_identity_missing"
            }
        }
    }
}

/// Fail-closed decision. `ReferToOfflineFallbackOwner` is not permission to
/// start work: the product owner must preserve the existing explicit fallback
/// authorization contract. Cancellation remains cancellation, and no variant
/// reports a graph miss as unsatisfiable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupPcAccelerationDisposition {
    Admitted(PreparedSetupPcCandidateInput),
    ReferToOfflineFallbackOwner { reason: SetupPcNoAccelerationReason },
    Cancelled,
}

impl SetupPcAccelerationDisposition {
    pub const fn concludes_unsatisfiable(&self) -> bool {
        false
    }
}

pub fn admit_setup_pc_candidate_input(
    request: SetupPcAccelerationRequestBinding,
    availability: SetupPcCandidateAvailability,
    qualification: Option<&SetupPcAccelerationCompatibilityProof>,
) -> SetupPcAccelerationDisposition {
    let candidates = match availability {
        SetupPcCandidateAvailability::Complete(candidates) => candidates,
        SetupPcCandidateAvailability::Partial => {
            return refer_offline(SetupPcNoAccelerationReason::PartialCandidateFamily)
        }
        SetupPcCandidateAvailability::LookupMiss => {
            return refer_offline(SetupPcNoAccelerationReason::LookupMiss)
        }
        SetupPcCandidateAvailability::ProviderFailure(
            SetupPcCandidateProviderFailure::Cancelled,
        ) => return SetupPcAccelerationDisposition::Cancelled,
        SetupPcCandidateAvailability::ProviderFailure(failure) => {
            return refer_offline(SetupPcNoAccelerationReason::ProviderFailure(failure))
        }
    };
    let source = candidates.source();
    if source.provider_kind() != PcCandidateProviderKind::OnlinePc4 {
        return refer_offline(SetupPcNoAccelerationReason::SourceIsNotOnlinePc4);
    }
    let Some(snapshot) = source.qualified_snapshot() else {
        return refer_offline(SetupPcNoAccelerationReason::SourceIsNotOnlinePc4);
    };
    let Some(qualification) = qualification else {
        return refer_offline(SetupPcNoAccelerationReason::QualificationMissing);
    };
    let source_request_identity = source.request_identity();
    let source_identity = source.source_identity();
    let source_profile = source.profile();
    let request_profile = request.profile;
    let qualified_profile = qualification.request.profile;
    let target_profile = qualification.target.profile();
    let target_lines = qualification.target.target_lines();
    let target_use_case = qualification.target.use_case();
    if let Err(rejection) = qualification_binding::validate(
        CandidateBinding::new(
            source_request_identity.as_bytes(),
            source_identity.as_bytes(),
            &source_profile,
            source.initial_board_mask(),
            snapshot,
        ),
        RequestBinding::new(
            request.request_identity.as_bytes(),
            &request_profile,
            request.initial_board_mask,
            request.target.get(),
            &request.objective,
        ),
        QualificationBinding::new(
            qualification.request.request_identity.as_bytes(),
            qualification.source_identity.as_bytes(),
            &qualified_profile,
            qualification.request.initial_board_mask,
            qualification.request.target.get(),
            &qualification.request.objective,
            &target_profile,
            target_lines.get(),
            &target_use_case,
            &Pc4TerminalUseCase::SetupSearch,
            qualification.target.snapshot(),
            &qualification.evidence_identity,
        ),
    ) {
        return refer_offline(rejection_reason(rejection));
    }
    if candidates.universe_identity().qualified_target() != Some(&qualification.target) {
        return refer_offline(SetupPcNoAccelerationReason::CandidateTargetMismatch);
    }

    SetupPcAccelerationDisposition::Admitted(PreparedSetupPcCandidateInput {
        contract_id: SETUP_PC_CANDIDATE_ACCELERATION_CONTRACT,
        request,
        qualification: qualification.clone(),
        candidates,
    })
}

const fn refer_offline(reason: SetupPcNoAccelerationReason) -> SetupPcAccelerationDisposition {
    SetupPcAccelerationDisposition::ReferToOfflineFallbackOwner { reason }
}

const fn rejection_reason(rejection: QualificationBindingRejection) -> SetupPcNoAccelerationReason {
    match rejection {
        QualificationBindingRejection::RequestIdentity => {
            SetupPcNoAccelerationReason::RequestIdentityMismatch
        }
        QualificationBindingRejection::Profile => SetupPcNoAccelerationReason::RuleProfileMismatch,
        QualificationBindingRejection::InitialBoard => {
            SetupPcNoAccelerationReason::InitialBoardMismatch
        }
        QualificationBindingRejection::QualifiedRequestIdentity => {
            SetupPcNoAccelerationReason::QualificationRequestMismatch
        }
        QualificationBindingRejection::QualifiedProfile => {
            SetupPcNoAccelerationReason::QualificationProfileMismatch
        }
        QualificationBindingRejection::QualifiedInitialBoard => {
            SetupPcNoAccelerationReason::QualificationInitialBoardMismatch
        }
        QualificationBindingRejection::Target => SetupPcNoAccelerationReason::TargetNotQualified,
        QualificationBindingRejection::TargetProfile => {
            SetupPcNoAccelerationReason::TargetProfileNotQualified
        }
        QualificationBindingRejection::TargetUseCase => {
            SetupPcNoAccelerationReason::TargetUseCaseNotQualified
        }
        QualificationBindingRejection::Objective => {
            SetupPcNoAccelerationReason::ObjectiveNotQualified
        }
        QualificationBindingRejection::CandidateSource => {
            SetupPcNoAccelerationReason::CandidateSourceNotQualified
        }
        QualificationBindingRejection::SnapshotGeneration => {
            SetupPcNoAccelerationReason::SnapshotGenerationNotQualified
        }
        QualificationBindingRejection::EvidenceIdentityMissing => {
            SetupPcNoAccelerationReason::QualificationEvidenceIdentityMissing
        }
    }
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU64;

    use clearra_core_domain::{
        piece::piece_kind::PieceKind,
        solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
    };
    use clearra_pc4_tablebase::{
        ArtifactDescriptor, DatasetSnapshotManifest, DatasetSnapshotVerifier, GraphTargetEncoding,
        ManifestContentIdentity, Pc4ArtifactRole, Pc4ProfileManifest, Pc4TerminalUseCase,
        ProfileAvailability, ProfileQualification, ProfileTargetCompletenessQualification,
        QualifiedPc4TargetIdentity, SnapshotIdentity, SnapshotVerificationAttestation,
        SnapshotVerificationFailure, SnapshotVerificationRequest,
    };

    use super::*;
    use crate::PcCandidateSessionId;

    struct SyntheticVerifier;

    impl DatasetSnapshotVerifier for SyntheticVerifier {
        fn verify(
            &mut self,
            request: SnapshotVerificationRequest<'_>,
        ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure> {
            SnapshotVerificationAttestation::new(
                request.snapshot_identity().clone(),
                request.manifest_content_identity().clone(),
                "synthetic-setup-acceleration-snapshot",
            )
            .map_err(|_| SnapshotVerificationFailure::Rejected)
        }
    }

    fn qualified_target(
        generation: &str,
        profile: Pc4RuleProfile,
        use_case: Pc4TerminalUseCase,
        target_lines: u8,
    ) -> QualifiedPc4TargetIdentity {
        let identity = SnapshotIdentity::new(
            "synthetic/repository",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            generation,
        )
        .expect("snapshot identity");
        let profiles = Pc4RuleProfile::ALL
            .into_iter()
            .map(|candidate_profile| {
                let prefix = candidate_profile.as_str();
                let artifact = |role, suffix: &str| {
                    let byte_len = match role {
                        Pc4ArtifactRole::FieldHashIndex | Pc4ArtifactRole::GraphOffsets => 24,
                        Pc4ArtifactRole::Graph => 64,
                    };
                    ArtifactDescriptor::new(
                        role,
                        format!("{prefix}/{suffix}"),
                        byte_len,
                        format!("{prefix}-{suffix}-identity"),
                    )
                    .expect("artifact descriptor")
                };
                let manifest = Pc4ProfileManifest::new(
                    candidate_profile,
                    1,
                    GraphTargetEncoding::U24LittleEndian,
                    clearra_pc4_tablebase::FieldIdIndexRelation::RecordOrdinal,
                    64,
                    artifact(Pc4ArtifactRole::FieldHashIndex, "field.idx"),
                    artifact(Pc4ArtifactRole::GraphOffsets, "offsets.idx"),
                    artifact(Pc4ArtifactRole::Graph, "graph.bin"),
                    ProfileQualification::new(
                        format!("{prefix}-index-spec"),
                        format!("{prefix}-graph-spec"),
                        format!("{prefix}-provenance"),
                        format!("{prefix}-kat"),
                    )
                    .expect("profile qualification"),
                )
                .expect("profile manifest");
                let manifest = if candidate_profile == profile {
                    let target_qualifications = [
                        Pc4TerminalUseCase::PcSearch,
                        Pc4TerminalUseCase::SetupSearch,
                    ]
                    .into_iter()
                    .flat_map(|qualified_use_case| {
                        (Pc4TargetLines::MIN..=Pc4TargetLines::MAX).map(move |qualified_lines| {
                            ProfileTargetCompletenessQualification::new(
                                qualified_use_case,
                                Pc4TargetLines::new(qualified_lines).expect("target lines"),
                                Pc4TerminalFieldIdentity::full_rows(
                                    Pc4TargetLines::new(qualified_lines).expect("target lines"),
                                    0,
                                ),
                                format!("terminal:{qualified_use_case:?}:{qualified_lines}"),
                                format!("outgoing:{qualified_use_case:?}:{qualified_lines}"),
                                format!("answers:{qualified_use_case:?}:{qualified_lines}"),
                                format!("parity:{qualified_use_case:?}:{qualified_lines}"),
                            )
                            .expect("target qualification")
                        })
                    })
                    .collect();
                    manifest
                        .with_target_qualifications(target_qualifications)
                        .expect("unique target qualification")
                } else {
                    manifest
                };
                ProfileAvailability::qualified(manifest)
            })
            .collect();
        let activated = DatasetSnapshotManifest::new(
            identity,
            ManifestContentIdentity::new(format!("manifest-{generation}-{}", profile.as_str()))
                .expect("manifest identity"),
            profiles,
        )
        .expect("snapshot manifest")
        .activate(&mut SyntheticVerifier)
        .expect("verified snapshot");
        activated
            .qualified_target(
                profile,
                use_case,
                Pc4TargetLines::new(target_lines).expect("target lines"),
            )
            .expect("qualified target")
    }

    fn source(
        generation: &str,
        request_byte: u8,
        source_byte: u8,
        profile: Pc4RuleProfile,
        initial_board_mask: u64,
    ) -> PcCandidateSourceBinding {
        let target = qualified_target(
            generation,
            profile,
            Pc4TerminalUseCase::SetupSearch,
            Pc4TargetLines::MAX,
        );
        PcCandidateSourceBinding::online_pc4(
            PcCandidateSessionId::new(NonZeroU64::new(1).expect("session")),
            PcCandidateRequestIdentity::from_sha256([request_byte; 32]),
            PcCandidateSourceIdentity::from_sha256([source_byte; 32]),
            profile,
            initial_board_mask,
            target.snapshot().clone(),
        )
    }

    fn request(
        request_byte: u8,
        profile: Pc4RuleProfile,
        initial_board_mask: u64,
        target_lines: u8,
        objective: SetupPcAccelerationObjective,
    ) -> SetupPcAccelerationRequestBinding {
        SetupPcAccelerationRequestBinding::new(
            PcCandidateRequestIdentity::from_sha256([request_byte; 32]),
            profile,
            initial_board_mask,
            Pc4TargetLines::new(target_lines).expect("target inside graph domain"),
            objective,
        )
    }

    fn reducer(source: PcCandidateSourceBinding) -> PcCandidateReducerInput {
        reducer_for_target(source, Pc4TerminalUseCase::SetupSearch, Pc4TargetLines::MAX)
    }

    fn reducer_for_target(
        source: PcCandidateSourceBinding,
        use_case: Pc4TerminalUseCase,
        target_lines: u8,
    ) -> PcCandidateReducerInput {
        let candidate = StandardBoard64TilingIdentity::from_placements(
            source.initial_board_mask(),
            [PiecePlacementMask::new(PieceKind::I, 0b1111 << 10)],
        )
        .expect("canonical candidate");
        let qualified_target = source.qualified_snapshot().map(|snapshot| {
            qualified_target(
                snapshot.snapshot_identity().generation(),
                source.profile(),
                use_case,
                target_lines,
            )
        });
        PcCandidateReducerInput::from_test_parts(source, qualified_target, vec![candidate])
    }

    fn proof(
        request: SetupPcAccelerationRequestBinding,
        source: &PcCandidateSourceBinding,
        use_case: Pc4TerminalUseCase,
        evidence_identity: impl Into<String>,
    ) -> SetupPcAccelerationCompatibilityProof {
        let generation = source
            .qualified_snapshot()
            .expect("synthetic proof requires an online source")
            .snapshot_identity()
            .generation();
        let target = qualified_target(
            generation,
            request.profile(),
            use_case,
            request.target().get(),
        );
        SetupPcAccelerationCompatibilityProof::synthetic(request, source, target, evidence_identity)
    }

    fn admitted_for(
        source: PcCandidateSourceBinding,
        request: SetupPcAccelerationRequestBinding,
    ) -> SetupPcAccelerationDisposition {
        let proof = proof(
            request.clone(),
            &source,
            Pc4TerminalUseCase::SetupSearch,
            "synthetic-differential-proof",
        );
        admit_setup_pc_candidate_input(
            request,
            SetupPcCandidateAvailability::Complete(reducer(source)),
            Some(&proof),
        )
    }

    #[test]
    fn every_pc4_domain_target_requires_and_accepts_only_its_exact_proof() {
        for target_lines in 1..=4 {
            let source = source("generation-a", 1, 2, Pc4RuleProfile::Srs, 0);
            let request = request(
                1,
                Pc4RuleProfile::Srs,
                0,
                target_lines,
                SetupPcAccelerationObjective::RankedJoint,
            );
            let SetupPcAccelerationDisposition::Admitted(prepared) =
                admitted_for(source.clone(), request.clone())
            else {
                panic!("exact per-target proof must admit {target_lines}L")
            };
            assert_eq!(
                prepared.contract_id(),
                SETUP_PC_CANDIDATE_ACCELERATION_CONTRACT
            );
            assert_eq!(prepared.request(), &request);
            assert_eq!(prepared.candidate_input().source(), &source);
            assert_eq!(prepared.into_candidate_input().candidates().len(), 1);
        }
    }

    #[test]
    fn request_binding_uses_the_shared_one_through_four_line_target_type() {
        for lines in [0, 5, 6] {
            assert!(Pc4TargetLines::new(lines).is_err());
        }
    }

    #[test]
    fn partial_miss_and_provider_failure_all_fall_back_without_unsat_authority() {
        let request = request(
            1,
            Pc4RuleProfile::Srs,
            0,
            4,
            SetupPcAccelerationObjective::RankedJoint,
        );
        for (availability, expected) in [
            (
                SetupPcCandidateAvailability::Partial,
                SetupPcNoAccelerationReason::PartialCandidateFamily,
            ),
            (
                SetupPcCandidateAvailability::LookupMiss,
                SetupPcNoAccelerationReason::LookupMiss,
            ),
            (
                SetupPcCandidateAvailability::ProviderFailure(
                    SetupPcCandidateProviderFailure::RateLimited,
                ),
                SetupPcNoAccelerationReason::ProviderFailure(
                    SetupPcCandidateProviderFailure::RateLimited,
                ),
            ),
        ] {
            let disposition = admit_setup_pc_candidate_input(request.clone(), availability, None);
            assert_eq!(
                disposition,
                SetupPcAccelerationDisposition::ReferToOfflineFallbackOwner { reason: expected }
            );
            assert!(!disposition.concludes_unsatisfiable());
        }
    }

    #[test]
    fn cancellation_is_preserved_and_never_referred_to_the_fallback_owner() {
        let request = request(
            1,
            Pc4RuleProfile::Srs,
            0,
            4,
            SetupPcAccelerationObjective::RankedJoint,
        );
        let disposition = admit_setup_pc_candidate_input(
            request,
            SetupPcCandidateAvailability::ProviderFailure(
                SetupPcCandidateProviderFailure::Cancelled,
            ),
            None,
        );
        assert_eq!(disposition, SetupPcAccelerationDisposition::Cancelled);
        assert!(!disposition.concludes_unsatisfiable());
    }

    #[test]
    fn a_complete_family_without_differential_qualification_stays_offline() {
        let source = source("generation-a", 1, 2, Pc4RuleProfile::Srs, 0);
        let request = request(
            1,
            Pc4RuleProfile::Srs,
            0,
            4,
            SetupPcAccelerationObjective::RankedJoint,
        );
        assert_eq!(
            admit_setup_pc_candidate_input(
                request,
                SetupPcCandidateAvailability::Complete(reducer(source)),
                None,
            ),
            SetupPcAccelerationDisposition::ReferToOfflineFallbackOwner {
                reason: SetupPcNoAccelerationReason::QualificationMissing,
            }
        );
    }

    #[test]
    fn pc_search_target_authority_cannot_authorize_setup_acceleration() {
        let source = source("generation-a", 1, 2, Pc4RuleProfile::Srs, 0);
        let request = request(
            1,
            Pc4RuleProfile::Srs,
            0,
            4,
            SetupPcAccelerationObjective::RankedJoint,
        );
        let proof = proof(
            request.clone(),
            &source,
            Pc4TerminalUseCase::PcSearch,
            "synthetic-wrong-use-case-proof",
        );
        assert_eq!(proof.target().use_case(), Pc4TerminalUseCase::PcSearch);
        assert_eq!(
            admit_setup_pc_candidate_input(
                request,
                SetupPcCandidateAvailability::Complete(reducer(source)),
                Some(&proof),
            ),
            SetupPcAccelerationDisposition::ReferToOfflineFallbackOwner {
                reason: SetupPcNoAccelerationReason::TargetUseCaseNotQualified,
            }
        );
    }

    #[test]
    fn target_authority_for_another_profile_cannot_cross_the_setup_request() {
        let source = source("generation-a", 1, 2, Pc4RuleProfile::Srs, 0);
        let request = request(
            1,
            Pc4RuleProfile::Srs,
            0,
            4,
            SetupPcAccelerationObjective::RankedJoint,
        );
        let target = qualified_target(
            "generation-a",
            Pc4RuleProfile::SrsX,
            Pc4TerminalUseCase::SetupSearch,
            4,
        );
        let proof = SetupPcAccelerationCompatibilityProof::synthetic(
            request.clone(),
            &source,
            target,
            "synthetic-wrong-profile-proof",
        );
        assert_eq!(
            admit_setup_pc_candidate_input(
                request,
                SetupPcCandidateAvailability::Complete(reducer(source)),
                Some(&proof),
            ),
            SetupPcAccelerationDisposition::ReferToOfflineFallbackOwner {
                reason: SetupPcNoAccelerationReason::TargetProfileNotQualified,
            }
        );
    }

    #[test]
    fn target_authority_for_another_line_count_cannot_cross_the_setup_request() {
        let source = source("generation-a", 1, 2, Pc4RuleProfile::Srs, 0);
        let request = request(
            1,
            Pc4RuleProfile::Srs,
            0,
            4,
            SetupPcAccelerationObjective::RankedJoint,
        );
        let target = qualified_target(
            "generation-a",
            Pc4RuleProfile::Srs,
            Pc4TerminalUseCase::SetupSearch,
            3,
        );
        let proof = SetupPcAccelerationCompatibilityProof::synthetic(
            request.clone(),
            &source,
            target,
            "synthetic-wrong-target-proof",
        );
        assert_eq!(
            admit_setup_pc_candidate_input(
                request,
                SetupPcCandidateAvailability::Complete(reducer(source)),
                Some(&proof),
            ),
            SetupPcAccelerationDisposition::ReferToOfflineFallbackOwner {
                reason: SetupPcNoAccelerationReason::TargetNotQualified,
            }
        );
    }

    #[test]
    fn candidate_universe_target_cannot_borrow_a_pc_or_other_line_qualification() {
        let source = source("generation-a", 1, 2, Pc4RuleProfile::Srs, 0);
        let request = request(
            1,
            Pc4RuleProfile::Srs,
            0,
            4,
            SetupPcAccelerationObjective::RankedJoint,
        );
        let proof = proof(
            request.clone(),
            &source,
            Pc4TerminalUseCase::SetupSearch,
            "synthetic-exact-setup-proof",
        );

        for candidates in [
            reducer_for_target(source.clone(), Pc4TerminalUseCase::PcSearch, 4),
            reducer_for_target(source.clone(), Pc4TerminalUseCase::SetupSearch, 3),
        ] {
            assert_eq!(
                admit_setup_pc_candidate_input(
                    request.clone(),
                    SetupPcCandidateAvailability::Complete(candidates),
                    Some(&proof),
                ),
                SetupPcAccelerationDisposition::ReferToOfflineFallbackOwner {
                    reason: SetupPcNoAccelerationReason::CandidateTargetMismatch,
                }
            );
        }
    }

    #[test]
    fn candidate_source_must_match_request_profile_and_initial_board() {
        let base_request = request(
            1,
            Pc4RuleProfile::Srs,
            0,
            4,
            SetupPcAccelerationObjective::RankedJoint,
        );
        for (mixed, expected) in [
            (
                source("generation-a", 9, 2, Pc4RuleProfile::Srs, 0),
                SetupPcNoAccelerationReason::RequestIdentityMismatch,
            ),
            (
                source("generation-a", 1, 2, Pc4RuleProfile::SrsX, 0),
                SetupPcNoAccelerationReason::RuleProfileMismatch,
            ),
            (
                source("generation-a", 1, 2, Pc4RuleProfile::Srs, 0x0f),
                SetupPcNoAccelerationReason::InitialBoardMismatch,
            ),
        ] {
            let proof = proof(
                base_request.clone(),
                &mixed,
                Pc4TerminalUseCase::SetupSearch,
                "synthetic-mixed-binding-proof",
            );
            assert_eq!(
                admit_setup_pc_candidate_input(
                    base_request.clone(),
                    SetupPcCandidateAvailability::Complete(reducer(mixed)),
                    Some(&proof),
                ),
                SetupPcAccelerationDisposition::ReferToOfflineFallbackOwner { reason: expected }
            );
        }
    }

    #[test]
    fn proof_cannot_cross_generation_source_target_or_objective() {
        let qualified_source = source("generation-a", 1, 2, Pc4RuleProfile::Srs, 0);
        let qualified_request = request(
            1,
            Pc4RuleProfile::Srs,
            0,
            4,
            SetupPcAccelerationObjective::RankedJoint,
        );
        let proof = proof(
            qualified_request.clone(),
            &qualified_source,
            Pc4TerminalUseCase::SetupSearch,
            "synthetic-exact-proof",
        );

        let generation_b = source("generation-b", 1, 2, Pc4RuleProfile::Srs, 0);
        assert_eq!(
            admit_setup_pc_candidate_input(
                qualified_request.clone(),
                SetupPcCandidateAvailability::Complete(reducer(generation_b)),
                Some(&proof),
            ),
            SetupPcAccelerationDisposition::ReferToOfflineFallbackOwner {
                reason: SetupPcNoAccelerationReason::SnapshotGenerationNotQualified,
            }
        );

        let different_source = source("generation-a", 1, 9, Pc4RuleProfile::Srs, 0);
        assert_eq!(
            admit_setup_pc_candidate_input(
                qualified_request.clone(),
                SetupPcCandidateAvailability::Complete(reducer(different_source)),
                Some(&proof),
            ),
            SetupPcAccelerationDisposition::ReferToOfflineFallbackOwner {
                reason: SetupPcNoAccelerationReason::CandidateSourceNotQualified,
            }
        );

        for (request, expected) in [
            (
                request(
                    1,
                    Pc4RuleProfile::Srs,
                    0,
                    3,
                    SetupPcAccelerationObjective::RankedJoint,
                ),
                SetupPcNoAccelerationReason::TargetNotQualified,
            ),
            (
                request(
                    1,
                    Pc4RuleProfile::Srs,
                    0,
                    4,
                    SetupPcAccelerationObjective::RankedBuildProbability,
                ),
                SetupPcNoAccelerationReason::ObjectiveNotQualified,
            ),
        ] {
            assert_eq!(
                admit_setup_pc_candidate_input(
                    request,
                    SetupPcCandidateAvailability::Complete(reducer(qualified_source.clone())),
                    Some(&proof),
                ),
                SetupPcAccelerationDisposition::ReferToOfflineFallbackOwner { reason: expected }
            );
        }
    }

    #[test]
    fn empty_differential_evidence_identity_cannot_activate_acceleration() {
        let source = source("generation-a", 1, 2, Pc4RuleProfile::Srs, 0);
        let request = request(
            1,
            Pc4RuleProfile::Srs,
            0,
            4,
            SetupPcAccelerationObjective::RankedJoint,
        );
        let proof = proof(
            request.clone(),
            &source,
            Pc4TerminalUseCase::SetupSearch,
            "  ",
        );
        assert_eq!(
            admit_setup_pc_candidate_input(
                request,
                SetupPcCandidateAvailability::Complete(reducer(source)),
                Some(&proof),
            ),
            SetupPcAccelerationDisposition::ReferToOfflineFallbackOwner {
                reason: SetupPcNoAccelerationReason::QualificationEvidenceIdentityMissing,
            }
        );
    }

    #[test]
    fn offline_exact_candidates_are_not_misrepresented_as_pc4_acceleration() {
        let source = PcCandidateSourceBinding::offline_exact(
            PcCandidateSessionId::new(NonZeroU64::new(1).expect("session")),
            PcCandidateRequestIdentity::from_sha256([1; 32]),
            PcCandidateSourceIdentity::from_sha256([2; 32]),
            Pc4RuleProfile::Srs,
            0,
        );
        let request = request(
            1,
            Pc4RuleProfile::Srs,
            0,
            4,
            SetupPcAccelerationObjective::RankedJoint,
        );
        assert_eq!(
            admit_setup_pc_candidate_input(
                request,
                SetupPcCandidateAvailability::Complete(reducer(source)),
                None,
            ),
            SetupPcAccelerationDisposition::ReferToOfflineFallbackOwner {
                reason: SetupPcNoAccelerationReason::SourceIsNotOnlinePc4,
            }
        );
    }
}
