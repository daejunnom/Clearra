pub use clearra_core_domain::field::occupancy_field::OccupancyField;

mod board_accessors {
    use clearra_pc_graph::request::PcScenarioBoard;
    use clearra_profiles::board::board_profile::BoardProfile;

    use super::{OccupancyField, SearchProblem, SearchProblemBoard};

    impl SearchProblem {
        pub fn board(&self) -> &SearchProblemBoard {
            &self.board
        }
    }
    impl SearchProblem {
        pub fn initial_occupancy(&self) -> Option<&OccupancyField> {
            self.initial_occupancy.as_ref()
        }
    }
    impl SearchProblem {
        pub fn initial_board(&self) -> &PcScenarioBoard {
            self.board.initial_board()
        }
    }
    impl SearchProblem {
        pub fn visible_height(&self) -> u16 {
            self.board.visible_height()
        }
    }
    impl SearchProblem {
        pub fn search_height(&self) -> u16 {
            self.board.search_height()
        }
    }
    impl SearchProblem {
        pub fn board_profile(&self) -> BoardProfile {
            self.board_profile
        }
    }
}
mod budget_policy {
    use crate::query::ScenarioQuery;

    use super::{SearchProblemBudget, SearchProblemPreset};

    pub(super) fn budget_for(
        preset: SearchProblemPreset,
        scenario: &ScenarioQuery,
    ) -> SearchProblemBudget {
        let defaults = SearchProblemBudget::default();
        match preset {
            SearchProblemPreset::Setup => scenario
                .setup_query()
                .map(|query| {
                    let limits = query.limits();
                    SearchProblemBudget::new(
                        defaults.max_nodes(),
                        defaults.max_seconds(),
                        limits.max_results(),
                        limits.max_patterns(),
                    )
                })
                .unwrap_or(defaults),
            SearchProblemPreset::Build => scenario
                .build_query()
                .map(|query| {
                    let limits = query.limits();
                    SearchProblemBudget::new(
                        defaults.max_nodes(),
                        defaults.max_seconds(),
                        limits.max_assignments(),
                        limits.max_patterns(),
                    )
                })
                .unwrap_or(defaults),
            SearchProblemPreset::OpeningPc | SearchProblemPreset::ScenarioPc => {
                let execution = scenario.core_query().execution_policy();
                SearchProblemBudget::new(
                    execution.max_nodes(),
                    defaults.max_seconds(),
                    defaults.max_results(),
                    execution.max_patterns(),
                )
            }
        }
    }
}
mod constructor {
    use clearra_rules::spawn::SpawnProfile;

    use crate::{compile::ProblemCompileError, query::ScenarioQuery};

    use super::{
        budget_policy::budget_for,
        output_policy::output_policy_for,
        piece_source_materializer::{
            initial_hold_automaton_for, piece_source_for, resolve_supply_window,
        },
        problem_identity::problem_id_for,
        search_goal_policy::search_goal_for,
        search_height_policy::search_height_for,
        ContinuationPolicy, ExactTargetPolicy, KickProfile, OccupancyField, RuleProfileSelection,
        SearchProblem, SearchProblemBoard, SearchProblemKind, SearchProblemPreset,
        SearchReplayTracePolicy, SupplyProvenance, TracePolicy,
    };

