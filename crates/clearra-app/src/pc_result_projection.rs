// SRP rationale: this module has one behavior-level change reason: validating typed PC result projections against their originating search contracts.

use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy,
    score_objective_policy::{ScoreObjectiveMode, SpinProfileSelection},
};
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
    PcScenarioQuery, PcSolutionProbabilityPolicy, RequestedSearchBackend, SupplyWindowSize,
    WorkerPolicy,
};
use clearra_supply::{hold::hold_slot::HoldSlot, QueueObservationPolicy};

use crate::{
    pc_chance_probability_result::PcChanceIngressOrigin,
    pc_minimum_cover_result::{
        validate_pc_minimals_common_request_contract, validate_pc_minimals_scenario_shape,
        PcMinimalsIngressOrigin, PC_MINIMUM_COVER_RESULT_CONTRACT,
    },
    pc_path_result::{PcPathIngressOrigin, PC_PATH_FAMILY_RESULT_CONTRACT},
    pc_save_result::{
        PcSaveIngressOrigin, PC_BEST_SAVE_RESULT_CONTRACT, PC_SAVE_GROUPS_RESULT_CONTRACT,
    },
    pc_score_minimum_cover_result::{
        PcScoreMinimalsIngressOrigin, PC_SCORE_PORTFOLIO_RESULT_CONTRACT,
    },
    pc_score_summary_result::{
        PcScoreIngressOrigin, PC_FIXED_SCORE_WITNESS_RESULT_CONTRACT, PC_SCORE_RESULT_CONTRACT,
    },
    pc_tiling_family_result::{PcTilingIngressOrigin, PC_TILING_FAMILY_RESULT_CONTRACT},
};

const EXACT_WITNESS_CONTRACT: &str = "pc-b2b-preserving-witness.v1";
const PATTERN_PROBABILITY_CONTRACT: &str = "pc-b2b-preservation-probability.v1";

/// Product-owned structural limits for `pc.score`. These are independently
/// enforced after parsing so typed callers cannot bypass the ingress envelope.
pub const PC_SCORE_MAX_PATTERN_BYTES: usize = 128;
pub const PC_SCORE_MAX_SOURCE_PIECES: usize = 16;
pub const PC_SCORE_MAX_PATTERNS: usize = 1_066_867_200;
const PC_SCORE_MAX_EXPLICIT_PATTERNS: usize = 4_096;
const PC_SCORE_MAX_QUERY_QUEUE_RETAINED_BYTES: u128 = 1024 * 1024;

/// Closed product ceiling for every request-owned allocation retained outside
/// the WASM search session. This constant is not evidence by itself: the typed
/// query is measured before compilation, then the query/problem authority and
/// each direct/cooperative concurrent phase are fieldwise checked against this
/// ceiling. Only a successful combined proof reserves the full sixteen MiB in
/// Core, and unused bytes are never exposed as reusable allocation credit.
pub(crate) const PC_SCORE_EXTERNAL_RETAINED_UPPER_BOUND_BYTES: u128 = 16 * 1024 * 1024;

/// Closed ceiling for request/App owners retained outside a canonical
/// `pc.tiling` WASM session. The authority measures the actual query/problem
/// graph and returns this full conservative reservation only after the
/// combined request phase fits inside it.
pub(crate) const PC_TILING_EXTERNAL_RETAINED_UPPER_BOUND_BYTES: u128 = 16 * 1024 * 1024;

/// Closed result projection selected by a typed PC request.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PcResultProjection {
    #[default]
    Standard,
    ChanceProbabilityV2(PcChanceIngressOrigin),
    MinimumCoverV2(PcMinimalsIngressOrigin),
    PathFamilyV2(PcPathIngressOrigin),
    ScoreSummaryV2(PcScoreIngressOrigin),
    ScorePortfolioV2(PcScoreMinimalsIngressOrigin),
    SaveGroupsV2(PcSaveIngressOrigin),
    BestSaveV2(PcSaveIngressOrigin),
    TilingFamilyV1(PcTilingIngressOrigin),
    AllSpinSolution(SpinProfileSelection),
    AllSpinPreservationChance(SpinProfileSelection),
}

impl PcResultProjection {
    pub const fn is_standard(self) -> bool {
        matches!(self, Self::Standard)
    }

    pub const fn spin_profile(self) -> Option<SpinProfileSelection> {
        match self {
            Self::Standard
            | Self::ChanceProbabilityV2(_)
            | Self::MinimumCoverV2(_)
            | Self::PathFamilyV2(_)
            | Self::ScoreSummaryV2(_)
            | Self::ScorePortfolioV2(_)
            | Self::SaveGroupsV2(_)
            | Self::BestSaveV2(_)
            | Self::TilingFamilyV1(_) => None,
            Self::AllSpinSolution(profile) | Self::AllSpinPreservationChance(profile) => {
                Some(profile)
            }
        }
    }

    pub const fn chance_origin(self) -> Option<PcChanceIngressOrigin> {
        match self {
            Self::ChanceProbabilityV2(origin) => Some(origin),
            Self::Standard
            | Self::MinimumCoverV2(_)
            | Self::PathFamilyV2(_)
            | Self::SaveGroupsV2(_)
            | Self::BestSaveV2(_)
            | Self::ScoreSummaryV2(_)
            | Self::ScorePortfolioV2(_)
            | Self::TilingFamilyV1(_)
            | Self::AllSpinSolution(_)
            | Self::AllSpinPreservationChance(_) => None,
        }
    }

    pub const fn score_origin(self) -> Option<PcScoreIngressOrigin> {
        match self {
            Self::ScoreSummaryV2(origin) => Some(origin),
            Self::Standard
            | Self::ChanceProbabilityV2(_)
            | Self::MinimumCoverV2(_)
            | Self::PathFamilyV2(_)
            | Self::ScorePortfolioV2(_)
            | Self::SaveGroupsV2(_)
            | Self::BestSaveV2(_)
            | Self::TilingFamilyV1(_)
            | Self::AllSpinSolution(_)
            | Self::AllSpinPreservationChance(_) => None,
        }
    }

    /// Constructs the distinct B-option result projection selected by the
    /// future `pc.score-minimals` ingress. This never aliases `pc.score`.
    pub const fn pc_score_minimals() -> Self {
        Self::ScorePortfolioV2(PcScoreMinimalsIngressOrigin::CanonicalPcScoreMinimals)
    }

    pub const fn score_minimals_origin(self) -> Option<PcScoreMinimalsIngressOrigin> {
        match self {
            Self::ScorePortfolioV2(origin) => Some(origin),
            Self::Standard
            | Self::ChanceProbabilityV2(_)
            | Self::MinimumCoverV2(_)
            | Self::PathFamilyV2(_)
            | Self::ScoreSummaryV2(_)
            | Self::SaveGroupsV2(_)
            | Self::BestSaveV2(_)
            | Self::TilingFamilyV1(_)
            | Self::AllSpinSolution(_)
            | Self::AllSpinPreservationChance(_) => None,
        }
    }

