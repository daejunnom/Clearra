// SRP rationale: this module has one change reason: exact PC-chance coverage evidence construction and validation.
use clearra_core_domain::{
    field::occupancy_field::OccupancyField, objective::objective_kind::ObjectiveKind,
    pc::pc_target::PcTarget, piece::piece_kind::PieceKind,
    probability::probability_value::ProbabilityValue,
    solution::StandardBoard64ColoredTilingIdentity,
};
use clearra_core_ffi::rules::{kick_profile_code, rule_profile_code};
use clearra_coverage::{
    pattern::{
        pattern_bitset::{PatternBitSet, PatternBitSetError},
        weighted_pattern_set::WeightedPatternSet,
    },
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};
use clearra_pc_graph::{
    classification::ChainClass,
    dag::CheckpointSchedule,
    request::{
        PcCompletionGoal, PcExecutionPolicy, PcQueueInput, PcSolutionProbabilityPolicy, PieceWindow,
    },
};
use clearra_problem::{
    goal::{RequiredClearKind, RequiredClearLines, RequiredSpinKind, SpinMiniPolicy},
    query::ScenarioQuerySource,
    ContinuationPolicy, CountPolicy, ExactTargetPolicy, HoldAutomatonState, KickProfile,
    PcChanceEvidencePolicy, SearchGoal, SearchOutputPolicy, SearchProblem, SearchProblemBoard,
    SearchProblemBudget, SearchProblemKind, SearchProblemPreset, SearchReplayTracePolicy,
    TracePolicy,
};
use clearra_rules::{
    kicks::{KickOffset, KickTableProfileId, KickTransition},
    profile::rule_profile::RuleProfile,
    spawn::SpawnProfile,
};
use clearra_supply::{
    hold::hold_slot::HoldSlot,
    mixed::supply_provenance::BagBoundaryEvidence,
    pattern_universe::MaterializedPatternUniverseStructure,
    piece_source::{PieceSourceKind, SupplyTruncationReason},
    QueueObservationPolicy,
};

/// Exact, compact snapshot of the compiled problem that produced PC chance rows.
///
/// This intentionally does not retain `SearchProblem` or its materialized sequence
/// universe. Instead it owns the normalized execution fields and the source inputs
/// needed to reproduce that universe. The canonical weight representation is
/// shared, so arbitrary explicit weights remain exact without copying their payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcChanceProblemEvidence {
    problem_id: String,
    problem_kind: SearchProblemKind,
    preset: SearchProblemPreset,
    scenario_source: ScenarioQuerySource,
    checkpoint_schedule: Option<PcChanceCheckpointScheduleEvidence>,
    chain_class: ChainClass,
    completion_goal: PcCompletionGoal,
    initial_occupancy: Option<OccupancyField>,
    board: SearchProblemBoard,
    board_profile: PcChanceBoardProfileEvidence,
    piece_source: PcChancePieceSourceEvidence,
    initial_hold: HoldAutomatonState,
    piece_window: PieceWindow,
    exact_pieces: Option<usize>,
    supply: PcChanceSupplyEvidence,
    piece_set: PcChancePieceSetEvidence,
    rule: PcChanceRuleEvidence,
    search_goal: PcChanceSearchGoalEvidence,
    exact_target_policy: ExactTargetPolicy,
    count_policy: CountPolicy,
    objective: clearra_objectives::policy::objective_policy::ObjectivePolicy,
    solution_probability_policy: PcSolutionProbabilityPolicy,
    queue_observation_policy: QueueObservationPolicy,
    budget: SearchProblemBudget,
    resource_budget: SearchProblemBudget,
    backend_policy: PcExecutionPolicy,
    output_policy: SearchOutputPolicy,
    pc_chance_evidence_policy: PcChanceEvidencePolicy,
    replay_trace_policy: SearchReplayTracePolicy,
    trace_policy: TracePolicy,
    continuation_policy: ContinuationPolicy,
    labels: Vec<String>,
    allowed_colored_solution_identities: Option<Vec<StandardBoard64ColoredTilingIdentity>>,
}