    impl SearchProblem {
        pub fn new(
            preset: SearchProblemPreset,
            scenario: ScenarioQuery,
        ) -> Result<Self, ProblemCompileError> {
            let core_query = scenario.core_query();
            let max_pieces = core_query.piece_window().max_pieces();
            if u16::try_from(max_pieces).is_err() {
                return Err(ProblemCompileError::PackingPieceWindowTooLarge { max_pieces });
            }
            let supply_window = resolve_supply_window(core_query)?;
            let problem_kind = SearchProblemKind::from(preset);
            let problem_id = problem_id_for(problem_kind, &scenario, supply_window);
            let search_height = search_height_for(preset, &scenario);
            let budget = budget_for(preset, &scenario);
            let board = SearchProblemBoard::new(core_query.initial_board().clone(), search_height);
            let initial_occupancy = OccupancyField::new(
                core_query.initial_board().width() as u8,
                core_query.initial_board().visible_height() as u8,
                core_query.initial_board().occupied_mask(),
            )
            .ok();
            let replay_trace_policy = SearchReplayTracePolicy::new(
                core_query.objective().trace_policy(),
                core_query.retained_trace_limit(),
            );
            let trace_policy = TracePolicy::new(replay_trace_policy);
            let continuation_policy =
                ContinuationPolicy::new(true, core_query.min_remaining_queue());
            let labels = scenario.labels().to_vec();
            let rule_profile = RuleProfileSelection::new(
                core_query.rule(),
                core_query.verified_kick_profile().cloned(),
                SpawnProfile::STANDARD_10,
            );
            let kick_profile = KickProfile::from_rule_selection(&rule_profile);
            let piece_source = piece_source_for(core_query, supply_window, budget.max_patterns())?;
            let initial_hold = initial_hold_automaton_for(core_query, &piece_source);

            Ok(Self {
                problem_id,
                problem_kind,
                preset,
                initial_occupancy,
                board,
                board_profile:
                    clearra_profiles::bundle::standard_profile_bundle::standard_profile_bundle()
                        .board(),
                piece_source,
                initial_hold,
                piece_window: core_query.piece_window(),
                exact_pieces: core_query.exact_pieces(),
                supply: SupplyProvenance::new(
                    core_query.remaining_queue().clone(),
                    core_query.hold_state(),
                    core_query.allow_hold(),
                    supply_window.source_sequence_length(),
                    supply_window.projects_unplaced_lookahead(),
                    core_query.bag(),
                ),
                piece_set: core_query.piece_set(),
                rule_profile,
                kick_profile,
                spawn_profile: SpawnProfile::STANDARD_10,
                search_goal: search_goal_for(preset, &scenario),
                exact_target_policy: ExactTargetPolicy::from_target(scenario.exact_target_policy()),
                count_policy: core_query.count_policy(),
                objective: core_query.objective(),
                budget,
                resource_budget: budget,
                backend_policy: core_query.execution_policy().clone(),
                output_policy: output_policy_for(preset),
                replay_trace_policy,
                trace_policy,
                continuation_policy,
                labels,
                scenario,
            })
        }
    }
}
mod execution_accessors {
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{
        PcCompletionGoal, PcExecutionPolicy, PcSolutionProbabilityPolicy,
    };

    use crate::goal::SearchGoal;

    use super::{
        BackendPolicy, ContinuationPolicy, CountPolicy, ExactTargetPolicy, ResourceBudget,
        SearchOutputPolicy, SearchProblem, SearchProblemBudget, SearchReplayTracePolicy,
        TracePolicy,
    };