    pub const fn tiling_origin(self) -> Option<PcTilingIngressOrigin> {
        match self {
            Self::TilingFamilyV1(origin) => Some(origin),
            Self::Standard
            | Self::ChanceProbabilityV2(_)
            | Self::MinimumCoverV2(_)
            | Self::PathFamilyV2(_)
            | Self::ScoreSummaryV2(_)
            | Self::ScorePortfolioV2(_)
            | Self::SaveGroupsV2(_)
            | Self::BestSaveV2(_)
            | Self::AllSpinSolution(_)
            | Self::AllSpinPreservationChance(_) => None,
        }
    }

    pub const fn minimals_origin(self) -> Option<PcMinimalsIngressOrigin> {
        match self {
            Self::MinimumCoverV2(origin) => Some(origin),
            Self::Standard
            | Self::ChanceProbabilityV2(_)
            | Self::PathFamilyV2(_)
            | Self::ScoreSummaryV2(_)
            | Self::ScorePortfolioV2(_)
            | Self::SaveGroupsV2(_)
            | Self::BestSaveV2(_)
            | Self::TilingFamilyV1(_)
            | Self::AllSpinSolution(_)
            | Self::AllSpinPreservationChance(_) => None,
        }
    }

    pub const fn path_origin(self) -> Option<PcPathIngressOrigin> {
        match self {
            Self::PathFamilyV2(origin) => Some(origin),
            Self::Standard
            | Self::ChanceProbabilityV2(_)
            | Self::MinimumCoverV2(_)
            | Self::ScoreSummaryV2(_)
            | Self::ScorePortfolioV2(_)
            | Self::SaveGroupsV2(_)
            | Self::BestSaveV2(_)
            | Self::TilingFamilyV1(_)
            | Self::AllSpinSolution(_)
            | Self::AllSpinPreservationChance(_) => None,
        }
    }

    pub const fn save_origin(self) -> Option<PcSaveIngressOrigin> {
        match self {
            Self::SaveGroupsV2(origin) | Self::BestSaveV2(origin) => Some(origin),
            Self::Standard
            | Self::ChanceProbabilityV2(_)
            | Self::MinimumCoverV2(_)
            | Self::PathFamilyV2(_)
            | Self::ScoreSummaryV2(_)
            | Self::ScorePortfolioV2(_)
            | Self::TilingFamilyV1(_)
            | Self::AllSpinSolution(_)
            | Self::AllSpinPreservationChance(_) => None,
        }
    }

    pub const fn contract_id(self) -> Option<&'static str> {
        match self {
            Self::Standard => None,
            Self::ChanceProbabilityV2(_) => Some("pc-probability.v2"),
            Self::MinimumCoverV2(_) => Some(PC_MINIMUM_COVER_RESULT_CONTRACT),
            Self::PathFamilyV2(_) => Some(PC_PATH_FAMILY_RESULT_CONTRACT),
            Self::ScoreSummaryV2(origin) => Some(if origin.is_score_finder() {
                PC_FIXED_SCORE_WITNESS_RESULT_CONTRACT
            } else {
                PC_SCORE_RESULT_CONTRACT
            }),
            Self::ScorePortfolioV2(_) => Some(PC_SCORE_PORTFOLIO_RESULT_CONTRACT),
            Self::SaveGroupsV2(_) => Some(PC_SAVE_GROUPS_RESULT_CONTRACT),
            Self::BestSaveV2(_) => Some(PC_BEST_SAVE_RESULT_CONTRACT),
            Self::TilingFamilyV1(_) => Some(PC_TILING_FAMILY_RESULT_CONTRACT),
            Self::AllSpinSolution(_) => Some(EXACT_WITNESS_CONTRACT),
            Self::AllSpinPreservationChance(_) => Some(PATTERN_PROBABILITY_CONTRACT),
        }
    }

    pub const fn mode(self) -> Option<&'static str> {
        match self {
            Self::Standard => None,
            Self::ChanceProbabilityV2(_) => Some("chance-probability-v2"),
            Self::MinimumCoverV2(_) => Some("minimum-cover-v2"),
            Self::PathFamilyV2(_) => Some("path-family-v2"),
            Self::ScoreSummaryV2(origin) => Some(if origin.is_score_finder() {
                "fixed-score-witness-v2"
            } else {
                "score-summary-v2"
            }),
            Self::ScorePortfolioV2(_) => Some("score-portfolio-v2"),
            Self::SaveGroupsV2(_) => Some("save-groups-v2"),
            Self::BestSaveV2(_) => Some("best-save-v2"),
            Self::TilingFamilyV1(_) => Some("tiling-family-v1"),
            Self::AllSpinSolution(_) => Some("exact-queue-witness"),
            Self::AllSpinPreservationChance(_) => Some("pattern-preservation-chance"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PcResultProblemProvenance {
    Opening,
    InitialFieldScenario,
}

impl PcResultProblemProvenance {
    pub(crate) const fn problem_preset(self) -> &'static str {
        match self {
            Self::Opening => "opening-pc",
            Self::InitialFieldScenario => "scenario-pc",
        }
    }
}

/// Proof that a PC result projection belongs to the query that produced the
/// execution result.
///
/// This token is intentionally crate-private and can only be created by the
/// fieldwise query validators below. Result projection therefore cannot turn an
/// invalid independently-constructed `(query, projection)` pair into a
/// complete public product result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedPcResultProjection {
    projection: PcResultProjection,
    provenance: PcResultProblemProvenance,
}

impl ValidatedPcResultProjection {
    pub(crate) const fn projection(self) -> PcResultProjection {
        self.projection
    }

    pub(crate) const fn provenance(self) -> PcResultProblemProvenance {
        self.provenance
    }

    #[cfg(test)]
    pub(crate) const fn new_for_test(
        projection: PcResultProjection,
        provenance: PcResultProblemProvenance,
    ) -> Self {
        Self {
            projection,
            provenance,
        }
    }
}