/// Producer-owned, full normalized problem binding for typed PC score output.
///
/// The compact problem snapshot is shared with PC chance so both product
/// projections compare the same exhaustive field inventory. Replay profile
/// identities are captured from that same executed problem and later checked
/// against the exact scoring batch; App-owned problem pointers are never used
/// as producer evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcScoreProblemEvidence {
    identity: PcScoreProblemEvidenceIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PcScoreProblemEvidenceIdentity {
    PcScoreV2(PcScoreReplayProblemEvidence),
    PcScorePortfolioV2(PcScorePortfolioProblemEvidence),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PcScoreReplayProblemEvidence {
    problem: PcChanceProblemEvidence,
    kick_table_id: u64,
    rule_profile_id: u64,
}

/// Purpose-separated score identity for the typed `pc.score-minimals`
/// producer. Keeping this wrapper private prevents callers from treating the
/// combined portfolio contract as ordinary `pc.score` evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
struct PcScorePortfolioProblemEvidence {
    replay: PcScoreReplayProblemEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PcChanceCheckpointScheduleEvidence {
    target: PcTarget,
    label: String,
    partition_increments: Vec<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PcChanceBoardProfileEvidence {
    id: &'static str,
    width: u16,
    height: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PcChancePieceSourceEvidence {
    id: u64,
    kind: PieceSourceKind,
    piece_set_id: u8,
    provenance: PcChanceSourceProvenanceEvidence,
    fixed_sequence: Option<Vec<PieceKind>>,
    bag_universe_pattern: Option<Vec<PieceKind>>,
    observed_window: Option<PcChanceObservedWindowEvidence>,
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    pattern_count: usize,
    total_possible_pattern_count: u128,
    materialized_probability_mass_bits: u64,
    complete: bool,
    truncation_reason: Option<SupplyTruncationReason>,
    structure: MaterializedPatternUniverseStructure,
    weights: PcChancePatternWeightsEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PcChanceSourceProvenanceEvidence {
    id: u64,
    bag_profile_id: String,
    piece_set_id: String,
    observed_window_id: Option<String>,
    bag_boundary_evidence: BagBoundaryEvidence,
    duplicate_witness: bool,
    ambiguity_report: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PcChanceObservedWindowEvidence {
    pieces: Vec<PieceKind>,
    budget: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct PcChancePatternWeightsEvidence(WeightedPatternSet);

// `WeightedPatternSet` only admits finite `ProbabilityValue`s, normalizes zero,
// and has reflexive structural equality. The local wrapper may therefore carry
// the stronger Eq contract required by `CoreExecutionResult`.
impl Eq for PcChancePatternWeightsEvidence {}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PcChanceSupplyEvidence {
    queue: PcChanceQueueEvidence,
    hold_state: HoldSlot,
    hold_enabled: bool,
    source_sequence_length: usize,
    projects_unplaced_lookahead: bool,
    bag: PcChanceBagEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PcChanceQueueEvidence {
    FixedSequence(Vec<PieceKind>),
    BagAlignedPattern(Vec<PieceKind>),
    PatternExpression {
        source: String,
        sequence_len: usize,
        pattern_count: usize,
        factorized: bool,
    },
    Standard7Bag,
    Observed(Vec<PieceKind>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PcChancePieceSetEvidence {
    id: &'static str,
    pieces: Vec<PieceKind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PcChanceBagEvidence {
    id: &'static str,
    piece_set_id: &'static str,
    pieces_per_bag: Vec<PieceKind>,
    entries: Vec<(PieceKind, usize, u32)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PcChanceRuleEvidence {
    rule: RuleProfile,
    verified_kick_profile: Option<PcChanceVerifiedKickProfileEvidence>,
    rule_spawn_profile: SpawnProfile,
    kick_profile: KickProfile,
    spawn_profile: SpawnProfile,
    requires_180: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PcChanceVerifiedKickProfileEvidence {
    id: KickTableProfileId,
    source_rule: clearra_rules::profile::rule_profile::RuleProfileId,
    entries: Vec<PcChanceKickEntryEvidence>,
    issue_count: usize,
    missing_transition_count: usize,
    duplicate_transition_count: usize,
    unsupported_annotation_count: usize,
    supports_180: bool,
    transition_complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PcChanceKickEntryEvidence {
    transition: KickTransition,
    offsets: Vec<KickOffset>,
    unsupported_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PcChanceSearchGoalEvidence {
    ClearToEmpty,
    BuildTemplate(String),
    SpinTarget(PcChanceSpinTargetEvidence),
    Composite(Vec<PcChanceSearchGoalEvidence>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PcChanceSpinTargetEvidence {
    id: String,
    piece_selector: PcChanceSpinPieceSelectorEvidence,
    spin_kind: RequiredSpinKind,
    clear_lines: RequiredClearLines,
    mini_policy: SpinMiniPolicy,
    required_clear_kind: RequiredClearKind,
    required_score_profile_id: Option<String>,
    target_probability_threshold_bits: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PcChanceSpinPieceSelectorEvidence {
    TOnly,
    AnyPiece,
    PieceSet(String),
}

impl PcChanceProblemEvidence {
    fn from_search_problem(problem: &SearchProblem) -> Result<Self, PcChanceProblemEvidenceError> {
        if !problem
            .pc_chance_evidence_policy()
            .retains_pc_coverage_evidence()
        {
            return Err(PcChanceProblemEvidenceError::EvidencePolicyDisabled);
        }
        if problem
            .pc_chance_evidence_policy()
            .retains_pc_score_portfolio_v2_evidence()
        {
            validate_pc_score_portfolio_problem(problem)?;
        }
        Self::from_search_problem_fields(problem)
    }

    fn from_score_search_problem(
        problem: &SearchProblem,
    ) -> Result<Self, PcChanceProblemEvidenceError> {
        if problem.pc_chance_evidence_policy() != PcChanceEvidencePolicy::Disabled {
            return Err(PcChanceProblemEvidenceError::UnexpectedChanceEvidencePolicy);
        }
        Self::from_search_problem_fields(problem)
    }

    fn from_score_portfolio_search_problem(
        problem: &SearchProblem,
    ) -> Result<Self, PcChanceProblemEvidenceError> {
        if !problem
            .pc_chance_evidence_policy()
            .retains_pc_score_portfolio_v2_evidence()
        {
            return Err(PcChanceProblemEvidenceError::UnexpectedChanceEvidencePolicy);
        }
        validate_pc_score_portfolio_problem(problem)?;
        Self::from_search_problem_fields(problem)
    }

    fn from_search_problem_fields(
        problem: &SearchProblem,
    ) -> Result<Self, PcChanceProblemEvidenceError> {
        if problem.scenario().setup_query().is_some() {
            return Err(PcChanceProblemEvidenceError::UnexpectedSetupQuery);
        }
        if problem.scenario().build_query().is_some() {
            return Err(PcChanceProblemEvidenceError::UnexpectedBuildQuery);
        }
        let source = problem.piece_source();
        let universe = source
            .materialized_universe()
            .ok_or(PcChanceProblemEvidenceError::MissingMaterializedPatternUniverse)?;
        validate_problem_identity(
            source.id().get(),
            universe.pattern_universe_id(),
            universe.pattern_weight_model_id(),
        )?;

        let board_profile = problem.board_profile();
        let board_size = board_profile.size();
        let source_provenance = source.provenance();
        let supply = problem.supply();
        let bag = supply.bag();
        let piece_set = problem.piece_set();
        let rule_selection = problem.rule_profile();

        Ok(Self {
            problem_id: problem.problem_id().as_str().to_owned(),
            problem_kind: problem.problem_kind(),
            preset: problem.preset(),
            scenario_source: problem.scenario().source(),
            checkpoint_schedule: problem
                .checkpoint_schedule()
                .map(PcChanceCheckpointScheduleEvidence::from_schedule),
            chain_class: problem.chain_class(),
            completion_goal: problem.goal(),
            initial_occupancy: problem.initial_occupancy().copied(),
            board: problem.board().clone(),
            board_profile: PcChanceBoardProfileEvidence {
                id: board_profile.id().as_str(),
                width: board_size.width(),
                height: board_size.height(),
            },
            piece_source: PcChancePieceSourceEvidence {
                id: source.id().get(),
                kind: source.kind(),
                piece_set_id: source.piece_set_id().get(),
                provenance: PcChanceSourceProvenanceEvidence {
                    id: source_provenance.supply_provenance_id(),
                    bag_profile_id: source_provenance.bag_profile_id().to_owned(),
                    piece_set_id: source_provenance.piece_set_id().to_owned(),
                    observed_window_id: source_provenance
                        .observed_window_id()
                        .map(ToOwned::to_owned),
                    bag_boundary_evidence: source_provenance.bag_boundary_evidence(),
                    duplicate_witness: source_provenance.duplicate_witness(),
                    ambiguity_report: source_provenance.ambiguity_report(),
                },
                fixed_sequence: source
                    .fixed_sequence()
                    .map(|sequence| sequence.pieces().to_vec()),
                bag_universe_pattern: source
                    .bag_universe_descriptor()
                    .map(|descriptor| descriptor.pattern().to_vec()),
                observed_window: source.observed_window_descriptor().map(|window| {
                    PcChanceObservedWindowEvidence {
                        pieces: window.observed().to_vec(),
                        budget: window.budget(),
                    }
                }),
                pattern_universe_id: universe.pattern_universe_id(),
                pattern_weight_model_id: universe.pattern_weight_model_id(),
                pattern_count: universe.pattern_count(),
                total_possible_pattern_count: universe.total_possible_pattern_count(),
                materialized_probability_mass_bits: universe
                    .materialized_probability_mass()
                    .get()
                    .to_bits(),
                complete: universe.complete(),
                truncation_reason: universe.truncation_reason(),
                structure: universe.structure(),
                weights: PcChancePatternWeightsEvidence(universe.weights().clone()),
            },
            initial_hold: problem.initial_hold(),
            piece_window: problem.piece_window(),
            exact_pieces: problem.exact_pieces(),
            supply: PcChanceSupplyEvidence {
                queue: PcChanceQueueEvidence::from_queue(supply.queue()),
                hold_state: supply.hold_state(),
                hold_enabled: supply.hold_enabled(),
                source_sequence_length: supply.source_sequence_length(),
                projects_unplaced_lookahead: supply.projects_unplaced_lookahead(),
                bag: PcChanceBagEvidence {
                    id: bag.id().as_str(),
                    piece_set_id: bag.piece_set_id().as_str(),
                    pieces_per_bag: bag.pieces_per_bag().to_vec(),
                    entries: bag
                        .entries()
                        .iter()
                        .map(|entry| (entry.piece(), entry.multiplicity(), entry.weight()))
                        .collect(),
                },
            },
            piece_set: PcChancePieceSetEvidence {
                id: piece_set.id().as_str(),
                pieces: piece_set.pieces().to_vec(),
            },
            rule: PcChanceRuleEvidence {
                rule: rule_selection.rule(),
                verified_kick_profile: rule_selection
                    .verified_kick_profile()
                    .map(PcChanceVerifiedKickProfileEvidence::from_verified),
                rule_spawn_profile: rule_selection.spawn_profile(),
                kick_profile: problem.kick_profile(),
                spawn_profile: problem.spawn_profile(),
                requires_180: problem.core_query().requires_180(),
            },
            search_goal: PcChanceSearchGoalEvidence::from_goal(problem.search_goal()),
            exact_target_policy: problem.exact_target_policy(),
            count_policy: problem.count_policy(),
            objective: problem.objective(),
            solution_probability_policy: problem.solution_probability_policy(),
            queue_observation_policy: problem.queue_observation_policy(),
            budget: problem.budget(),
            resource_budget: problem.resource_budget(),
            backend_policy: problem.backend_policy().clone(),
            output_policy: problem.output_policy(),
            pc_chance_evidence_policy: problem.pc_chance_evidence_policy(),
            replay_trace_policy: problem.replay_trace_policy(),
            trace_policy: problem.trace_policy(),
            continuation_policy: problem.continuation_policy(),
            labels: problem.labels().to_vec(),
            allowed_colored_solution_identities: problem
                .allowed_colored_solution_identities()
                .map(ToOwned::to_owned),
        })
    }

    /// Compares every retained normalized field against a freshly compiled problem.
    pub fn matches_search_problem(&self, problem: &SearchProblem) -> bool {
        let source = problem.piece_source();
        if problem.scenario().setup_query().is_some() || problem.scenario().build_query().is_some()
        {
            return false;
        }
        let Some(universe) = source.materialized_universe() else {
            return false;
        };
        if validate_problem_identity(
            source.id().get(),
            universe.pattern_universe_id(),
            universe.pattern_weight_model_id(),
        )
        .is_err()
        {
            return false;
        }
        let board_profile = problem.board_profile();
        let board_size = board_profile.size();
        let piece_set = problem.piece_set();

        self.problem_id == problem.problem_id().as_str()
            && self.problem_kind == problem.problem_kind()
            && self.preset == problem.preset()
            && self.scenario_source == problem.scenario().source()
            && match (&self.checkpoint_schedule, problem.checkpoint_schedule()) {
                (None, None) => true,
                (Some(expected), Some(actual)) => expected.matches(actual),
                (None, Some(_)) | (Some(_), None) => false,
            }
            && self.chain_class == problem.chain_class()
            && self.completion_goal == problem.goal()
            && self.initial_occupancy.as_ref() == problem.initial_occupancy()
            && &self.board == problem.board()
            && self.board_profile.id == board_profile.id().as_str()
            && self.board_profile.width == board_size.width()
            && self.board_profile.height == board_size.height()
            && self.piece_source.matches(source, universe)
            && self.initial_hold == problem.initial_hold()
            && self.piece_window == problem.piece_window()
            && self.exact_pieces == problem.exact_pieces()
            && self.supply.matches(problem.supply())
            && self.piece_set.id == piece_set.id().as_str()
            && self.piece_set.pieces.as_slice() == piece_set.pieces()
            && self.rule.matches(problem)
            && self.search_goal.matches(problem.search_goal())
            && self.exact_target_policy == problem.exact_target_policy()
            && self.count_policy == problem.count_policy()
            && self.objective == problem.objective()
            && self.solution_probability_policy == problem.solution_probability_policy()
            && self.queue_observation_policy == problem.queue_observation_policy()
            && self.budget == problem.budget()
            && self.resource_budget == problem.resource_budget()
            && self.backend_policy == *problem.backend_policy()
            && self.output_policy == problem.output_policy()
            && self.pc_chance_evidence_policy == problem.pc_chance_evidence_policy()
            && self.replay_trace_policy == problem.replay_trace_policy()
            && self.trace_policy == problem.trace_policy()
            && self.continuation_policy == problem.continuation_policy()
            && self.labels.as_slice() == problem.labels()
            && self.allowed_colored_solution_identities.as_deref()
                == problem.allowed_colored_solution_identities()
    }

    pub fn problem_id(&self) -> &str {
        &self.problem_id
    }

    pub const fn problem_kind(&self) -> SearchProblemKind {
        self.problem_kind
    }

    pub const fn preset(&self) -> SearchProblemPreset {
        self.preset
    }

    pub const fn output_policy(&self) -> SearchOutputPolicy {
        self.output_policy
    }

    pub const fn completion_goal(&self) -> PcCompletionGoal {
        self.completion_goal
    }

    pub const fn pc_chance_evidence_policy(&self) -> PcChanceEvidencePolicy {
        self.pc_chance_evidence_policy
    }

    pub const fn piece_source_id(&self) -> u64 {
        self.piece_source.id
    }

    pub const fn pattern_universe_id(&self) -> PatternUniverseId {
        self.piece_source.pattern_universe_id
    }

    pub const fn pattern_weight_model_id(&self) -> PatternWeightModelId {
        self.piece_source.pattern_weight_model_id
    }

    pub const fn pattern_count(&self) -> usize {
        self.piece_source.pattern_count
    }

    pub const fn total_possible_pattern_count(&self) -> u128 {
        self.piece_source.total_possible_pattern_count
    }

    pub const fn materialized_probability_mass_bits(&self) -> u64 {
        self.piece_source.materialized_probability_mass_bits
    }

    pub const fn piece_source_complete(&self) -> bool {
        self.piece_source.complete
    }

    fn checked_storage_retained_bytes(&self) -> Option<u128> {
        let mut bytes = self.problem_id.capacity() as u128;
        if let Some(schedule) = &self.checkpoint_schedule {
            bytes = bytes.checked_add(schedule.checked_storage_retained_bytes()?)?;
        }
        bytes = bytes.checked_add(self.piece_source.checked_storage_retained_bytes()?)?;
        bytes = bytes.checked_add(self.supply.checked_storage_retained_bytes()?)?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.piece_set.pieces)?)?;
        bytes = bytes.checked_add(self.rule.checked_storage_retained_bytes()?)?;
        bytes = bytes.checked_add(self.search_goal.checked_storage_retained_bytes()?)?;
        bytes = bytes.checked_add(checked_string_vec_retained_bytes(&self.labels)?)?;
        if let Some(identities) = &self.allowed_colored_solution_identities {
            bytes = bytes.checked_add(checked_vec_capacity_bytes(identities)?)?;
        }
        Some(bytes)
    }
}

impl PcScoreProblemEvidence {
    pub(crate) fn from_executed_problem(
        problem: &SearchProblem,
    ) -> Result<Self, PcChanceProblemEvidenceError> {
        let snapshot = PcChanceProblemEvidence::from_score_search_problem(problem)?;
        Ok(Self {
            identity: PcScoreProblemEvidenceIdentity::PcScoreV2(
                PcScoreReplayProblemEvidence::from_snapshot(snapshot, problem)?,
            ),
        })
    }

    pub(crate) fn from_executed_score_portfolio_problem(
        problem: &SearchProblem,
    ) -> Result<Self, PcChanceProblemEvidenceError> {
        Ok(Self {
            identity: PcScoreProblemEvidenceIdentity::PcScorePortfolioV2(
                PcScorePortfolioProblemEvidence::from_executed_problem(problem)?,
            ),
        })
    }

    /// Exact fieldwise comparison against the App authority's freshly compiled
    /// expected problem. This does not rely on the non-exhaustive problem ID,
    /// and the closed evidence identity prevents score-only and score-portfolio
    /// authorities from accepting one another.
    pub fn matches_search_problem(&self, problem: &SearchProblem) -> bool {
        match &self.identity {
            PcScoreProblemEvidenceIdentity::PcScoreV2(identity) => {
                problem.pc_chance_evidence_policy() == PcChanceEvidencePolicy::Disabled
                    && identity.matches_search_problem(problem)
            }
            PcScoreProblemEvidenceIdentity::PcScorePortfolioV2(evidence) => {
                evidence.matches_search_problem(problem)
            }
        }
    }

    pub const fn kick_table_id(&self) -> u64 {
        match &self.identity {
            PcScoreProblemEvidenceIdentity::PcScoreV2(identity) => identity.kick_table_id,
            PcScoreProblemEvidenceIdentity::PcScorePortfolioV2(evidence) => {
                evidence.replay.kick_table_id
            }
        }
    }

    pub const fn rule_profile_id(&self) -> u64 {
        match &self.identity {
            PcScoreProblemEvidenceIdentity::PcScoreV2(identity) => identity.rule_profile_id,
            PcScoreProblemEvidenceIdentity::PcScorePortfolioV2(evidence) => {
                evidence.replay.rule_profile_id
            }
        }
    }

    pub(crate) fn checked_storage_retained_bytes(&self) -> Option<u128> {
        match &self.identity {
            PcScoreProblemEvidenceIdentity::PcScoreV2(identity) => {
                identity.checked_storage_retained_bytes()
            }
            PcScoreProblemEvidenceIdentity::PcScorePortfolioV2(evidence) => {
                evidence.replay.checked_storage_retained_bytes()
            }
        }
    }
}

impl PcScoreReplayProblemEvidence {
    fn from_snapshot(
        snapshot: PcChanceProblemEvidence,
        problem: &SearchProblem,
    ) -> Result<Self, PcChanceProblemEvidenceError> {
        let kick_profile = problem.kick_profile();
        let kick_table_id = u64::from(kick_profile_code(kick_profile.profile_id()));
        let rule_profile_id = u64::from(rule_profile_code(kick_profile.source_rule()));
        if kick_table_id == 0 {
            return Err(PcChanceProblemEvidenceError::ZeroReplayKickTableId);
        }
        if rule_profile_id == 0 {
            return Err(PcChanceProblemEvidenceError::ZeroReplayRuleProfileId);
        }
        Ok(Self {
            problem: snapshot,
            kick_table_id,
            rule_profile_id,
        })
    }

    fn matches_search_problem(&self, problem: &SearchProblem) -> bool {
        let kick_profile = problem.kick_profile();
        self.problem.matches_search_problem(problem)
            && self.kick_table_id == u64::from(kick_profile_code(kick_profile.profile_id()))
            && self.rule_profile_id == u64::from(rule_profile_code(kick_profile.source_rule()))
    }

    fn checked_storage_retained_bytes(&self) -> Option<u128> {
        self.problem.checked_storage_retained_bytes()
    }
}

impl PcScorePortfolioProblemEvidence {
    fn from_executed_problem(
        problem: &SearchProblem,
    ) -> Result<Self, PcChanceProblemEvidenceError> {
        let snapshot = PcChanceProblemEvidence::from_score_portfolio_search_problem(problem)?;
        Ok(Self {
            replay: PcScoreReplayProblemEvidence::from_snapshot(snapshot, problem)?,
        })
    }

    fn matches_search_problem(&self, problem: &SearchProblem) -> bool {
        problem
            .pc_chance_evidence_policy()
            .retains_pc_score_portfolio_v2_evidence()
            && validate_pc_score_portfolio_problem(problem).is_ok()
            && self.replay.matches_search_problem(problem)
    }
}

impl PcChanceCheckpointScheduleEvidence {
    fn from_schedule(schedule: &CheckpointSchedule) -> Self {
        Self {
            target: schedule.target(),
            label: schedule.label().to_owned(),
            partition_increments: schedule
                .partitions()
                .iter()
                .map(|partition| {
                    partition
                        .increments()
                        .iter()
                        .map(|increment| increment.lines())
                        .collect()
                })
                .collect(),
        }
    }

    fn matches(&self, schedule: &CheckpointSchedule) -> bool {
        self.target == schedule.target()
            && self.label == schedule.label()
            && self.partition_increments.len() == schedule.partitions().len()
            && self
                .partition_increments
                .iter()
                .zip(schedule.partitions())
                .all(|(expected, actual)| {
                    expected.len() == actual.increments().len()
                        && expected
                            .iter()
                            .zip(actual.increments())
                            .all(|(lines, increment)| *lines == increment.lines())
                })
    }

    fn checked_storage_retained_bytes(&self) -> Option<u128> {
        let mut bytes = self.label.capacity() as u128;
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.partition_increments)?)?;
        for increments in &self.partition_increments {
            bytes = bytes.checked_add(checked_vec_capacity_bytes(increments)?)?;
        }
        Some(bytes)
    }
}

impl PcChanceQueueEvidence {
    fn from_queue(queue: &PcQueueInput) -> Self {
        match queue {
            PcQueueInput::FixedSequence(sequence) => {
                Self::FixedSequence(sequence.pieces().to_vec())
            }
            PcQueueInput::BagAlignedPattern(pattern) => {
                Self::BagAlignedPattern(pattern.pieces().to_vec())
            }
            PcQueueInput::PatternExpression(expression) => Self::PatternExpression {
                source: expression.source().to_owned(),
                sequence_len: expression.sequence_len(),
                pattern_count: expression.pattern_count(),
                factorized: expression.is_factorized(),
            },
            PcQueueInput::Standard7Bag => Self::Standard7Bag,
            PcQueueInput::Observed(queue) => Self::Observed(queue.pieces().to_vec()),
        }
    }

    fn checked_storage_retained_bytes(&self) -> Option<u128> {
        match self {
            Self::FixedSequence(pieces)
            | Self::BagAlignedPattern(pieces)
            | Self::Observed(pieces) => checked_vec_capacity_bytes(pieces),
            Self::PatternExpression { source, .. } => Some(source.capacity() as u128),
            Self::Standard7Bag => Some(0),
        }
    }

    fn matches(&self, queue: &PcQueueInput) -> bool {
        match (self, queue) {
            (Self::FixedSequence(expected), PcQueueInput::FixedSequence(actual)) => {
                expected.as_slice() == actual.pieces()
            }
            (Self::BagAlignedPattern(expected), PcQueueInput::BagAlignedPattern(actual)) => {
                expected.as_slice() == actual.pieces()
            }
            (
                Self::PatternExpression {
                    source,
                    sequence_len,
                    pattern_count,
                    factorized,
                },
                PcQueueInput::PatternExpression(actual),
            ) => {
                source == actual.source()
                    && *sequence_len == actual.sequence_len()
                    && *pattern_count == actual.pattern_count()
                    && *factorized == actual.is_factorized()
            }
            (Self::Standard7Bag, PcQueueInput::Standard7Bag) => true,
            (Self::Observed(expected), PcQueueInput::Observed(actual)) => {
                expected.as_slice() == actual.pieces()
            }
            _ => false,
        }
    }
}

impl PcChancePieceSourceEvidence {
    fn matches(
        &self,
        source: &clearra_problem::PieceSource,
        universe: &clearra_supply::pattern_universe::MaterializedPatternUniverse,
    ) -> bool {
        let provenance = source.provenance();
        self.id == source.id().get()
            && self.kind == source.kind()
            && self.piece_set_id == source.piece_set_id().get()
            && self.provenance.id == provenance.supply_provenance_id()
            && self.provenance.bag_profile_id == provenance.bag_profile_id()
            && self.provenance.piece_set_id == provenance.piece_set_id()
            && self.provenance.observed_window_id.as_deref() == provenance.observed_window_id()
            && self.provenance.bag_boundary_evidence == provenance.bag_boundary_evidence()
            && self.provenance.duplicate_witness == provenance.duplicate_witness()
            && self.provenance.ambiguity_report == provenance.ambiguity_report()
            && self.fixed_sequence.as_deref()
                == source.fixed_sequence().map(|sequence| sequence.pieces())
            && self.bag_universe_pattern.as_deref()
                == source
                    .bag_universe_descriptor()
                    .map(|descriptor| descriptor.pattern())
            && match (&self.observed_window, source.observed_window_descriptor()) {
                (None, None) => true,
                (Some(expected), Some(actual)) => {
                    expected.pieces.as_slice() == actual.observed()
                        && expected.budget == actual.budget()
                }
                (None, Some(_)) | (Some(_), None) => false,
            }
            && self.pattern_universe_id == universe.pattern_universe_id()
            && self.pattern_weight_model_id == universe.pattern_weight_model_id()
            && self.pattern_count == universe.pattern_count()
            && self.total_possible_pattern_count == universe.total_possible_pattern_count()
            && self.materialized_probability_mass_bits
                == universe.materialized_probability_mass().get().to_bits()
            && self.complete == universe.complete()
            && self.truncation_reason == universe.truncation_reason()
            && self.structure == universe.structure()
            && self.weights.0 == *universe.weights()
    }

    fn checked_storage_retained_bytes(&self) -> Option<u128> {
        let mut bytes = self.provenance.bag_profile_id.capacity() as u128;
        bytes = bytes.checked_add(self.provenance.piece_set_id.capacity() as u128)?;
        if let Some(value) = &self.provenance.observed_window_id {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        if let Some(pieces) = &self.fixed_sequence {
            bytes = bytes.checked_add(checked_vec_capacity_bytes(pieces)?)?;
        }
        if let Some(pieces) = &self.bag_universe_pattern {
            bytes = bytes.checked_add(checked_vec_capacity_bytes(pieces)?)?;
        }
        if let Some(window) = &self.observed_window {
            bytes = bytes.checked_add(checked_vec_capacity_bytes(&window.pieces)?)?;
        }
        bytes = bytes.checked_add(self.weights.0.checked_storage_retained_bytes()?)?;
        Some(bytes)
    }
}

impl PcChanceSupplyEvidence {
    fn matches(&self, supply: &clearra_problem::SupplyProvenance) -> bool {
        let bag = supply.bag();
        self.queue.matches(supply.queue())
            && self.hold_state == supply.hold_state()
            && self.hold_enabled == supply.hold_enabled()
            && self.source_sequence_length == supply.source_sequence_length()
            && self.projects_unplaced_lookahead == supply.projects_unplaced_lookahead()
            && self.bag.id == bag.id().as_str()
            && self.bag.piece_set_id == bag.piece_set_id().as_str()
            && self.bag.pieces_per_bag.as_slice() == bag.pieces_per_bag()
            && self.bag.entries.len() == bag.entries().len()
            && self
                .bag
                .entries
                .iter()
                .zip(bag.entries())
                .all(|(expected, actual)| {
                    *expected == (actual.piece(), actual.multiplicity(), actual.weight())
                })
    }

    fn checked_storage_retained_bytes(&self) -> Option<u128> {
        let mut bytes = self.queue.checked_storage_retained_bytes()?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.bag.pieces_per_bag)?)?;
        bytes = bytes.checked_add(checked_vec_capacity_bytes(&self.bag.entries)?)?;
        Some(bytes)
    }
}

impl PcChanceVerifiedKickProfileEvidence {
    fn from_verified(verified: &clearra_rules::kicks::VerifiedKickTableProfile) -> Self {
        let profile = verified.profile();
        let report = verified.report();
        Self {
            id: profile.id(),
            source_rule: profile.source_rule(),
            entries: profile
                .entries()
                .iter()
                .map(|entry| PcChanceKickEntryEvidence {
                    transition: entry.transition(),
                    offsets: entry.sequence().offsets().to_vec(),
                    unsupported_reason: entry.unsupported_reason().map(ToOwned::to_owned),
                })
                .collect(),
            issue_count: report.issue_count(),
            missing_transition_count: report.missing_transition_count(),
            duplicate_transition_count: report.duplicate_transition_count(),
            unsupported_annotation_count: report.unsupported_annotation_count(),
            supports_180: report.supports_180(),
            transition_complete: report.transition_complete(),
        }
    }

    fn checked_storage_retained_bytes(&self) -> Option<u128> {
        let mut bytes = checked_vec_capacity_bytes(&self.entries)?;
        for entry in &self.entries {
            bytes = bytes.checked_add(checked_vec_capacity_bytes(&entry.offsets)?)?;
            if let Some(reason) = &entry.unsupported_reason {
                bytes = bytes.checked_add(reason.capacity() as u128)?;
            }
        }
        Some(bytes)
    }

    fn matches(&self, verified: &clearra_rules::kicks::VerifiedKickTableProfile) -> bool {
        let profile = verified.profile();
        let report = verified.report();
        self.id == profile.id()
            && self.source_rule == profile.source_rule()
            && self.entries.len() == profile.entries().len()
            && self
                .entries
                .iter()
                .zip(profile.entries())
                .all(|(expected, actual)| {
                    expected.transition == actual.transition()
                        && expected.offsets.as_slice() == actual.sequence().offsets()
                        && expected.unsupported_reason.as_deref() == actual.unsupported_reason()
                })
            && self.issue_count == report.issue_count()
            && self.missing_transition_count == report.missing_transition_count()
            && self.duplicate_transition_count == report.duplicate_transition_count()
            && self.unsupported_annotation_count == report.unsupported_annotation_count()
            && self.supports_180 == report.supports_180()
            && self.transition_complete == report.transition_complete()
    }
}

impl PcChanceRuleEvidence {
    fn matches(&self, problem: &SearchProblem) -> bool {
        let selection = problem.rule_profile();
        self.rule == selection.rule()
            && match (
                &self.verified_kick_profile,
                selection.verified_kick_profile(),
            ) {
                (None, None) => true,
                (Some(expected), Some(actual)) => expected.matches(actual),
                (None, Some(_)) | (Some(_), None) => false,
            }
            && self.rule_spawn_profile == selection.spawn_profile()
            && self.kick_profile == problem.kick_profile()
            && self.spawn_profile == problem.spawn_profile()
            && self.requires_180 == problem.core_query().requires_180()
    }

    fn checked_storage_retained_bytes(&self) -> Option<u128> {
        self.verified_kick_profile.as_ref().map_or(
            Some(0),
            PcChanceVerifiedKickProfileEvidence::checked_storage_retained_bytes,
        )
    }
}

impl PcChanceSearchGoalEvidence {
    fn from_goal(goal: &SearchGoal) -> Self {
        match goal {
            SearchGoal::ClearToEmpty => Self::ClearToEmpty,
            SearchGoal::BuildTemplate(goal) => Self::BuildTemplate(goal.template_id().to_owned()),
            SearchGoal::SpinTarget(target) => Self::SpinTarget(PcChanceSpinTargetEvidence {
                id: target.id().as_str().to_owned(),
                piece_selector: match target.spin_piece_selector() {
                    clearra_problem::SpinPieceSelector::TOnly => {
                        PcChanceSpinPieceSelectorEvidence::TOnly
                    }
                    clearra_problem::SpinPieceSelector::AnyPiece => {
                        PcChanceSpinPieceSelectorEvidence::AnyPiece
                    }
                    clearra_problem::SpinPieceSelector::PieceSet(pieces) => {
                        PcChanceSpinPieceSelectorEvidence::PieceSet(pieces.clone())
                    }
                },
                spin_kind: target.spin_kind(),
                clear_lines: target.clear_lines(),
                mini_policy: target.mini_policy(),
                required_clear_kind: target.required_clear_kind(),
                required_score_profile_id: target
                    .required_score_profile_id()
                    .map(|id| id.as_str().to_owned()),
                target_probability_threshold_bits: target
                    .target_probability_threshold()
                    .map(|value| value.get().to_bits()),
            }),
            SearchGoal::Composite(composite) => {
                Self::Composite(composite.goals().iter().map(Self::from_goal).collect())
            }
        }
    }

    fn checked_storage_retained_bytes(&self) -> Option<u128> {
        match self {
            Self::ClearToEmpty => Some(0),
            Self::BuildTemplate(template_id) => Some(template_id.capacity() as u128),
            Self::SpinTarget(target) => target.checked_storage_retained_bytes(),
            Self::Composite(goals) => {
                let mut bytes = checked_vec_capacity_bytes(goals)?;
                for goal in goals {
                    bytes = bytes.checked_add(goal.checked_storage_retained_bytes()?)?;
                }
                Some(bytes)
            }
        }
    }

    fn matches(&self, goal: &SearchGoal) -> bool {
        match (self, goal) {
            (Self::ClearToEmpty, SearchGoal::ClearToEmpty) => true,
            (Self::BuildTemplate(expected), SearchGoal::BuildTemplate(actual)) => {
                expected == actual.template_id()
            }
            (Self::SpinTarget(expected), SearchGoal::SpinTarget(actual)) => {
                expected.matches(actual)
            }
            (Self::Composite(expected), SearchGoal::Composite(actual)) => {
                expected.len() == actual.goals().len()
                    && expected
                        .iter()
                        .zip(actual.goals())
                        .all(|(expected, actual)| expected.matches(actual))
            }
            _ => false,
        }
    }
}

impl PcChanceSpinTargetEvidence {
    fn checked_storage_retained_bytes(&self) -> Option<u128> {
        let mut bytes = self.id.capacity() as u128;
        if let PcChanceSpinPieceSelectorEvidence::PieceSet(pieces) = &self.piece_selector {
            bytes = bytes.checked_add(pieces.capacity() as u128)?;
        }
        if let Some(profile_id) = &self.required_score_profile_id {
            bytes = bytes.checked_add(profile_id.capacity() as u128)?;
        }
        Some(bytes)
    }

    fn matches(&self, target: &clearra_problem::SpinTargetRequest) -> bool {
        self.id == target.id().as_str()
            && self.piece_selector.matches(target.spin_piece_selector())
            && self.spin_kind == target.spin_kind()
            && self.clear_lines == target.clear_lines()
            && self.mini_policy == target.mini_policy()
            && self.required_clear_kind == target.required_clear_kind()
            && self.required_score_profile_id.as_deref() == target.required_score_profile()
            && self.target_probability_threshold_bits
                == target
                    .target_probability_threshold()
                    .map(|value| value.get().to_bits())
    }
}

impl PcChanceSpinPieceSelectorEvidence {
    fn matches(&self, selector: &clearra_problem::SpinPieceSelector) -> bool {
        match (self, selector) {
            (Self::TOnly, clearra_problem::SpinPieceSelector::TOnly)
            | (Self::AnyPiece, clearra_problem::SpinPieceSelector::AnyPiece) => true,
            (Self::PieceSet(expected), clearra_problem::SpinPieceSelector::PieceSet(actual)) => {
                expected == actual
            }
            _ => false,
        }
    }
}

fn validate_pc_score_portfolio_problem(
    problem: &SearchProblem,
) -> Result<(), PcChanceProblemEvidenceError> {
    if problem.objective().kind() != ObjectiveKind::MinimumCover
        || !problem.objective().score().requested()
    {
        return Err(PcChanceProblemEvidenceError::UnexpectedScorePortfolioObjective);
    }
    Ok(())
}

fn validate_problem_identity(
    piece_source_id: u64,
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
) -> Result<(), PcChanceProblemEvidenceError> {
    if piece_source_id == 0 {
        return Err(PcChanceProblemEvidenceError::ZeroPieceSourceId);
    }
    if pattern_universe_id.get() == 0 {
        return Err(PcChanceProblemEvidenceError::ZeroPatternUniverseId);
    }
    if pattern_weight_model_id.get() == 0 {
        return Err(PcChanceProblemEvidenceError::ZeroPatternWeightModelId);
    }
    Ok(())
}

fn checked_vec_capacity_bytes<T>(values: &Vec<T>) -> Option<u128> {
    (values.capacity() as u128).checked_mul(core::mem::size_of::<T>() as u128)
}

fn checked_string_vec_retained_bytes(values: &Vec<String>) -> Option<u128> {
    let mut bytes = checked_vec_capacity_bytes(values)?;
    for value in values {
        bytes = bytes.checked_add(value.capacity() as u128)?;
    }
    Some(bytes)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PcChanceProblemEvidenceError {
    EvidencePolicyDisabled,
    UnexpectedChanceEvidencePolicy,
    UnexpectedScorePortfolioObjective,
    UnexpectedSetupQuery,
    UnexpectedBuildQuery,
    MissingMaterializedPatternUniverse,
    ZeroPieceSourceId,
    ZeroPatternUniverseId,
    ZeroPatternWeightModelId,
    ZeroReplayKickTableId,
    ZeroReplayRuleProfileId,
}

/// Product-private Build coverage evidence retained for exact PC chance validation.
///
/// The batch identity is retained independently of the rows so an empty, complete
/// batch still identifies the exact source and weighted pattern universe it covers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcChanceCoverageEvidence {
    problem: PcChanceProblemEvidence,
    row_kind: CoverageRowKind,
    piece_source_id: u64,
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    pattern_count: usize,
    rows: Vec<CoverageRow>,
    complete: bool,
}

impl PcChanceCoverageEvidence {
    pub(crate) fn from_problem_rows(
        problem: &SearchProblem,
        rows: Vec<CoverageRow>,
        complete: bool,
    ) -> Result<Self, PcChanceCoverageEvidenceError> {
        let problem = PcChanceProblemEvidence::from_search_problem(problem)
            .map_err(PcChanceCoverageEvidenceError::Problem)?;
        let piece_source_id = problem.piece_source_id();
        let pattern_universe_id = problem.pattern_universe_id();
        let pattern_weight_model_id = problem.pattern_weight_model_id();
        let pattern_count = problem.pattern_count();
        for (row_index, row) in rows.iter().enumerate() {
            if row.row_kind() != &CoverageRowKind::Build {
                return Err(PcChanceCoverageEvidenceError::RowKindMismatch { row_index });
            }
            if row.piece_source_id() != piece_source_id {
                return Err(PcChanceCoverageEvidenceError::PieceSourceMismatch {
                    row_index,
                    expected: piece_source_id,
                    actual: row.piece_source_id(),
                });
            }
            if row.pattern_universe_id() != pattern_universe_id {
                return Err(PcChanceCoverageEvidenceError::PatternUniverseMismatch {
                    row_index,
                    expected: pattern_universe_id,
                    actual: row.pattern_universe_id(),
                });
            }
            if row.pattern_weight_model_id() != pattern_weight_model_id {
                return Err(PcChanceCoverageEvidenceError::PatternWeightModelMismatch {
                    row_index,
                    expected: pattern_weight_model_id,
                    actual: row.pattern_weight_model_id(),
                });
            }
            if row.pattern_count() != pattern_count {
                return Err(PcChanceCoverageEvidenceError::PatternCountMismatch {
                    row_index,
                    expected: pattern_count,
                    actual: row.pattern_count(),
                });
            }
        }

        Ok(Self {
            problem,
            row_kind: CoverageRowKind::Build,
            piece_source_id,
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
            rows,
            complete,
        })
    }

    pub fn problem(&self) -> &PcChanceProblemEvidence {
        &self.problem
    }

    pub fn row_kind(&self) -> &CoverageRowKind {
        &self.row_kind
    }

    pub const fn piece_source_id(&self) -> u64 {
        self.piece_source_id
    }

    pub const fn pattern_universe_id(&self) -> PatternUniverseId {
        self.pattern_universe_id
    }

    pub const fn pattern_weight_model_id(&self) -> PatternWeightModelId {
        self.pattern_weight_model_id
    }

    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub fn rows(&self) -> &[CoverageRow] {
        &self.rows
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn into_incomplete(mut self) -> Self {
        self.complete = false;
        self
    }

    /// Recomputes the authoritative aggregate exclusively from the typed Build rows.
    pub fn coverage_union(&self) -> PatternBitSet {
        let mut union = PatternBitSet::new(self.pattern_count);
        for row in &self.rows {
            union
                .union_with(row.coverage_bits())
                .expect("constructor validated every row against the batch pattern count");
        }
        union
    }

    pub(crate) fn checked_non_pattern_storage_retained_bytes(&self) -> Option<u128> {
        let row_slots = (self.rows.capacity() as u128)
            .checked_mul(core::mem::size_of::<CoverageRow>() as u128)?;
        row_slots.checked_add(self.problem.checked_storage_retained_bytes()?)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PcChanceCoverageEvidenceError {
    Problem(PcChanceProblemEvidenceError),
    RowKindMismatch {
        row_index: usize,
    },
    PieceSourceMismatch {
        row_index: usize,
        expected: u64,
        actual: u64,
    },
    PatternUniverseMismatch {
        row_index: usize,
        expected: PatternUniverseId,
        actual: PatternUniverseId,
    },
    PatternWeightModelMismatch {
        row_index: usize,
        expected: PatternWeightModelId,
        actual: PatternWeightModelId,
    },
    PatternCountMismatch {
        row_index: usize,
        expected: usize,
        actual: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrictCoveragePatternWordsError {
    WordCountMismatch {
        expected: usize,
        actual: usize,
    },
    NonZeroPaddingBits {
        word_index: usize,
        invalid_bits: u64,
    },
    InvalidBitSet(PatternBitSetError),
}

/// Constructs a bitset from an external aggregate without canonicalizing malformed input.
///
/// `PatternBitSet::from_words` intentionally masks high padding bits. Product validation must
/// reject those bits before calling it so two distinct external encodings cannot collapse into
/// one valid aggregate.
pub fn strict_coverage_pattern_bitset_from_words(
    pattern_count: usize,
    words: &[u64],
) -> Result<PatternBitSet, StrictCoveragePatternWordsError> {
    let expected_word_count = pattern_count.div_ceil(u64::BITS as usize);
    if words.len() != expected_word_count {
        return Err(StrictCoveragePatternWordsError::WordCountMismatch {
            expected: expected_word_count,
            actual: words.len(),
        });
    }

    let remainder = pattern_count % u64::BITS as usize;
    if remainder != 0 {
        let allowed_mask = (1_u64 << remainder) - 1;
        let invalid_bits = words.last().copied().unwrap_or(0) & !allowed_mask;
        if invalid_bits != 0 {
            return Err(StrictCoveragePatternWordsError::NonZeroPaddingBits {
                word_index: expected_word_count - 1,
                invalid_bits,
            });
        }
    }

    PatternBitSet::from_words(pattern_count, words.to_vec())
        .map_err(StrictCoveragePatternWordsError::InvalidBitSet)
}

/// Canonical text for the versioned PC probability surface.
///
/// Legacy generic result fields retain their existing 12-digit presentation separately.
pub fn canonical_probability_v2(value: ProbabilityValue) -> String {
    match value.get() {
        0.0 => "0".to_owned(),
        1.0 => "1".to_owned(),
        value => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use clearra_coverage::pattern::pattern_id::PatternId;
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{
        PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    };
    use clearra_problem::{PcChanceEvidencePolicy, ProblemCompiler};
    use clearra_rules::profile::builtin_rules::{srs, srs_plus};
    use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

    use super::*;

    fn bare_problem() -> SearchProblem {
        let expression = QueuePatternExpression::parse("[IO]", 2).expect("two-pattern expression");
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0),
            PcQueueInput::pattern_expression(expression),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        ProblemCompiler::compile_scenario_pc(&query).expect("chance evidence problem")
    }

    fn problem() -> SearchProblem {
        bare_problem().with_pc_chance_probability_v2_evidence()
    }

    fn score_problem(rule: RuleProfile) -> SearchProblem {
        let expression = QueuePatternExpression::parse("[IO]", 2).expect("two-pattern expression");
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0),
            PcQueueInput::pattern_expression(expression),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_rule(rule)
        .with_objective(ObjectivePolicy::all().with_score_summary());
        ProblemCompiler::compile_scenario_pc(&query).expect("score evidence problem")
    }

    fn score_portfolio_problem(rule: RuleProfile) -> SearchProblem {
        let expression = QueuePatternExpression::parse("[IO]", 2)
            .expect("two-pattern score-portfolio expression");
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0),
            PcQueueInput::pattern_expression(expression),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_rule(rule)
        .with_objective(ObjectivePolicy::minimum_cover().with_score_summary());
        ProblemCompiler::compile_scenario_pc(&query)
            .expect("score-portfolio evidence problem")
            .with_pc_score_portfolio_v2_evidence()
    }

    fn identity(problem: &SearchProblem) -> (u64, PatternUniverseId, PatternWeightModelId, usize) {
        let universe = problem
            .piece_source()
            .materialized_universe()
            .expect("materialized test universe");
        (
            problem.piece_source().id().get(),
            universe.pattern_universe_id(),
            universe.pattern_weight_model_id(),
            universe.pattern_count(),
        )
    }

    fn row(
        candidate_id: u64,
        row_kind: CoverageRowKind,
        piece_source_id: u64,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
        covered: &[usize],
    ) -> CoverageRow {
        CoverageRow::new_with_piece_source(
            candidate_id,
            row_kind,
            piece_source_id,
            pattern_universe_id,
            pattern_weight_model_id,
            PatternBitSet::from_patterns(
                pattern_count,
                covered.iter().copied().map(PatternId::new),
            )
            .expect("test coverage"),
        )
    }

    #[test]
    fn score_problem_evidence_is_full_fieldwise_rule_bound_and_accounted() {
        let expected = score_problem(srs_plus());
        let foreign_rule = score_problem(srs());
        assert_eq!(expected.problem_id(), foreign_rule.problem_id());

        let evidence = PcScoreProblemEvidence::from_executed_problem(&expected)
            .expect("score problem evidence");
        assert!(evidence.matches_search_problem(&expected));
        assert!(!evidence.matches_search_problem(&foreign_rule));
        assert_ne!(evidence.kick_table_id(), 0);
        assert_ne!(evidence.rule_profile_id(), 0);
        assert!(evidence
            .checked_storage_retained_bytes()
            .is_some_and(|bytes| bytes > 0));
        assert!(matches!(
            PcScoreProblemEvidence::from_executed_problem(
                &expected.with_pc_chance_probability_v2_evidence()
            ),
            Err(PcChanceProblemEvidenceError::UnexpectedChanceEvidencePolicy)
        ));
    }

    #[test]
    fn score_portfolio_evidence_is_purpose_separated_and_retains_both_proofs() {
        let expected = score_portfolio_problem(srs_plus());
        let foreign_rule = score_portfolio_problem(srs());
        assert_eq!(expected.problem_id(), foreign_rule.problem_id());

        let coverage = PcChanceCoverageEvidence::from_problem_rows(&expected, Vec::new(), true)
            .expect("score-portfolio coverage evidence");
        let score = PcScoreProblemEvidence::from_executed_score_portfolio_problem(&expected)
            .expect("score-portfolio replay evidence");

        assert!(coverage.problem().matches_search_problem(&expected));
        assert!(score.matches_search_problem(&expected));
        assert!(!score.matches_search_problem(&foreign_rule));
        assert_ne!(score.kick_table_id(), 0);
        assert_ne!(score.rule_profile_id(), 0);
        assert!(score
            .checked_storage_retained_bytes()
            .is_some_and(|bytes| bytes > 0));
        assert!(matches!(
            PcScoreProblemEvidence::from_executed_problem(&expected),
            Err(PcChanceProblemEvidenceError::UnexpectedChanceEvidencePolicy)
        ));

        let score_only = score_problem(srs_plus());
        let score_only_evidence = PcScoreProblemEvidence::from_executed_problem(&score_only)
            .expect("score-only evidence");
        assert!(!score_only_evidence.matches_search_problem(&expected));
        assert!(!score.matches_search_problem(&score_only));

        let wrong_objective = bare_problem().with_pc_score_portfolio_v2_evidence();
        assert!(matches!(
            PcChanceCoverageEvidence::from_problem_rows(&wrong_objective, Vec::new(), true),
            Err(PcChanceCoverageEvidenceError::Problem(
                PcChanceProblemEvidenceError::UnexpectedScorePortfolioObjective
            ))
        ));
        assert!(matches!(
            PcScoreProblemEvidence::from_executed_score_portfolio_problem(&wrong_objective),
            Err(PcChanceProblemEvidenceError::UnexpectedScorePortfolioObjective)
        ));
    }

    #[test]
    fn typed_policy_is_explicit_id_separated_and_required_by_the_constructor() {
        let bare = bare_problem();
        let bare_id = bare.problem_id().as_str().to_owned();
        assert_eq!(
            bare.pc_chance_evidence_policy(),
            PcChanceEvidencePolicy::Disabled
        );
        assert!(matches!(
            PcChanceCoverageEvidence::from_problem_rows(&bare, Vec::new(), true),
            Err(PcChanceCoverageEvidenceError::Problem(
                PcChanceProblemEvidenceError::EvidencePolicyDisabled
            ))
        ));

        let typed = bare.clone().with_pc_chance_probability_v2_evidence();
        assert_eq!(
            typed.pc_chance_evidence_policy(),
            PcChanceEvidencePolicy::PcProbabilityV2
        );
        assert_eq!(
            typed.problem_id().as_str(),
            format!("{bare_id}:pc-probability-v2")
        );
        assert_ne!(typed.problem_id(), bare.problem_id());
        assert_ne!(typed, bare);
        assert_eq!(typed.piece_source().id(), bare.piece_source().id());
        assert_eq!(
            typed
                .clone()
                .with_pc_chance_probability_v2_evidence()
                .problem_id(),
            typed.problem_id()
        );
    }

    #[test]
    fn checkpoint_schedule_and_chain_class_mutations_do_not_match() {
        let problem = problem();
        let evidence = PcChanceCoverageEvidence::from_problem_rows(&problem, Vec::new(), true)
            .expect("typed problem snapshot");
        assert_eq!(evidence.problem().completion_goal(), problem.goal());
        assert!(problem.scenario().setup_query().is_none());
        assert!(problem.scenario().build_query().is_none());

        let schedule = CheckpointSchedule::for_opening_target(PcTarget::six_lines())
            .expect("opening checkpoint schedule");
        let mut schedule_evidence = PcChanceCheckpointScheduleEvidence::from_schedule(&schedule);
        assert!(schedule_evidence.matches(&schedule));
        schedule_evidence.partition_increments[0][0] = 4;
        assert!(!schedule_evidence.matches(&schedule));

        let mut foreign_schedule = evidence.problem().clone();
        foreign_schedule.checkpoint_schedule =
            Some(PcChanceCheckpointScheduleEvidence::from_schedule(&schedule));
        assert!(!foreign_schedule.matches_search_problem(&problem));

        let mut foreign_chain = evidence.problem().clone();
        foreign_chain.chain_class = ChainClass::Opening2L;
        assert!(!foreign_chain.matches_search_problem(&problem));
    }

    #[test]
    fn empty_batch_retains_its_full_identity_and_completeness() {
        let problem = problem();
        let (source_id, universe_id, weight_model_id, pattern_count) = identity(&problem);
        let evidence = PcChanceCoverageEvidence::from_problem_rows(&problem, Vec::new(), true)
            .expect("empty typed batch");

        assert_eq!(evidence.row_kind(), &CoverageRowKind::Build);
        assert_eq!(evidence.piece_source_id(), source_id);
        assert_eq!(evidence.pattern_universe_id(), universe_id);
        assert_eq!(evidence.pattern_weight_model_id(), weight_model_id);
        assert_eq!(evidence.pattern_count(), pattern_count);
        assert_eq!(evidence.row_count(), 0);
        assert!(evidence.complete());
        assert_eq!(evidence.coverage_union().words(), &[0]);
        assert!(evidence.problem().matches_search_problem(&problem));
        assert!(!evidence.problem().matches_search_problem(
            &problem
                .clone()
                .with_output_policy(SearchOutputPolicy::CoverageSummary)
        ));
    }

    #[test]
    fn typed_rows_recompute_the_exact_or_union() {
        let problem = problem();
        let (source_id, universe_id, weight_model_id, pattern_count) = identity(&problem);
        let rows = vec![
            row(
                1,
                CoverageRowKind::Build,
                source_id,
                universe_id,
                weight_model_id,
                pattern_count,
                &[0],
            ),
            row(
                2,
                CoverageRowKind::Build,
                source_id,
                universe_id,
                weight_model_id,
                pattern_count,
                &[1],
            ),
        ];
        let evidence = PcChanceCoverageEvidence::from_problem_rows(&problem, rows, false)
            .expect("typed batch");

        assert_eq!(evidence.row_count(), 2);
        assert!(!evidence.complete());
        assert_eq!(evidence.coverage_union().words(), &[0b11]);
    }

    #[test]
    fn same_universe_with_different_execution_semantics_is_not_the_same_problem() {
        let problem = problem();
        let foreign_query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0),
            PcQueueInput::pattern_expression(
                QueuePatternExpression::parse("[IO]", 2).expect("same two-pattern expression"),
            ),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_allow_hold(false)
        .with_rule(clearra_rules::profile::builtin_rules::srs_x())
        .with_objective(ObjectivePolicy::unique());
        let foreign = ProblemCompiler::compile_scenario_pc(&foreign_query)
            .expect("foreign problem")
            .with_pc_chance_probability_v2_evidence();
        let original_universe = problem
            .piece_source()
            .materialized_universe()
            .expect("original universe");
        let foreign_universe = foreign
            .piece_source()
            .materialized_universe()
            .expect("foreign universe");
        assert_eq!(
            original_universe.pattern_universe_id(),
            foreign_universe.pattern_universe_id()
        );
        assert_eq!(
            original_universe.pattern_weight_model_id(),
            foreign_universe.pattern_weight_model_id()
        );

        let evidence = PcChanceCoverageEvidence::from_problem_rows(&problem, Vec::new(), true)
            .expect("original evidence");
        assert!(!evidence.problem().matches_search_problem(&foreign));
    }

    #[test]
    fn constructor_rejects_every_row_identity_dimension() {
        let problem = problem();
        let (source_id, universe_id, weight_model_id, pattern_count) = identity(&problem);
        let cases = [
            row(
                1,
                CoverageRowKind::Pc,
                source_id,
                universe_id,
                weight_model_id,
                pattern_count,
                &[0],
            ),
            row(
                2,
                CoverageRowKind::Build,
                source_id + 1,
                universe_id,
                weight_model_id,
                pattern_count,
                &[0],
            ),
            row(
                3,
                CoverageRowKind::Build,
                source_id,
                PatternUniverseId::new(99),
                weight_model_id,
                pattern_count,
                &[0],
            ),
            row(
                4,
                CoverageRowKind::Build,
                source_id,
                universe_id,
                PatternWeightModelId::new(99),
                pattern_count,
                &[0],
            ),
            row(
                5,
                CoverageRowKind::Build,
                source_id,
                universe_id,
                weight_model_id,
                pattern_count - 1,
                &[0],
            ),
        ];

        let errors = cases.map(|bad_row| {
            PcChanceCoverageEvidence::from_problem_rows(&problem, vec![bad_row], true)
                .expect_err("identity mismatch")
        });
        assert!(matches!(
            errors[0],
            PcChanceCoverageEvidenceError::RowKindMismatch { row_index: 0 }
        ));
        assert!(matches!(
            errors[1],
            PcChanceCoverageEvidenceError::PieceSourceMismatch { row_index: 0, .. }
        ));
        assert!(matches!(
            errors[2],
            PcChanceCoverageEvidenceError::PatternUniverseMismatch { row_index: 0, .. }
        ));
        assert!(matches!(
            errors[3],
            PcChanceCoverageEvidenceError::PatternWeightModelMismatch { row_index: 0, .. }
        ));
        assert!(matches!(
            errors[4],
            PcChanceCoverageEvidenceError::PatternCountMismatch { row_index: 0, .. }
        ));
    }

    #[test]
    fn constructor_rejects_zero_batch_identity_even_without_rows() {
        let cases = [
            validate_problem_identity(0, PatternUniverseId::new(11), PatternWeightModelId::new(13))
                .expect_err("zero source"),
            validate_problem_identity(7, PatternUniverseId::new(0), PatternWeightModelId::new(13))
                .expect_err("zero universe"),
            validate_problem_identity(7, PatternUniverseId::new(11), PatternWeightModelId::new(0))
                .expect_err("zero weight model"),
        ];

        assert_eq!(cases[0], PcChanceProblemEvidenceError::ZeroPieceSourceId);
        assert_eq!(
            cases[1],
            PcChanceProblemEvidenceError::ZeroPatternUniverseId
        );
        assert_eq!(
            cases[2],
            PcChanceProblemEvidenceError::ZeroPatternWeightModelId
        );
    }

    #[test]
    fn strict_words_reject_length_and_high_padding_before_construction() {
        assert_eq!(
            strict_coverage_pattern_bitset_from_words(65, &[1]),
            Err(StrictCoveragePatternWordsError::WordCountMismatch {
                expected: 2,
                actual: 1,
            })
        );
        assert_eq!(
            strict_coverage_pattern_bitset_from_words(65, &[1, 2]),
            Err(StrictCoveragePatternWordsError::NonZeroPaddingBits {
                word_index: 1,
                invalid_bits: 2,
            })
        );
        assert_eq!(
            strict_coverage_pattern_bitset_from_words(0, &[0]),
            Err(StrictCoveragePatternWordsError::WordCountMismatch {
                expected: 0,
                actual: 1,
            })
        );

        let accepted =
            strict_coverage_pattern_bitset_from_words(65, &[1, 1]).expect("canonical final word");
        assert_eq!(accepted.words(), &[1, 1]);
    }

    #[test]
    fn v2_probability_text_uses_exact_endpoints_and_round_trip_f64_text() {
        assert_eq!(canonical_probability_v2(ProbabilityValue::ZERO), "0");
        assert_eq!(canonical_probability_v2(ProbabilityValue::ONE), "1");
        let third = ProbabilityValue::new(1.0 / 3.0).expect("probability");
        assert_eq!(canonical_probability_v2(third), (1.0_f64 / 3.0).to_string());
        assert_ne!(canonical_probability_v2(third), "0.333333333333");
    }
}