    impl SearchProblem {
        pub fn goal(&self) -> PcCompletionGoal {
            self.search_goal.completion_goal()
        }
    }
    impl SearchProblem {
        pub fn search_goal(&self) -> &SearchGoal {
            &self.search_goal
        }
    }
    impl SearchProblem {
        pub fn with_search_goal(mut self, search_goal: SearchGoal) -> Self {
            self.search_goal = search_goal;
            self
        }
    }
    impl SearchProblem {
        pub fn exact_target_policy(&self) -> ExactTargetPolicy {
            self.exact_target_policy
        }
    }
    impl SearchProblem {
        pub fn count_policy(&self) -> CountPolicy {
            self.count_policy
        }
    }
    impl SearchProblem {
        pub fn objective(&self) -> ObjectivePolicy {
            self.objective
        }
    }
    impl SearchProblem {
        pub fn solution_probability_policy(&self) -> PcSolutionProbabilityPolicy {
            self.scenario.core_query().solution_probability_policy()
        }
    }
    impl SearchProblem {
        pub fn budget(&self) -> SearchProblemBudget {
            self.budget
        }
    }
    impl SearchProblem {
        pub fn backend_request(&self) -> &PcExecutionPolicy {
            &self.backend_policy
        }
    }
    impl SearchProblem {
        pub fn backend_policy(&self) -> &BackendPolicy {
            &self.backend_policy
        }
    }
    impl SearchProblem {
        pub fn resource_budget(&self) -> ResourceBudget {
            self.resource_budget
        }
    }
    impl SearchProblem {
        pub fn output_policy(&self) -> SearchOutputPolicy {
            self.output_policy
        }
    }
    impl SearchProblem {
        pub fn replay_trace_policy(&self) -> SearchReplayTracePolicy {
            self.replay_trace_policy
        }
    }
    impl SearchProblem {
        pub fn trace_policy(&self) -> TracePolicy {
            self.trace_policy
        }
    }
    impl SearchProblem {
        pub fn continuation_policy(&self) -> ContinuationPolicy {
            self.continuation_policy
        }
    }
}
mod identity_accessors {
    use clearra_pc_graph::{classification::ChainClass, dag::CheckpointSchedule};

    use super::{SearchProblem, SearchProblemId, SearchProblemKind, SearchProblemPreset};

    impl SearchProblem {
        pub fn problem_id(&self) -> &SearchProblemId {
            &self.problem_id
        }
    }
    impl SearchProblem {
        pub fn problem_kind(&self) -> SearchProblemKind {
            self.problem_kind
        }
    }
    impl SearchProblem {
        pub fn preset(&self) -> SearchProblemPreset {
            self.preset
        }
    }
    impl SearchProblem {
        pub fn labels(&self) -> &[String] {
            &self.labels
        }
    }
    impl SearchProblem {
        pub fn checkpoint_schedule(&self) -> Option<&CheckpointSchedule> {
            self.scenario.checkpoint_schedule()
        }
    }
    impl SearchProblem {
        pub fn chain_class(&self) -> ChainClass {
            self.scenario.chain_class()
        }
    }
}
mod model {
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::PieceWindow;
    use clearra_profiles::{
        board::board_profile::BoardProfile, pieces::piece_set_profile::PieceSetProfile,
    };
    use clearra_rules::spawn::SpawnProfile;

    use crate::{goal::SearchGoal, query::ScenarioQuery};

    use super::{
        BackendPolicy, ContinuationPolicy, CountPolicy, ExactTargetPolicy, HoldAutomatonState,
        KickProfile, OccupancyField, PieceSource, ResourceBudget, RuleProfileSelection,
        SearchOutputPolicy, SearchProblemBoard, SearchProblemBudget, SearchProblemId,
        SearchProblemKind, SearchProblemPreset, SearchReplayTracePolicy, SupplyProvenance,
        TracePolicy,
    };

    #[derive(Clone, Debug, PartialEq)]
    pub struct SearchProblem {
        pub(super) problem_id: SearchProblemId,
        pub(super) problem_kind: SearchProblemKind,
        pub(super) preset: SearchProblemPreset,
        pub(super) scenario: ScenarioQuery,
        pub(super) initial_occupancy: Option<OccupancyField>,
        pub(super) board: SearchProblemBoard,
        pub(super) board_profile: BoardProfile,
        pub(super) piece_source: PieceSource,
        pub(super) initial_hold: HoldAutomatonState,
        pub(super) piece_window: PieceWindow,
        pub(super) exact_pieces: Option<usize>,
        pub(super) supply: SupplyProvenance,
        pub(super) piece_set: PieceSetProfile,
        pub(super) rule_profile: RuleProfileSelection,
        pub(super) kick_profile: KickProfile,
        pub(super) spawn_profile: SpawnProfile,
        pub(super) search_goal: SearchGoal,
        pub(super) exact_target_policy: ExactTargetPolicy,
        pub(super) count_policy: CountPolicy,
        pub(super) objective: ObjectivePolicy,
        pub(super) budget: SearchProblemBudget,
        pub(super) resource_budget: ResourceBudget,
        pub(super) backend_policy: BackendPolicy,
        pub(super) output_policy: SearchOutputPolicy,
        pub(super) replay_trace_policy: SearchReplayTracePolicy,
        pub(super) trace_policy: TracePolicy,
        pub(super) continuation_policy: ContinuationPolicy,
        pub(super) labels: Vec<String>,
    }
}
mod output_policy {
    use super::{SearchOutputPolicy, SearchProblemPreset};