pub(crate) fn validate_opening_pc_result_projection(
    query: &OpeningPcSearchQuery,
    projection: PcResultProjection,
) -> Result<ValidatedPcResultProjection, &'static str> {
    let validated = ValidatedPcResultProjection {
        projection,
        provenance: PcResultProblemProvenance::Opening,
    };
    let profile = match projection {
        PcResultProjection::Standard => return Ok(validated),
        PcResultProjection::ChanceProbabilityV2(_) => {
            validate_pc_chance_opening_request_contract(query)?;
            return Ok(validated);
        }
        PcResultProjection::MinimumCoverV2(_) => {
            if query.execution_policy().max_memory_mib().is_some() {
                return Err(
                    "pc minimals does not support an explicit memory cap until exact replay scratch is accounted",
                );
            }
            validate_pc_minimals_common_request_contract(
                query.objective(),
                query.solution_probability_policy(),
                query.queue_observation_policy(),
            )?;
            return Ok(validated);
        }
        PcResultProjection::PathFamilyV2(_) => {
            validate_pc_path_common_request_contract(
                query.objective(),
                PcCountPolicy::CountAll,
                query.solution_probability_policy(),
                query.queue_observation_policy(),
                query.execution_policy(),
            )?;
            return Ok(validated);
        }
        PcResultProjection::ScoreSummaryV2(origin) => {
            validate_pc_score_opening_request_contract(query, origin)?;
            return Ok(validated);
        }
        PcResultProjection::ScorePortfolioV2(_) => {
            validate_pc_score_minimals_opening_request_contract(query)?;
            return Ok(validated);
        }
        PcResultProjection::SaveGroupsV2(origin) => {
            validate_pc_save_opening_request_contract(query, origin, "save-groups")?;
            return Ok(validated);
        }
        PcResultProjection::BestSaveV2(origin) => {
            validate_pc_save_opening_request_contract(query, origin, "best-save")?;
            return Ok(validated);
        }
        PcResultProjection::TilingFamilyV1(origin) => {
            validate_pc_tiling_opening_request_contract(query, origin)?;
            return Ok(validated);
        }
        PcResultProjection::AllSpinSolution(profile)
        | PcResultProjection::AllSpinPreservationChance(profile) => profile,
    };

    if !matches!(query.target().lines(), 2 | 4 | 6) {
        return Err("pc All-Spin opening target must be exactly 2, 4, or 6 lines");
    }
    validate_queue_projection(query.queue(), projection)?;
    if matches!(query.hold_policy(), PcHoldPolicy::EnabledWithPiece(_)) {
        return Err("pc All-Spin does not accept an occupied initial hold slot");
    }
    validate_common_request_contract(
        query.objective(),
        profile,
        query.solution_probability_policy(),
        query.queue_observation_policy(),
    )?;

    let required_pieces = usize::from(query.target().lines()) * 10 / 4;
    validate_supply_window(
        query.queue(),
        query.supply_window_size(),
        required_pieces,
        query.hold_policy().is_enabled(),
    )?;
    Ok(validated)
}

pub(crate) fn validate_scenario_pc_result_projection(
    query: &PcScenarioQuery,
    projection: PcResultProjection,
) -> Result<ValidatedPcResultProjection, &'static str> {
    let validated = ValidatedPcResultProjection {
        projection,
        provenance: PcResultProblemProvenance::InitialFieldScenario,
    };
    let profile = match projection {
        PcResultProjection::Standard => return Ok(validated),
        PcResultProjection::ChanceProbabilityV2(_) => {
            validate_pc_chance_scenario_request_contract(query)?;
            return Ok(validated);
        }
        PcResultProjection::MinimumCoverV2(_) => {
            if query.execution_policy().max_memory_mib().is_some() {
                return Err(
                    "pc minimals does not support an explicit memory cap until exact replay scratch is accounted",
                );
            }
            validate_pc_minimals_common_request_contract(
                query.objective(),
                query.solution_probability_policy(),
                query.queue_observation_policy(),
            )?;
            validate_pc_minimals_scenario_shape(query)?;
            return Ok(validated);
        }
        PcResultProjection::PathFamilyV2(_) => {
            validate_pc_path_common_request_contract(
                query.objective(),
                query.count_policy(),
                query.solution_probability_policy(),
                query.queue_observation_policy(),
                query.execution_policy(),
            )?;
            if query.allowed_colored_solution_identities().is_some() {
                return Err("pc path does not accept caller-selected solution identities");
            }
            return Ok(validated);
        }
        PcResultProjection::ScoreSummaryV2(origin) => {
            validate_pc_score_scenario_request_contract(query, origin)?;
            return Ok(validated);
        }
        PcResultProjection::ScorePortfolioV2(_) => {
            validate_pc_score_minimals_scenario_request_contract(query)?;
            return Ok(validated);
        }
        PcResultProjection::SaveGroupsV2(origin) => {
            validate_pc_save_scenario_request_contract(query, origin, "save-groups")?;
            return Ok(validated);
        }
        PcResultProjection::BestSaveV2(origin) => {
            validate_pc_save_scenario_request_contract(query, origin, "best-save")?;
            return Ok(validated);
        }
        PcResultProjection::TilingFamilyV1(origin) => {
            validate_pc_tiling_scenario_request_contract(query, origin)?;
            return Ok(validated);
        }
        PcResultProjection::AllSpinSolution(profile)
        | PcResultProjection::AllSpinPreservationChance(profile) => profile,
    };

    let board = query.initial_board();
    if board.width() != 10 || !(1..=6).contains(&board.visible_height()) {
        return Err("pc All-Spin initial field must be a 10-column board with height in 1..=6");
    }
    if board.occupied_mask() == 0 {
        return Err("pc All-Spin scenario requires a nonempty initial field");
    }
    let visible_bits = u32::from(board.width()) * u32::from(board.visible_height());
    let visible_mask = if visible_bits == u64::BITS {
        u64::MAX
    } else {
        (1_u64 << visible_bits) - 1
    };
    if board.occupied_mask() & !visible_mask != 0 {
        return Err("pc All-Spin initial field contains cells above its declared height");
    }

    validate_queue_projection(query.remaining_queue(), projection)?;
    if query.hold_state() != HoldSlot::Empty {
        return Err("pc All-Spin does not accept an occupied initial hold slot");
    }
    let piece_count = query.piece_window().max_pieces();
    if piece_count == 0 || query.exact_pieces() != Some(piece_count) {
        return Err("pc All-Spin initial-field pieces must be positive and exact");
    }
    if query.min_remaining_queue() != 0 {
        return Err("pc All-Spin does not accept a remaining-queue target override");
    }
    if query.count_policy() != PcCountPolicy::CountUnique {
        return Err("pc All-Spin requires unique solution counting");
    }
    if query.retained_trace_limit() != 1 {
        return Err("pc All-Spin does not accept a retained-trace override");
    }
    if query.requires_180() {
        return Err("pc All-Spin does not accept a scenario 180-requirement override");
    }
    if query.allowed_colored_solution_identities().is_some() {
        return Err("pc All-Spin does not accept caller-selected solution identities");
    }
    validate_common_request_contract(
        query.objective(),
        profile,
        query.solution_probability_policy(),
        query.queue_observation_policy(),
    )?;
    validate_supply_window(
        query.remaining_queue(),
        query.supply_window_size(),
        piece_count,
        query.allow_hold(),
    )?;
    Ok(validated)
}

fn validate_queue_projection(
    queue: &PcQueueInput,
    projection: PcResultProjection,
) -> Result<(), &'static str> {
    match (projection, queue) {
        (PcResultProjection::Standard, _) => Ok(()),
        (PcResultProjection::ChanceProbabilityV2(_), _) => Ok(()),
        (PcResultProjection::MinimumCoverV2(_), _) => Ok(()),
        (PcResultProjection::PathFamilyV2(_), _) => Ok(()),
        (PcResultProjection::ScoreSummaryV2(_), _) => Ok(()),
        (PcResultProjection::ScorePortfolioV2(_), _) => Ok(()),
        (PcResultProjection::SaveGroupsV2(_), _) => Ok(()),
        (PcResultProjection::BestSaveV2(_), _) => Ok(()),
        (PcResultProjection::TilingFamilyV1(_), _) => Ok(()),
        (PcResultProjection::AllSpinSolution(_), PcQueueInput::FixedSequence(_)) => Ok(()),
        (
            PcResultProjection::AllSpinPreservationChance(_),
            PcQueueInput::BagAlignedPattern(_)
            | PcQueueInput::PatternExpression(_)
            | PcQueueInput::Standard7Bag,
        ) => Ok(()),
        (PcResultProjection::AllSpinSolution(_), _) => {
            Err("pc All-Spin exact solution projection requires one fixed queue")
        }
        (PcResultProjection::AllSpinPreservationChance(_), _) => {
            Err("pc All-Spin preservation chance projection requires a queue pattern")
        }
    }
}

