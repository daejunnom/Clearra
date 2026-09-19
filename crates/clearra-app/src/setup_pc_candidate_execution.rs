// SRP rationale: this module has one change reason: consuming an App-admitted
// complete PC4 candidate family through Core's existing Setup evaluator.
// Lookup, transport, graph parsing, compatibility-proof minting, ranking,
// fallback selection and capability activation remain with their current
// owners.

use std::sync::Arc;

use clearra_core_domain::execution_cancellation::ExecutionControl;
#[cfg(not(target_family = "wasm"))]
use clearra_core_executor::WasmSetupParallelCoordinator;
use clearra_core_executor::{CoreExecutionResult, WasmCpuSearchError, WasmSetupSearchBackend};
use clearra_pc4_tablebase::Pc4RuleProfile;
use clearra_problem::{SetupCandidatePriority, SetupSearchQuery};
use clearra_rules::profile::rule_profile::RuleProfileId;

use crate::{
    PcCandidateBoundaryError, PcCandidateSetDigest, PcCandidateUniverseIdentity,
    PreparedSetupPcCandidateInput, SetupPcAccelerationObjective, SetupPcAccelerationRequestBinding,
    SETUP_PC_CANDIDATE_ACCELERATION_CONTRACT,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedSetupPcCandidateExecutionEvidence {
    universe_identity: PcCandidateUniverseIdentity,
    request: SetupPcAccelerationRequestBinding,
    compatibility_evidence_identity: String,
}

impl ValidatedSetupPcCandidateExecutionEvidence {
    pub const fn universe_identity(&self) -> &PcCandidateUniverseIdentity {
        &self.universe_identity
    }

    pub const fn request(&self) -> &SetupPcAccelerationRequestBinding {
        &self.request
    }

    pub fn compatibility_evidence_identity(&self) -> &str {
        &self.compatibility_evidence_identity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupPcCandidateExecutionError {
    Cancelled,
    PartitionedExecutionControl,
    ContractMismatch,
    TablebaseNotRequested,
    UnsupportedSetupBoard,
    TargetMismatch,
    InitialBoardMismatch,
    RuleProfileMismatch,
    ObjectiveMismatch,
    CandidateInitialBoardMismatch,
    CandidatesNotStrictlyCanonical,
    CandidateCountOverflow,
    CandidateCountMismatch,
    CandidateDigest(PcCandidateBoundaryError),
    CandidateDigestMismatch,
    Core(WasmCpuSearchError),
}

impl SetupPcCandidateExecutionError {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "setup_pc_candidate_execution_cancelled",
            Self::PartitionedExecutionControl => {
                "setup_pc_candidate_execution_partitioned_control_not_allowed"
            }
            Self::ContractMismatch => "setup_pc_candidate_execution_contract_mismatch",
            Self::TablebaseNotRequested => "setup_pc_candidate_execution_tablebase_not_requested",
            Self::UnsupportedSetupBoard => "setup_pc_candidate_execution_board_unsupported",
            Self::TargetMismatch => "setup_pc_candidate_execution_target_mismatch",
            Self::InitialBoardMismatch => "setup_pc_candidate_execution_initial_board_mismatch",
            Self::RuleProfileMismatch => "setup_pc_candidate_execution_rule_profile_mismatch",
            Self::ObjectiveMismatch => "setup_pc_candidate_execution_objective_mismatch",
            Self::CandidateInitialBoardMismatch => {
                "setup_pc_candidate_execution_candidate_initial_board_mismatch"
            }
            Self::CandidatesNotStrictlyCanonical => {
                "setup_pc_candidate_execution_candidates_not_strictly_canonical"
            }
            Self::CandidateCountOverflow => "setup_pc_candidate_execution_candidate_count_overflow",
            Self::CandidateCountMismatch => "setup_pc_candidate_execution_candidate_count_mismatch",
            Self::CandidateDigest(error) => error.reason(),
            Self::CandidateDigestMismatch => {
                "setup_pc_candidate_execution_candidate_digest_mismatch"
            }
            Self::Core(error) => error.reason(),
        }
    }
}

impl core::fmt::Display for SetupPcCandidateExecutionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for SetupPcCandidateExecutionError {}

/// Runs a complete candidate family only after the separate admission owner
/// has attached an exact SetupSearch differential proof. The ordinary Setup
/// graph remains authoritative for BuildUp, shape, spin, score, probability
/// and result grouping; candidates replace only its initial PC geometry
/// universe.
pub fn execute_prepared_setup_pc_candidate_input(
    prepared: PreparedSetupPcCandidateInput,
    query: &SetupSearchQuery,
    workers: usize,
    control: &ExecutionControl,
) -> Result<
    (
        CoreExecutionResult,
        ValidatedSetupPcCandidateExecutionEvidence,
    ),
    SetupPcCandidateExecutionError,
> {
    if control.is_cancelled() {
        return Err(SetupPcCandidateExecutionError::Cancelled);
    }
    if control.partition().count() != 1 || control.partition().index() != 0 {
        return Err(SetupPcCandidateExecutionError::PartitionedExecutionControl);
    }
    if prepared.contract_id() != SETUP_PC_CANDIDATE_ACCELERATION_CONTRACT {
        return Err(SetupPcCandidateExecutionError::ContractMismatch);
    }
    let request = prepared.request().clone();
    validate_setup_query_binding(query, &request)?;
    let compatibility_evidence_identity = prepared.qualification().evidence_identity().to_owned();
    let input = prepared.into_candidate_input();
    let universe_identity = input.universe_identity().clone();
    if input
        .candidates()
        .iter()
        .any(|candidate| candidate.initial_board_mask() != request.initial_board_mask())
    {
        return Err(SetupPcCandidateExecutionError::CandidateInitialBoardMismatch);
    }
    if input.candidates().windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(SetupPcCandidateExecutionError::CandidatesNotStrictlyCanonical);
    }
    let actual_count = u64::try_from(input.candidates().len())
        .map_err(|_| SetupPcCandidateExecutionError::CandidateCountOverflow)?;
    if actual_count != universe_identity.exact_candidate_count() {
        return Err(SetupPcCandidateExecutionError::CandidateCountMismatch);
    }
    let actual_digest = PcCandidateSetDigest::calculate(input.candidates())
        .map_err(SetupPcCandidateExecutionError::CandidateDigest)?;
    if actual_digest != universe_identity.candidate_set_digest() {
        return Err(SetupPcCandidateExecutionError::CandidateDigestMismatch);
    }

    let candidates: Arc<[_]> = input.into_candidates().into();
    let workers = workers.max(1);
    #[cfg(not(target_family = "wasm"))]
    let result = if workers > 1 {
        WasmSetupParallelCoordinator::execute_native_with_complete_precomputed_pc_candidates(
            query, workers, candidates, control,
        )
        .map_err(SetupPcCandidateExecutionError::Core)?
    } else {
        WasmSetupSearchBackend::execute_with_complete_precomputed_pc_candidates_and_control(
            query, candidates, workers, control,
        )
        .map_err(SetupPcCandidateExecutionError::Core)?
    };
    #[cfg(target_family = "wasm")]
    let result =
        WasmSetupSearchBackend::execute_with_complete_precomputed_pc_candidates_and_control(
            query, candidates, workers, control,
        )
        .map_err(SetupPcCandidateExecutionError::Core)?;
    Ok((
        result,
        ValidatedSetupPcCandidateExecutionEvidence {
            universe_identity,
            request,
            compatibility_evidence_identity,
        },
    ))
}