    pub(super) fn output_policy_for(preset: SearchProblemPreset) -> SearchOutputPolicy {
        match preset {
            SearchProblemPreset::OpeningPc | SearchProblemPreset::ScenarioPc => {
                SearchOutputPolicy::Trace
            }
            SearchProblemPreset::Setup => SearchOutputPolicy::Summary,
            SearchProblemPreset::Build => SearchOutputPolicy::CoverageRows,
        }
    }
}
mod piece_source_materializer {
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioQuery};
    use clearra_supply::{
        bag::BagState,
        hold_automaton::SupplyProvenanceId,
        mixed::supply_provenance::{BagBoundaryEvidence, SupplyProvenance},
    };

    use crate::compile::ProblemCompileError;

    use super::{HoldAutomatonState, PieceSource};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct ResolvedSupplyWindow {
        source_sequence_length: usize,
        projects_unplaced_lookahead: bool,
    }

    impl ResolvedSupplyWindow {
        pub(super) const fn source_sequence_length(self) -> usize {
            self.source_sequence_length
        }

        pub(super) const fn projects_unplaced_lookahead(self) -> bool {
            self.projects_unplaced_lookahead
        }
    }

    pub(super) fn resolve_supply_window(
        query: &PcScenarioQuery,
    ) -> Result<ResolvedSupplyWindow, ProblemCompileError> {
        let geometry_piece_count = query
            .exact_pieces()
            .unwrap_or_else(|| query.piece_window().max_pieces());
        let initial_hold_piece_count =
            usize::from(query.allow_hold() && query.hold_state().piece().is_some());
        let required_source_pieces = geometry_piece_count.saturating_sub(initial_hold_piece_count);
        let automatic_lookahead = usize::from(query.allow_hold());
        let automatic_source_pieces = geometry_piece_count
            .saturating_add(automatic_lookahead)
            .saturating_sub(initial_hold_piece_count);
        let requested_source_pieces = query
            .supply_window_size()
            .map(|window| window.source_pieces());

        let source_sequence_length = match query.remaining_queue() {
            PcQueueInput::FixedSequence(sequence) => {
                let queue_pieces = sequence.len();
                if requested_source_pieces.is_some_and(|requested| requested > queue_pieces) {
                    return Err(
                        ProblemCompileError::SupplyWindowConflictsWithConcreteQueue {
                            source_pieces: requested_source_pieces.unwrap_or_default(),
                            queue_pieces,
                        },
                    );
                }
                requested_source_pieces
                    .unwrap_or(automatic_source_pieces)
                    .min(queue_pieces)
            }
            PcQueueInput::BagAlignedPattern(pattern) => {
                let queue_pieces = pattern.len();
                if requested_source_pieces.is_some_and(|requested| requested > queue_pieces) {
                    return Err(
                        ProblemCompileError::SupplyWindowConflictsWithConcreteQueue {
                            source_pieces: requested_source_pieces.unwrap_or_default(),
                            queue_pieces,
                        },
                    );
                }
                requested_source_pieces
                    .unwrap_or(automatic_source_pieces)
                    .min(queue_pieces)
            }
            PcQueueInput::PatternExpression(expression) => {
                let queue_pieces = expression.sequence_len();
                if requested_source_pieces.is_some_and(|requested| requested > queue_pieces) {
                    return Err(
                        ProblemCompileError::SupplyWindowConflictsWithConcreteQueue {
                            source_pieces: requested_source_pieces.unwrap_or_default(),
                            queue_pieces,
                        },
                    );
                }
                requested_source_pieces
                    .unwrap_or(automatic_source_pieces)
                    .min(queue_pieces)
            }
            PcQueueInput::Observed(observed) => {
                let observed_pieces = observed.len();
                if requested_source_pieces.is_some_and(|requested| requested < observed_pieces) {
                    return Err(ProblemCompileError::SupplyWindowShorterThanObservedQueue {
                        source_pieces: requested_source_pieces.unwrap_or_default(),
                        observed_pieces,
                    });
                }
                requested_source_pieces
                    .unwrap_or(automatic_source_pieces)
                    .max(observed_pieces)
            }
            PcQueueInput::Standard7Bag => {
                requested_source_pieces.unwrap_or(automatic_source_pieces)
            }
        };

        if source_sequence_length < required_source_pieces {
            return Err(ProblemCompileError::SupplyWindowTooShort {
                source_pieces: source_sequence_length,
                required_source_pieces,
            });
        }

        let projects_unplaced_lookahead =
            matches!(query.remaining_queue(), PcQueueInput::Standard7Bag)
                && query.allow_hold()
                && query.exact_pieces() == Some(geometry_piece_count)
                && source_sequence_length == required_source_pieces;

        Ok(ResolvedSupplyWindow {
            source_sequence_length,
            projects_unplaced_lookahead,
        })
    }

    pub(super) fn piece_source_for(
        query: &PcScenarioQuery,
        supply_window: ResolvedSupplyWindow,
        max_patterns: usize,
    ) -> Result<PieceSource, ProblemCompileError> {
        let provenance = SupplyProvenance::new(
            query.bag().id().as_str(),
            query.piece_set().id().as_str(),
            observed_window_id(query.remaining_queue()),
            bag_boundary_evidence(query.remaining_queue()),
            false,
            matches!(query.remaining_queue(), PcQueueInput::Observed(_)),
        )
        .expect("validated standard supply provenance");

        match query.remaining_queue() {
            PcQueueInput::FixedSequence(sequence) => {
                let pieces = sequence.pieces()[..supply_window.source_sequence_length()].to_vec();
                Ok(PieceSource::fixed_queue(
                    clearra_supply::queue::fixed_sequence::FixedSequence::new(pieces),
                    provenance,
                ))
            }
            PcQueueInput::BagAlignedPattern(pattern) => {
                let pieces = pattern.pieces()[..supply_window.source_sequence_length()].to_vec();
                Ok(PieceSource::bag_universe(
                    clearra_supply::queue::bag_aligned_pattern::BagAlignedPattern::new(pieces),
                    provenance,
                ))
            }
            PcQueueInput::PatternExpression(expression) => PieceSource::queue_pattern_expression(
                expression.prefix(supply_window.source_sequence_length()),
                provenance,
            ),
            PcQueueInput::Standard7Bag => PieceSource::standard_7_bag(
                provenance,
                supply_window.source_sequence_length(),
                max_patterns,
            ),
            PcQueueInput::Observed(observed) => PieceSource::observed_window(
                observed.clone(),
                provenance,
                supply_window.source_sequence_length(),
                max_patterns,
            ),
        }
        .map_err(crate::compile::ProblemCompileError::PatternUniverseMaterialization)
    }

    pub(super) fn initial_hold_automaton_for(
        query: &PcScenarioQuery,
        piece_source: &PieceSource,
    ) -> HoldAutomatonState {
        let hold_piece = query
            .allow_hold()
            .then(|| query.hold_state().piece())
            .flatten();
        let bag_state = matches!(query.remaining_queue(), PcQueueInput::Standard7Bag)
            .then(BagState::fresh_standard_7_bag);
        HoldAutomatonState::new(
            piece_source.id(),
            bag_state.map_or(0, BagState::generated_count),
            hold_piece,
            bag_state.map_or(0, BagState::epoch),
            bag_state.map_or(0, BagState::packed_remainder_key),
            SupplyProvenanceId(piece_source.provenance().supply_provenance_id()),
        )
    }

    fn observed_window_id(queue: &PcQueueInput) -> Option<String> {
        match queue {
            PcQueueInput::Observed(observed) => Some(format!(
                "observed:{}:{}:{}",
                observed.len(),
                queue.mode(),
                observed
                    .pieces()
                    .iter()
                    .map(|piece| piece.as_ascii())
                    .collect::<String>()
            )),
            PcQueueInput::FixedSequence(_)
            | PcQueueInput::BagAlignedPattern(_)
            | PcQueueInput::PatternExpression(_)
            | PcQueueInput::Standard7Bag => None,
        }
    }

    fn bag_boundary_evidence(queue: &PcQueueInput) -> BagBoundaryEvidence {
        match queue {
            PcQueueInput::FixedSequence(_)
            | PcQueueInput::BagAlignedPattern(_)
            | PcQueueInput::PatternExpression(_)
            | PcQueueInput::Standard7Bag => BagBoundaryEvidence::FixedBoundary,
            PcQueueInput::Observed(_) => BagBoundaryEvidence::ObservedCompatible,
        }
    }
}
mod preset {
    use super::SearchProblemKind;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum SearchProblemPreset {
        OpeningPc,
        ScenarioPc,
        Setup,
        Build,
    }