fn validate_pc_path_common_request_contract(
    objective: ObjectivePolicy,
    count_policy: PcCountPolicy,
    probability_policy: PcSolutionProbabilityPolicy,
    observation_policy: QueueObservationPolicy,
    execution_policy: &PcExecutionPolicy,
) -> Result<(), &'static str> {
    if objective != ObjectivePolicy::all() || objective.score().requested() {
        return Err("pc path requires the all non-scoring objective without constraints");
    }
    if count_policy != PcCountPolicy::CountAll {
        return Err("pc path requires all build variants");
    }
    if probability_policy != PcSolutionProbabilityPolicy::Omit {
        return Err("pc path does not accept per-solution probability calculation");
    }
    if observation_policy != QueueObservationPolicy::FullQueueOracle {
        return Err("pc path requires full-queue oracle knowledge");
    }
    if execution_policy.max_memory_mib().is_some()
        || execution_policy.tablebase_requested()
        || execution_policy.precompute_build_dependencies()
    {
        return Err("pc path does not accept incomplete execution overrides");
    }
    Ok(())
}

fn validate_pc_chance_opening_request_contract(
    query: &OpeningPcSearchQuery,
) -> Result<(), &'static str> {
    if query.execution_policy().max_memory_mib().is_some() {
        return Err(
            "pc chance does not support an explicit memory cap until transient proof memory is accounted",
        );
    }
    validate_pc_chance_common_request_contract(
        query.objective(),
        query.solution_probability_policy(),
        query.queue_observation_policy(),
    )
}

fn validate_pc_chance_scenario_request_contract(
    query: &PcScenarioQuery,
) -> Result<(), &'static str> {
    if query.execution_policy().max_memory_mib().is_some() {
        return Err(
            "pc chance does not support an explicit memory cap until transient proof memory is accounted",
        );
    }
    validate_pc_chance_common_request_contract(
        query.objective(),
        query.solution_probability_policy(),
        query.queue_observation_policy(),
    )?;
    if query.completion_goal().as_str() != "clear-to-empty" {
        return Err("pc chance requires the clear-to-empty completion goal");
    }
    if query.count_policy() != PcCountPolicy::CountUnique {
        return Err("pc chance requires unique solution counting");
    }
    if query.allowed_colored_solution_identities().is_some() {
        return Err("pc chance does not accept caller-selected solution identities");
    }
    Ok(())
}

fn validate_pc_chance_common_request_contract(
    objective: ObjectivePolicy,
    probability_policy: PcSolutionProbabilityPolicy,
    observation_policy: QueueObservationPolicy,
) -> Result<(), &'static str> {
    if objective != ObjectivePolicy::unique() {
        return Err("pc chance requires the unique non-scoring objective without constraints");
    }
    if probability_policy != PcSolutionProbabilityPolicy::Omit {
        return Err("pc chance does not accept per-solution probability calculation");
    }
    if observation_policy != QueueObservationPolicy::FullQueueOracle {
        return Err("pc chance requires full-queue oracle knowledge");
    }
    Ok(())
}

fn validate_pc_save_opening_request_contract(
    query: &OpeningPcSearchQuery,
    origin: PcSaveIngressOrigin,
    expected_mode: &'static str,
) -> Result<(), &'static str> {
    validate_pc_save_origin(origin, expected_mode)?;
    if query.execution_policy().max_memory_mib().is_some() {
        return Err(
            "pc saves does not support an explicit memory cap until terminal-supply proof memory is accounted",
        );
    }
    if !matches!(query.target().lines(), 2 | 4 | 6) {
        return Err("pc saves opening target must be exactly 2, 4, or 6 lines");
    }
    validate_pc_save_queue_contract(query.queue())?;
    validate_pc_save_common_request_contract(
        query.objective(),
        query.solution_probability_policy(),
        query.queue_observation_policy(),
    )
}

fn validate_pc_save_scenario_request_contract(
    query: &PcScenarioQuery,
    origin: PcSaveIngressOrigin,
    expected_mode: &'static str,
) -> Result<(), &'static str> {
    validate_pc_save_origin(origin, expected_mode)?;
    if query.execution_policy().max_memory_mib().is_some() {
        return Err(
            "pc saves does not support an explicit memory cap until terminal-supply proof memory is accounted",
        );
    }
    validate_pc_save_queue_contract(query.remaining_queue())?;
    validate_pc_save_common_request_contract(
        query.objective(),
        query.solution_probability_policy(),
        query.queue_observation_policy(),
    )?;
    if query.completion_goal().as_str() != "clear-to-empty" {
        return Err("pc saves requires the clear-to-empty completion goal");
    }
    if query.count_policy() != PcCountPolicy::CountAll {
        return Err("pc saves requires all solution counting");
    }
    if query.allowed_colored_solution_identities().is_some() {
        return Err("pc saves does not accept caller-selected solution identities");
    }
    if query.min_remaining_queue() != 0 {
        return Err("pc saves does not accept a minimum remaining-queue override");
    }
    if query.retained_trace_limit() != 1 {
        return Err("pc saves requires its fixed single retained-trace limit");
    }
    if query.requires_180() {
        return Err("pc saves does not accept a scenario 180-requirement override");
    }
    if !query.allow_hold() && query.hold_state().piece().is_some() {
        return Err("pc saves does not accept an occupied hold slot when hold is disabled");
    }
    Ok(())
}

fn validate_pc_save_origin(
    origin: PcSaveIngressOrigin,
    expected_mode: &'static str,
) -> Result<(), &'static str> {
    if origin.mode().as_str() != expected_mode {
        return Err("pc saves ingress origin does not match the selected result projection");
    }
    Ok(())
}

fn validate_pc_save_queue_contract(queue: &PcQueueInput) -> Result<(), &'static str> {
    match queue {
        PcQueueInput::Standard7Bag
        | PcQueueInput::BagAlignedPattern(_)
        | PcQueueInput::PatternExpression(_) => Ok(()),
        PcQueueInput::FixedSequence(_) => {
            Err("pc saves requires bag provenance; a fixed queue has no bag boundary authority")
        }
        PcQueueInput::Observed(_) => {
            Err("pc saves requires fixed bag-boundary provenance; observed queues are ambiguous")
        }
    }
}

fn validate_pc_save_common_request_contract(
    objective: ObjectivePolicy,
    probability_policy: PcSolutionProbabilityPolicy,
    observation_policy: QueueObservationPolicy,
) -> Result<(), &'static str> {
    if objective != ObjectivePolicy::all() {
        return Err("pc saves requires the all non-scoring objective without constraints");
    }
    if probability_policy != PcSolutionProbabilityPolicy::Omit {
        return Err(
            "pc saves computes group probabilities from its own exact pattern universe and does not accept per-solution probabilities",
        );
    }
    if observation_policy != QueueObservationPolicy::FullQueueOracle {
        return Err("pc saves requires full-queue oracle knowledge");
    }
    Ok(())
}