fn validate_setup_query_binding(
    query: &SetupSearchQuery,
    request: &SetupPcAccelerationRequestBinding,
) -> Result<(), SetupPcCandidateExecutionError> {
    if !query.tablebase_requested() {
        return Err(SetupPcCandidateExecutionError::TablebaseNotRequested);
    }
    let board = query.board_size();
    if board.width() != 10
        || board.height() != 4
        || request.target().get() != 4
        || board.height() != u16::from(request.target().get())
    {
        return Err(SetupPcCandidateExecutionError::UnsupportedSetupBoard);
    }
    if query.target().lines() != request.target().get() {
        return Err(SetupPcCandidateExecutionError::TargetMismatch);
    }
    // The current Setup engine's target frame is empty. A future scenario-
    // Setup owner must add an explicit board value instead of silently
    // projecting a non-empty PC4 request into this route.
    if request.initial_board_mask() != 0 {
        return Err(SetupPcCandidateExecutionError::InitialBoardMismatch);
    }
    if query_profile(query.rule().id()) != Some(request.profile()) {
        return Err(SetupPcCandidateExecutionError::RuleProfileMismatch);
    }
    if query_objective(query) != request.objective() {
        return Err(SetupPcCandidateExecutionError::ObjectiveMismatch);
    }
    Ok(())
}