    impl SearchProblemPreset {
        pub fn as_str(self) -> &'static str {
            match self {
                Self::OpeningPc => "opening-pc",
                Self::ScenarioPc => "scenario-pc",
                Self::Setup => "setup",
                Self::Build => "build",
            }
        }
    }

    impl From<SearchProblemPreset> for SearchProblemKind {
        fn from(preset: SearchProblemPreset) -> Self {
            match preset {
                SearchProblemPreset::OpeningPc => Self::OpeningPc,
                SearchProblemPreset::ScenarioPc => Self::ScenarioPc,
                SearchProblemPreset::Setup => Self::SetupPostPc,
                SearchProblemPreset::Build => Self::BuildCoverage,
            }
        }
    }
}
mod problem_identity {
    use crate::query::ScenarioQuery;

    use super::{
        piece_source_materializer::ResolvedSupplyWindow, SearchProblemId, SearchProblemKind,
    };

    pub(super) fn problem_id_for(
        kind: SearchProblemKind,
        scenario: &ScenarioQuery,
        supply_window: ResolvedSupplyWindow,
    ) -> SearchProblemId {
        let core = scenario.core_query();
        SearchProblemId::new(format!(
            "{}:{}:{}x{}:{:016x}:{}:{}:{}:{}:{}",
            kind.as_str(),
            scenario.source().as_str(),
            core.initial_board().width(),
            core.initial_board().visible_height(),
            core.initial_board().occupied_mask(),
            core.remaining_queue().mode(),
            core.piece_window().max_pieces(),
            core.exact_pieces().unwrap_or(0),
            supply_window.source_sequence_length(),
            supply_window.projects_unplaced_lookahead()
        ))
    }
}
mod query_accessors {
    use clearra_pc_graph::request::PcScenarioQuery;