pub(crate) fn validate_pc_tiling_opening_request_contract(
    query: &OpeningPcSearchQuery,
    origin: PcTilingIngressOrigin,
) -> Result<(), &'static str> {
    validate_pc_tiling_origin(origin)?;
    validate_pc_tiling_execution_policy(query.execution_policy())?;
    if !matches!(query.target().lines(), 2 | 4 | 6) {
        return Err("pc tiling opening target must be exactly 2, 4, or 6 lines");
    }
    if query.verified_kick_profile().is_some() {
        return Err("pc tiling does not accept an imported kick-table profile");
    }
    validate_pc_tiling_common_request_contract(
        query.objective(),
        query.solution_probability_policy(),
        query.queue_observation_policy(),
    )
}

pub(crate) fn validate_pc_tiling_scenario_request_contract(
    query: &PcScenarioQuery,
    origin: PcTilingIngressOrigin,
) -> Result<(), &'static str> {
    validate_pc_tiling_origin(origin)?;
    validate_pc_tiling_execution_policy(query.execution_policy())?;
    if query.verified_kick_profile().is_some() {
        return Err("pc tiling does not accept an imported kick-table profile");
    }
    validate_pc_tiling_common_request_contract(
        query.objective(),
        query.solution_probability_policy(),
        query.queue_observation_policy(),
    )?;
    if query.completion_goal().as_str() != "clear-to-empty" {
        return Err("pc tiling requires the clear-to-empty completion goal");
    }
    if query.count_policy() != PcCountPolicy::CountUnique {
        return Err("pc tiling requires normalized unique geometry-family counting");
    }
    if query.allowed_colored_solution_identities().is_some() {
        return Err("pc tiling does not accept caller-selected solution identities");
    }
    if query.min_remaining_queue() != 0 {
        return Err("pc tiling does not accept a minimum remaining-queue override");
    }
    if query.retained_trace_limit() != 1 {
        return Err("pc tiling requires its fixed unused retained-trace limit");
    }
    if query.requires_180() {
        return Err("pc tiling does not accept a scenario 180-requirement override");
    }
    if !query.allow_hold() && query.hold_state().piece().is_some() {
        return Err("pc tiling does not accept an occupied hold slot when hold is disabled");
    }

    let board = query.initial_board();
    if board.width() != 10 || !(1..=6).contains(&board.visible_height()) {
        return Err("pc tiling scenario requires a 10-column board with height in 1..=6");
    }
    let visible_bits = u32::from(board.width()) * u32::from(board.visible_height());
    let visible_mask = (1_u64 << visible_bits) - 1;
    if board.occupied_mask() & !visible_mask != 0 {
        return Err("pc tiling scenario contains cells above its declared height");
    }
    let normalized_board = board.after_initial_line_clear();
    let empty_cells = visible_bits - (normalized_board.occupied_mask() & visible_mask).count_ones();
    if empty_cells == 0 || !empty_cells.is_multiple_of(4) {
        return Err("pc tiling scenario empty-cell count must be a positive multiple of four");
    }
    let required_pieces = empty_cells as usize / 4;
    if query.piece_window().max_pieces() != required_pieces
        || query.exact_pieces() != Some(required_pieces)
    {
        return Err("pc tiling scenario piece window must exactly cover its empty cells");
    }
    validate_supply_window(
        query.remaining_queue(),
        query.supply_window_size(),
        required_pieces,
        query.allow_hold(),
    )
}

fn validate_pc_tiling_origin(origin: PcTilingIngressOrigin) -> Result<(), &'static str> {
    match origin {
        PcTilingIngressOrigin::CanonicalPcTiling => Ok(()),
    }
}

fn validate_pc_tiling_execution_policy(policy: &PcExecutionPolicy) -> Result<(), &'static str> {
    if policy.tablebase_requested() {
        return Err("pc tiling does not accept a tablebase request");
    }
    if policy.precompute_build_dependencies() {
        return Err("pc tiling does not accept build-dependency precomputation");
    }
    Ok(())
}

fn validate_pc_tiling_common_request_contract(
    objective: ObjectivePolicy,
    probability_policy: PcSolutionProbabilityPolicy,
    observation_policy: QueueObservationPolicy,
) -> Result<(), &'static str> {
    if objective != ObjectivePolicy::tiling() {
        return Err("pc tiling requires the geometry-only tiling objective without constraints");
    }
    if probability_policy != PcSolutionProbabilityPolicy::Omit {
        return Err("pc tiling does not accept per-solution probability calculation");
    }
    if observation_policy != QueueObservationPolicy::FullQueueOracle {
        return Err("pc tiling requires full-queue oracle knowledge");
    }
    Ok(())
}

pub(crate) fn validate_pc_score_opening_request_contract(
    query: &OpeningPcSearchQuery,
    origin: PcScoreIngressOrigin,
) -> Result<(), &'static str> {
    validate_pc_score_opening_request_contract_with_origin(query, Some(origin))
}

pub(crate) fn validate_pc_score_minimals_opening_request_contract(
    query: &OpeningPcSearchQuery,
) -> Result<(), &'static str> {
    validate_pc_score_opening_request_contract_with_origin(query, None)
}

fn validate_pc_score_opening_request_contract_with_origin(
    query: &OpeningPcSearchQuery,
    origin: Option<PcScoreIngressOrigin>,
) -> Result<(), &'static str> {
    if origin.is_some_and(PcScoreIngressOrigin::is_score_finder) {
        return Err("pc score-finder requires an explicit initial-field scenario");
    }
    validate_pc_score_execution_policy(query.execution_policy())?;
    if !matches!(query.target().lines(), 2 | 4 | 6) {
        return Err("pc score opening target must be exactly 2, 4, or 6 lines");
    }
    if query.verified_kick_profile().is_some() {
        return Err("pc score does not accept an imported kick-table profile");
    }
    validate_pc_score_queue_contract(query.queue())?;
    let geometry_pieces = usize::from(query.target().lines()) * 10 / 4;
    validate_pc_score_supply_window(
        query.queue(),
        query.supply_window_size(),
        geometry_pieces,
        query.hold_policy().is_enabled(),
        query.hold_policy().initial_piece().is_some(),
    )?;
    validate_pc_score_common_request_contract(
        query.objective(),
        query.solution_probability_policy(),
        query.queue_observation_policy(),
        origin,
    )
}

pub(crate) fn validate_pc_score_scenario_request_contract(
    query: &PcScenarioQuery,
    origin: PcScoreIngressOrigin,
) -> Result<(), &'static str> {
    validate_pc_score_scenario_request_contract_with_origin(query, Some(origin))
}

pub(crate) fn validate_pc_score_minimals_scenario_request_contract(
    query: &PcScenarioQuery,
) -> Result<(), &'static str> {
    validate_pc_score_scenario_request_contract_with_origin(query, None)
}