const fn query_profile(profile: RuleProfileId) -> Option<Pc4RuleProfile> {
    match profile {
        RuleProfileId::Srs => Some(Pc4RuleProfile::Srs),
        RuleProfileId::SrsPlus => Some(Pc4RuleProfile::SrsPlus),
        RuleProfileId::SrsX => Some(Pc4RuleProfile::SrsX),
        RuleProfileId::Jstris180 => Some(Pc4RuleProfile::Jstris180),
        RuleProfileId::NoKick => Some(Pc4RuleProfile::NoKick),
        RuleProfileId::Custom => None,
    }
}

fn query_objective(query: &SetupSearchQuery) -> SetupPcAccelerationObjective {
    if query.path_detail().is_some() {
        return SetupPcAccelerationObjective::ExactPathDetail;
    }
    match query.candidate_priority() {
        SetupCandidatePriority::All => SetupPcAccelerationObjective::RankedJoint,
        SetupCandidatePriority::BuildProbabilityFirst => {
            SetupPcAccelerationObjective::RankedBuildProbability
        }
        SetupCandidatePriority::PcProbabilityFirst => {
            SetupPcAccelerationObjective::RankedConditionalPc
        }
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{board::board_size::BoardSize, pc::pc_target::PcTarget};
    use clearra_pc4_tablebase::Pc4TargetLines;
    use clearra_problem::{
        GroupingMode, PieceBudget, SetupHoldPolicy, SetupLimits, SetupProbabilityFilter,
        SetupQueueInput,
    };
    use clearra_rules::profile::builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x};

    use super::*;
    use crate::PcCandidateRequestIdentity;

    #[test]
    fn all_profile_and_objective_mappings_are_exact() {
        for (profile, expected) in [
            (srs(), Pc4RuleProfile::Srs),
            (srs_plus(), Pc4RuleProfile::SrsPlus),
            (srs_x(), Pc4RuleProfile::SrsX),
            (jstris_180(), Pc4RuleProfile::Jstris180),
            (no_kick(), Pc4RuleProfile::NoKick),
        ] {
            assert_eq!(query_profile(profile.id()), Some(expected));
        }

        let base = SetupSearchQuery::default();
        assert_eq!(
            query_objective(&base),
            SetupPcAccelerationObjective::RankedJoint
        );
        assert_eq!(
            query_objective(
                &base
                    .clone()
                    .with_candidate_priority(SetupCandidatePriority::BuildProbabilityFirst)
            ),
            SetupPcAccelerationObjective::RankedBuildProbability
        );
        assert_eq!(
            query_objective(
                &base.with_candidate_priority(SetupCandidatePriority::PcProbabilityFirst)
            ),
            SetupPcAccelerationObjective::RankedConditionalPc
        );
    }

    #[test]
    fn setup_domain_check_does_not_generalize_beyond_the_bound_target() {
        let query = SetupSearchQuery::new(
            BoardSize::new(10, 2).expect("board"),
            PcTarget::two_lines(),
            SetupQueueInput::default(),
            SetupHoldPolicy::default(),
            PieceBudget::default(),
            SetupProbabilityFilter::default(),
            GroupingMode::default(),
            SetupLimits::default(),
        )
        .with_tablebase_requested(true);
        let request = SetupPcAccelerationRequestBinding::new(
            PcCandidateRequestIdentity::from_sha256([1; 32]),
            Pc4RuleProfile::SrsPlus,
            0,
            Pc4TargetLines::new(2).expect("target"),
            SetupPcAccelerationObjective::RankedJoint,
        );
        assert_eq!(
            validate_setup_query_binding(&query, &request),
            Err(SetupPcCandidateExecutionError::UnsupportedSetupBoard)
        );
    }

    #[test]
    fn exact_four_line_query_binding_is_admitted_before_candidate_validation() {
        let query = SetupSearchQuery::default().with_tablebase_requested(true);
        let request = SetupPcAccelerationRequestBinding::new(
            PcCandidateRequestIdentity::from_sha256([2; 32]),
            Pc4RuleProfile::SrsPlus,
            0,
            Pc4TargetLines::new(4).expect("target"),
            SetupPcAccelerationObjective::RankedJoint,
        );
        assert_eq!(validate_setup_query_binding(&query, &request), Ok(()));
    }
}