    use crate::query::{BuildQuery, ScenarioQuery, SetupSearchQuery};

    use super::SearchProblem;

    impl SearchProblem {
        pub fn scenario(&self) -> &ScenarioQuery {
            &self.scenario
        }
    }
    impl SearchProblem {
        pub fn setup_query(&self) -> Option<&SetupSearchQuery> {
            self.scenario.setup_query()
        }
    }
    impl SearchProblem {
        pub fn build_query(&self) -> Option<&BuildQuery> {
            self.scenario.build_query()
        }
    }
    impl SearchProblem {
        pub fn core_query(&self) -> &PcScenarioQuery {
            self.scenario.core_query()
        }
    }
}
mod rule_accessors {
    use clearra_rules::{profile::rule_profile::RuleProfile, spawn::SpawnProfile};

    use super::{KickProfile, RuleProfileSelection, SearchProblem};

    impl SearchProblem {
        pub fn rule_profile(&self) -> &RuleProfileSelection {
            &self.rule_profile
        }
    }
    impl SearchProblem {
        pub fn rule_profile_value(&self) -> RuleProfile {
            self.rule_profile.rule()
        }
    }
    impl SearchProblem {
        pub fn kick_profile(&self) -> KickProfile {
            self.kick_profile
        }
    }
    impl SearchProblem {
        pub fn spawn_profile(&self) -> SpawnProfile {
            self.spawn_profile
        }
    }
}
mod search_goal_policy {
    use crate::{
        goal::{BuildTemplateGoal, SearchGoal},
        query::ScenarioQuery,
    };