fn validate_pc_score_scenario_request_contract_with_origin(
    query: &PcScenarioQuery,
    origin: Option<PcScoreIngressOrigin>,
) -> Result<(), &'static str> {
    validate_pc_score_execution_policy(query.execution_policy())?;
    if query.verified_kick_profile().is_some() {
        return Err("pc score does not accept an imported kick-table profile");
    }
    validate_pc_score_queue_contract(query.remaining_queue())?;
    if origin.is_some_and(PcScoreIngressOrigin::is_score_finder) {
        if !matches!(
            query.remaining_queue(),
            PcQueueInput::FixedSequence(sequence) if !sequence.is_empty()
        ) {
            return Err("pc score-finder requires exactly one fixed queue");
        }
        let score = query.objective().score();
        if score.profile()
            != clearra_objectives::policy::score_objective_policy::ScoreProfileSelection::JstrisUltra
            || score.spin_profile() != SpinProfileSelection::TSpins
            || score.initial_b2b() > 1
        {
            return Err(
                "pc score-finder requires jstris-ultra, t-spins, and boolean initial B2B",
            );
        }
    }
    validate_pc_score_common_request_contract(
        query.objective(),
        query.solution_probability_policy(),
        query.queue_observation_policy(),
        origin,
    )?;
    if query.completion_goal().as_str() != "clear-to-empty" {
        return Err("pc score requires the clear-to-empty completion goal");
    }
    if query.count_policy() != PcCountPolicy::CountAll {
        return Err("pc score requires all solution counting");
    }
    if query.allowed_colored_solution_identities().is_some() {
        return Err("pc score does not accept caller-selected solution identities");
    }
    if query.min_remaining_queue() != 0 {
        return Err("pc score does not accept a minimum remaining-queue override");
    }
    if query.retained_trace_limit() != 1 {
        return Err("pc score requires its fixed single retained-trace limit");
    }
    if query.requires_180() {
        return Err("pc score does not accept a scenario 180-requirement override");
    }
    if !query.allow_hold() && query.hold_state().piece().is_some() {
        return Err("pc score does not accept an occupied hold slot when hold is disabled");
    }

    let board = query.initial_board();
    if board.width() != 10 || !(1..=6).contains(&board.visible_height()) {
        return Err("pc score scenario requires a 10-column board with height in 1..=6");
    }
    let visible_bits = u32::from(board.width()) * u32::from(board.visible_height());
    let visible_mask = (1_u64 << visible_bits) - 1;
    if board.occupied_mask() & !visible_mask != 0 {
        return Err("pc score scenario contains cells above its declared height");
    }
    let normalized_board = board.after_initial_line_clear();
    let empty_cells = visible_bits - (normalized_board.occupied_mask() & visible_mask).count_ones();
    if empty_cells == 0 || !empty_cells.is_multiple_of(4) {
        return Err("pc score scenario empty-cell count must be a positive multiple of four");
    }
    let required_pieces = empty_cells as usize / 4;
    if required_pieces > PC_SCORE_MAX_SOURCE_PIECES - 1
        || query.piece_window().max_pieces() != required_pieces
        || query.exact_pieces() != Some(required_pieces)
    {
        return Err("pc score scenario piece window must exactly cover its bounded empty cells");
    }
    validate_pc_score_supply_window(
        query.remaining_queue(),
        query.supply_window_size(),
        required_pieces,
        query.allow_hold(),
        query.allow_hold() && query.hold_state().piece().is_some(),
    )?;
    Ok(())
}

fn validate_pc_score_execution_policy(policy: &PcExecutionPolicy) -> Result<(), &'static str> {
    let baseline = PcExecutionPolicy::mvp_default();
    if policy.requested_backend() != RequestedSearchBackend::Cpu
        || matches!(policy.worker_policy(), WorkerPolicy::Fixed(0))
        || policy.gpu_warmup()
        || policy.tablebase_requested()
        || policy.precompute_build_dependencies()
        || policy.allow_backend_fallback()
        || policy.gpu_device() != baseline.gpu_device()
        || policy.deterministic() != baseline.deterministic()
        || policy.max_memory_mib().is_some()
        || policy.max_patterns() != PC_SCORE_MAX_PATTERNS
        || policy.max_nodes() != baseline.max_nodes()
        || policy.max_frontier_states() != baseline.max_frontier_states()
        || policy.max_candidates() != baseline.max_candidates()
    {
        return Err("pc score requires its fixed-cap CPU execution policy");
    }
    Ok(())
}

fn validate_pc_score_queue_contract(queue: &PcQueueInput) -> Result<(), &'static str> {
    match queue {
        PcQueueInput::FixedSequence(sequence) => {
            if sequence.len() > PC_SCORE_MAX_SOURCE_PIECES {
                return Err("pc score accepts at most 16 fixed source pieces");
            }
            if sequence
                .checked_retained_capacity_bytes()
                .is_none_or(|bytes| bytes > PC_SCORE_MAX_QUERY_QUEUE_RETAINED_BYTES)
            {
                return Err("pc score fixed queue retains more than the product memory envelope");
            }
            Ok(())
        }
        PcQueueInput::PatternExpression(expression)
            if expression.source().len() <= PC_SCORE_MAX_PATTERN_BYTES
                && !expression.source().contains(';')
                && expression.sequence_len() <= PC_SCORE_MAX_SOURCE_PIECES
                && expression.pattern_count() <= PC_SCORE_MAX_PATTERNS
                && expression
                    .checked_retained_capacity_bytes()
                    .is_some_and(|bytes| bytes <= PC_SCORE_MAX_QUERY_QUEUE_RETAINED_BYTES)
                && expression.explicit_sequences().is_none_or(|sequences| {
                    sequences.len() <= PC_SCORE_MAX_EXPLICIT_PATTERNS
                        && sequences
                            .iter()
                            .all(|sequence| sequence.len() <= PC_SCORE_MAX_SOURCE_PIECES)
                }) =>
        {
            Ok(())
        }
        PcQueueInput::Standard7Bag => Ok(()),
        PcQueueInput::PatternExpression(_) => {
            Err("pc score requires one bounded factorized queue expression")
        }
        PcQueueInput::BagAlignedPattern(_) | PcQueueInput::Observed(_) => {
            Err("pc score requires a fixed queue, one pattern expression, or the standard 7-bag")
        }
    }
}

fn validate_pc_score_supply_window(
    queue: &PcQueueInput,
    supply_window: Option<SupplyWindowSize>,
    geometry_pieces: usize,
    hold_enabled: bool,
    initial_hold_occupied: bool,
) -> Result<(), &'static str> {
    let initial_hold_pieces = usize::from(hold_enabled && initial_hold_occupied);
    let required_source_pieces = geometry_pieces.saturating_sub(initial_hold_pieces);
    let automatic_source_pieces = geometry_pieces
        .saturating_add(usize::from(hold_enabled))
        .saturating_sub(initial_hold_pieces);
    if automatic_source_pieces > PC_SCORE_MAX_SOURCE_PIECES {
        return Err("pc score automatic source window exceeds 16 pieces");
    }

    let finite_queue_len = match queue {
        PcQueueInput::FixedSequence(sequence) => Some(sequence.len()),
        PcQueueInput::PatternExpression(expression) => Some(expression.sequence_len()),
        PcQueueInput::Standard7Bag
        | PcQueueInput::BagAlignedPattern(_)
        | PcQueueInput::Observed(_) => None,
    };
    if finite_queue_len.is_some_and(|length| length < required_source_pieces) {
        return Err("pc score finite queue is shorter than the required source window");
    }
    if let Some(source_pieces) = supply_window.map(SupplyWindowSize::source_pieces) {
        if source_pieces < required_source_pieces
            || source_pieces > automatic_source_pieces
            || finite_queue_len.is_some_and(|length| source_pieces > length)
        {
            return Err(
                "pc score source window must stay within the required automatic search window",
            );
        }
    }
    Ok(())
}

fn validate_pc_score_common_request_contract(
    objective: ObjectivePolicy,
    probability_policy: PcSolutionProbabilityPolicy,
    observation_policy: QueueObservationPolicy,
    origin: Option<PcScoreIngressOrigin>,
) -> Result<(), &'static str> {
    let score = objective.score();
    let expected_objective = if origin.is_some() {
        ObjectivePolicy::all().with_score_policy(score)
    } else {
        ObjectivePolicy::minimum_cover().with_score_policy(score)
    };
    if score.mode() != ScoreObjectiveMode::Summary || objective != expected_objective {
        return Err(if origin.is_some() {
            "pc score requires the all score-summary objective without constraints"
        } else {
            "pc score-minimals requires the score-aware minimum-cover objective without constraints"
        });
    }
    if matches!(
        origin,
        Some(
            PcScoreIngressOrigin::CompatibilityScore | PcScoreIngressOrigin::CanonicalPcScoreFinder
        )
    ) && score.profile()
        != clearra_objectives::policy::score_objective_policy::ScoreProfileSelection::JstrisUltra
    {
        return Err(
            if origin.is_some_and(PcScoreIngressOrigin::is_score_finder) {
                "pc score-finder requires the fixed jstris-ultra profile"
            } else {
                "pc score compatibility ingress requires the fixed jstris-ultra profile"
            },
        );
    }
    if probability_policy != PcSolutionProbabilityPolicy::Omit {
        return Err("pc score does not accept per-solution probability calculation");
    }
    if observation_policy != QueueObservationPolicy::FullQueueOracle {
        return Err("pc score requires full-queue oracle knowledge");
    }
    Ok(())
}

fn validate_common_request_contract(
    objective: ObjectivePolicy,
    profile: SpinProfileSelection,
    probability_policy: PcSolutionProbabilityPolicy,
    observation_policy: QueueObservationPolicy,
) -> Result<(), &'static str> {
    if objective.kind() != ObjectiveKind::Unique {
        return Err("pc All-Spin requires the unique non-scoring objective");
    }
    if objective.score().requested() {
        return Err("pc All-Spin does not accept score-selected semantics");
    }
    let constraints = objective.execution_constraints();
    if !constraints.preserves_back_to_back() {
        return Err("pc All-Spin requires existential B2B preservation");
    }
    if constraints.spin_profile() != profile {
        return Err("pc All-Spin projection profile does not match the B2B constraint profile");
    }
    if objective != ObjectivePolicy::unique().with_back_to_back_preservation(profile) {
        return Err("pc All-Spin does not accept an objective policy override");
    }
    if probability_policy != PcSolutionProbabilityPolicy::Omit {
        return Err("pc All-Spin does not accept per-solution probability calculation");
    }
    if observation_policy != QueueObservationPolicy::FullQueueOracle {
        return Err("pc All-Spin requires full-queue oracle knowledge");
    }
    Ok(())
}