    use super::SearchProblemPreset;

    pub(super) fn search_goal_for(
        preset: SearchProblemPreset,
        scenario: &ScenarioQuery,
    ) -> SearchGoal {
        match preset {
            SearchProblemPreset::Build => scenario
                .build_query()
                .map(|query| {
                    SearchGoal::BuildTemplate(BuildTemplateGoal::new(query.template().id()))
                })
                .unwrap_or(SearchGoal::ClearToEmpty),
            SearchProblemPreset::OpeningPc
            | SearchProblemPreset::ScenarioPc
            | SearchProblemPreset::Setup => SearchGoal::ClearToEmpty,
        }
    }
}
mod search_height_policy {
    use crate::query::ScenarioQuery;

    use super::SearchProblemPreset;

    pub(super) fn search_height_for(preset: SearchProblemPreset, scenario: &ScenarioQuery) -> u16 {
        match preset {
            SearchProblemPreset::OpeningPc => scenario
                .pc_query()
                .map(|query| query.board().size().height())
                .unwrap_or_else(|| scenario.initial_board().visible_height()),
            SearchProblemPreset::ScenarioPc => scenario.initial_board().visible_height(),
            SearchProblemPreset::Setup => scenario
                .setup_query()
                .map(|query| query.board_size().height())
                .unwrap_or_else(|| scenario.initial_board().visible_height()),
            SearchProblemPreset::Build => scenario
                .build_query()
                .map(|query| query.template().board_size().height())
                .unwrap_or_else(|| scenario.initial_board().visible_height()),
        }
    }
}
mod supply_accessors {
    use clearra_pc_graph::request::PieceWindow;
    use clearra_profiles::pieces::piece_set_profile::PieceSetProfile;

    use super::{HoldAutomatonState, PieceSource, SearchProblem, SupplyProvenance};

    impl SearchProblem {
        pub fn piece_window(&self) -> PieceWindow {
            self.piece_window
        }
    }
    impl SearchProblem {
        pub fn piece_source(&self) -> &PieceSource {
            &self.piece_source
        }
    }
    impl SearchProblem {
        pub fn initial_hold(&self) -> HoldAutomatonState {
            self.initial_hold
        }
    }
    impl SearchProblem {
        pub fn exact_pieces(&self) -> Option<usize> {
            self.exact_pieces
        }
    }
    impl SearchProblem {
        pub fn supply(&self) -> &SupplyProvenance {
            &self.supply
        }
    }
    impl SearchProblem {
        pub fn piece_set(&self) -> PieceSetProfile {
            self.piece_set
        }
    }
}

pub use model::SearchProblem;
pub use preset::SearchProblemPreset;

pub use crate::search_problem_fields::{
    BackendPolicy, ContinuationPolicy, CountPolicy, ExactTargetPolicy, HoldAutomatonState,
    KickProfile, PieceSource, ResourceBudget, RuleProfileSelection, SearchOutputPolicy,
    SearchProblemBoard, SearchProblemBudget, SearchProblemId, SearchProblemKind,
    SearchReplayTracePolicy, SupplyProvenance, TracePolicy,
};