fn validate_supply_window(
    queue: &PcQueueInput,
    supplied: Option<SupplyWindowSize>,
    required_pieces: usize,
    hold_enabled: bool,
) -> Result<(), &'static str> {
    let expected = matches!(queue, PcQueueInput::Standard7Bag).then(|| {
        SupplyWindowSize::new(7.min(required_pieces.saturating_add(usize::from(hold_enabled))))
    });
    if supplied != expected {
        return Err("pc All-Spin does not accept a source-piece window override");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        objective::{
            objective_kind::ObjectiveKind, tie_policy::TiePolicy, trace_policy::TracePolicy,
        },
        pc::pc_target::PcTarget,
        piece::piece_kind::PieceKind,
        solution::StandardBoard64ColoredTilingIdentity,
    };
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
    };
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcCountPolicy, PcHoldPolicy, PcQueueInput, PcScenarioBoard,
        PcScenarioQuery, PcSolutionProbabilityPolicy, PieceWindow, SupplyWindowSize,
    };
    use clearra_supply::{
        queue::{fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression},
        QueueObservationPolicy,
    };

    use super::{
        validate_opening_pc_result_projection, validate_pc_score_common_request_contract,
        validate_scenario_pc_result_projection, PcResultProjection,
    };

    #[test]
    fn score_minimals_owns_the_score_only_minimum_cover_objective_domain() {
        let score_minimals = ObjectivePolicy::minimum_cover().with_score_summary();
        assert!(validate_pc_score_common_request_contract(
            score_minimals,
            PcSolutionProbabilityPolicy::Omit,
            QueueObservationPolicy::FullQueueOracle,
            None,
        )
        .is_ok());

        let summary_error = validate_pc_score_common_request_contract(
            score_minimals,
            PcSolutionProbabilityPolicy::Omit,
            QueueObservationPolicy::FullQueueOracle,
            Some(super::PcScoreIngressOrigin::CanonicalPcScore),
        )
        .expect_err("minimum-cover must not masquerade as the score summary product");
        assert!(summary_error.contains("all score-summary"));

        let constrained_error = validate_pc_score_common_request_contract(
            score_minimals.with_back_to_back_preservation(SpinProfileSelection::TSpins),
            PcSolutionProbabilityPolicy::Omit,
            QueueObservationPolicy::FullQueueOracle,
            None,
        )
        .expect_err("score-minimals must not add a non-score membership constraint");
        assert!(constrained_error.contains("minimum-cover objective without constraints"));
    }

    fn b2b_objective(profile: SpinProfileSelection) -> ObjectivePolicy {
        ObjectivePolicy::unique().with_back_to_back_preservation(profile)
    }

    fn fixed_queue() -> PcQueueInput {
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
        ]))
    }

    fn pattern_queue() -> PcQueueInput {
        PcQueueInput::pattern_expression(
            QueuePatternExpression::parse("[TI]!", 2).expect("two-pattern queue"),
        )
    }

    fn valid_scenario(queue: PcQueueInput, profile: SpinProfileSelection) -> PcScenarioQuery {
        PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 1),
            queue,
            PieceWindow::new(5),
        )
        .with_exact_pieces(Some(5))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_retained_trace_limit(1)
        .with_objective(b2b_objective(profile))
    }

    #[test]
    fn pc_chance_requires_full_queue_oracle_for_opening_and_scenario() {
        let projection = PcResultProjection::ChanceProbabilityV2(
            crate::PcChanceIngressOrigin::CanonicalPcChance,
        );
        let opening = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(pattern_queue())
            .with_objective(ObjectivePolicy::unique())
            .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven);
        assert_eq!(
            validate_opening_pc_result_projection(&opening, projection),
            Err("pc chance requires full-queue oracle knowledge")
        );

        let scenario = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(1, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_objective(ObjectivePolicy::unique())
        .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven);
        assert_eq!(
            validate_scenario_pc_result_projection(&scenario, projection),
            Err("pc chance requires full-queue oracle knowledge")
        );
    }

    #[test]
    fn opening_request_projection_validation_is_fieldwise_and_fail_closed() {
        let profile = SpinProfileSelection::AllSpinPlus;
        let exact = PcResultProjection::AllSpinSolution(profile);
        let chance = PcResultProjection::AllSpinPreservationChance(profile);
        let valid_exact = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(fixed_queue())
            .with_objective(b2b_objective(profile));
        assert_eq!(
            validate_opening_pc_result_projection(&valid_exact, exact)
                .expect("valid exact request")
                .projection(),
            exact
        );
        assert!(validate_opening_pc_result_projection(
            &OpeningPcSearchQuery::new(PcTarget::four_lines())
                .with_queue(pattern_queue())
                .with_objective(b2b_objective(profile)),
            chance,
        )
        .is_ok());
        assert!(validate_opening_pc_result_projection(
            &OpeningPcSearchQuery::new(PcTarget::two_lines())
                .with_queue(PcQueueInput::standard_7_bag())
                .with_supply_window_size(SupplyWindowSize::new(6))
                .with_objective(b2b_objective(profile)),
            chance,
        )
        .is_ok());

        let invalid = vec![
            (
                OpeningPcSearchQuery::new(PcTarget::new(8).expect("8L target"))
                    .with_queue(fixed_queue())
                    .with_objective(b2b_objective(profile)),
                exact,
                "2, 4, or 6",
            ),
            (
                OpeningPcSearchQuery::new(PcTarget::two_lines())
                    .with_queue(pattern_queue())
                    .with_objective(b2b_objective(profile)),
                exact,
                "fixed queue",
            ),
            (valid_exact.clone(), chance, "queue pattern"),
            (
                valid_exact
                    .clone()
                    .with_hold_policy(PcHoldPolicy::EnabledWithPiece(PieceKind::T)),
                exact,
                "occupied initial hold",
            ),
            (
                valid_exact
                    .clone()
                    .with_objective(ObjectivePolicy::all().with_back_to_back_preservation(profile)),
                exact,
                "unique non-scoring",
            ),
            (
                valid_exact.clone().with_objective(
                    ObjectivePolicy::unique()
                        .with_score_summary()
                        .with_back_to_back_preservation(profile),
                ),
                exact,
                "score-selected",
            ),
            (
                valid_exact
                    .clone()
                    .with_objective(ObjectivePolicy::unique()),
                exact,
                "requires existential B2B",
            ),
            (
                valid_exact
                    .clone()
                    .with_objective(b2b_objective(SpinProfileSelection::AllMiniPlus)),
                exact,
                "does not match",
            ),
            (
                valid_exact.clone().with_objective(
                    ObjectivePolicy::new(
                        ObjectiveKind::Unique,
                        TiePolicy::LowestCandidateId,
                        TracePolicy::Keep,
                    )
                    .with_back_to_back_preservation(profile),
                ),
                exact,
                "objective policy override",
            ),
            (
                valid_exact
                    .clone()
                    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include),
                exact,
                "per-solution probability",
            ),
            (
                valid_exact
                    .clone()
                    .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven),
                exact,
                "full-queue oracle",
            ),
            (
                valid_exact
                    .clone()
                    .with_supply_window_size(SupplyWindowSize::new(5)),
                exact,
                "source-piece window",
            ),
        ];
        for (query, projection, expected) in invalid {
            let error = validate_opening_pc_result_projection(&query, projection)
                .expect_err("invalid opening All-Spin request");
            assert!(
                error.contains(expected),
                "{error:?} did not contain {expected:?}"
            );
        }
    }

    #[test]
    fn scenario_request_projection_validation_keeps_initial_field_and_target_domains_distinct() {
        let profile = SpinProfileSelection::AllSpinPlus;
        let exact = PcResultProjection::AllSpinSolution(profile);
        let chance = PcResultProjection::AllSpinPreservationChance(profile);
        let valid_exact = valid_scenario(fixed_queue(), profile);
        assert!(validate_scenario_pc_result_projection(&valid_exact, exact).is_ok());
        assert!(validate_scenario_pc_result_projection(
            &valid_exact.clone().with_allow_hold(false),
            exact,
        )
        .is_ok());
        assert!(validate_scenario_pc_result_projection(
            &valid_scenario(PcQueueInput::standard_7_bag(), profile)
                .with_supply_window_size(SupplyWindowSize::new(6)),
            chance,
        )
        .is_ok());

        let selected_identity = StandardBoard64ColoredTilingIdentity::from_piece_masks(0, [0; 7])
            .expect("empty colored identity");
        let invalid = vec![
            (
                valid_exact
                    .clone()
                    .with_initial_board(PcScenarioBoard::new(9, 2, 1)),
                exact,
                "10-column",
            ),
            (
                valid_exact
                    .clone()
                    .with_initial_board(PcScenarioBoard::standard_10(7, 1)),
                exact,
                "height in 1..=6",
            ),
            (
                valid_exact
                    .clone()
                    .with_initial_board(PcScenarioBoard::standard_10(2, 0)),
                exact,
                "nonempty initial field",
            ),
            (
                valid_exact
                    .clone()
                    .with_initial_board(PcScenarioBoard::standard_10(1, 1 << 10)),
                exact,
                "above its declared height",
            ),
            (
                valid_scenario(pattern_queue(), profile),
                exact,
                "fixed queue",
            ),
            (
                valid_exact.clone().with_hold_piece(Some(PieceKind::T)),
                exact,
                "occupied initial hold",
            ),
            (
                valid_exact.clone().with_exact_pieces(Some(4)),
                exact,
                "positive and exact",
            ),
            (
                valid_exact.clone().with_min_remaining_queue(1),
                exact,
                "remaining-queue target",
            ),
            (
                valid_exact
                    .clone()
                    .with_count_policy(PcCountPolicy::CountAll)
                    .with_objective(b2b_objective(profile)),
                exact,
                "unique solution counting",
            ),
            (
                valid_exact.clone().with_retained_trace_limit(2),
                exact,
                "retained-trace",
            ),
            (
                valid_exact.clone().with_requires_180(true),
                exact,
                "180-requirement",
            ),
            (
                valid_exact
                    .clone()
                    .with_allowed_colored_solution_identities([selected_identity]),
                exact,
                "caller-selected solution identities",
            ),
            (
                valid_exact
                    .clone()
                    .with_supply_window_size(SupplyWindowSize::new(5)),
                exact,
                "source-piece window",
            ),
        ];
        for (query, projection, expected) in invalid {
            let error = validate_scenario_pc_result_projection(&query, projection)
                .expect_err("invalid scenario All-Spin request");
            assert!(
                error.contains(expected),
                "{error:?} did not contain {expected:?}"
            );
        }
    }
}
