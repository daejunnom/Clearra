// SRP rationale: this module has one change reason: the validated canonical search-problem model.
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
    use clearra_core_domain::solution::StandardBoard64ColoredTilingIdentity;
    use clearra_pc_graph::request::{validate_pc_observation_objective, PcQueueInput};
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
        ContinuationPolicy, ExactTargetPolicy, KickProfile, OccupancyField, PcChanceEvidencePolicy,
        PieceSource, RuleProfileSelection, SearchProblem, SearchProblemBoard, SearchProblemId,
        SearchProblemKind, SearchProblemPreset, SearchReplayTracePolicy, SupplyProvenance,
        TracePolicy,
    };

    impl SearchProblem {
        pub fn new(
            preset: SearchProblemPreset,
            scenario: ScenarioQuery,
        ) -> Result<Self, ProblemCompileError> {
            let core_query = scenario.core_query();
            validate_pc_observation_objective(
                core_query.queue_observation_policy(),
                core_query.objective().kind(),
            )
            .map_err(ProblemCompileError::PcSearchContract)?;
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
            let allowed_colored_solution_identities: Option<
                Vec<StandardBoard64ColoredTilingIdentity>,
            > = core_query
                .allowed_colored_solution_identities()
                .map(ToOwned::to_owned);
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
                pc_chance_evidence_policy: PcChanceEvidencePolicy::Disabled,
                replay_trace_policy,
                trace_policy,
                continuation_policy,
                labels,
                allowed_colored_solution_identities,
                scenario,
            })
        }

        /// Assembles the already-validated canonical scenario-PC problem from
        /// owners materialized under the finite compiler's authority.
        ///
        /// This path deliberately performs no heap allocation: the scenario
        /// query and both supply owners are moved, while every remaining field
        /// is inline, static, or an empty `Vec`. The ordinary borrowed
        /// constructor remains the compatibility path for all other callers.
        /// This crate-private seam is intentionally infallible so it introduces
        /// no second `Result<SearchProblem, _>` carrier after validation.
        pub(crate) fn from_validated_finite_scenario_parts(
            scenario: ScenarioQuery,
            problem_id: SearchProblemId,
            piece_source: PieceSource,
            retained_queue: PcQueueInput,
            source_sequence_length: usize,
            projects_unplaced_lookahead: bool,
            allowed_colored_solution_identities: Option<Vec<StandardBoard64ColoredTilingIdentity>>,
        ) -> Self {
            let preset = SearchProblemPreset::ScenarioPc;
            let problem_kind = SearchProblemKind::ScenarioPc;
            let core_query = scenario.core_query();
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
            let rule_profile = RuleProfileSelection::new(
                core_query.rule(),
                core_query.verified_kick_profile().cloned(),
                SpawnProfile::STANDARD_10,
            );
            let kick_profile = KickProfile::from_rule_selection(&rule_profile);
            let initial_hold = initial_hold_automaton_for(core_query, &piece_source);
            let supply = SupplyProvenance::new(
                retained_queue,
                core_query.hold_state(),
                core_query.allow_hold(),
                source_sequence_length,
                projects_unplaced_lookahead,
                core_query.bag(),
            );
            let search_goal = search_goal_for(preset, &scenario);
            let exact_target_policy =
                ExactTargetPolicy::from_target(scenario.exact_target_policy());
            let piece_window = core_query.piece_window();
            let exact_pieces = core_query.exact_pieces();
            let piece_set = core_query.piece_set();
            let count_policy = core_query.count_policy();
            let objective = core_query.objective();
            let backend_policy = core_query.execution_policy().clone();

            Self {
                problem_id,
                problem_kind,
                preset,
                scenario,
                initial_occupancy,
                board,
                board_profile:
                    clearra_profiles::bundle::standard_profile_bundle::standard_profile_bundle()
                        .board(),
                piece_source,
                initial_hold,
                piece_window,
                exact_pieces,
                supply,
                piece_set,
                rule_profile,
                kick_profile,
                spawn_profile: SpawnProfile::STANDARD_10,
                search_goal,
                exact_target_policy,
                count_policy,
                objective,
                budget,
                resource_budget: budget,
                backend_policy,
                output_policy: output_policy_for(preset),
                pc_chance_evidence_policy: PcChanceEvidencePolicy::Disabled,
                replay_trace_policy,
                trace_policy,
                continuation_policy,
                labels: Vec::new(),
                allowed_colored_solution_identities,
            }
        }
    }
}
mod execution_accessors {
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{
        PcCompletionGoal, PcExecutionPolicy, PcSolutionProbabilityPolicy,
    };
    use clearra_supply::QueueObservationPolicy;

    use crate::goal::SearchGoal;

    use super::{
        BackendPolicy, ContinuationPolicy, CountPolicy, ExactTargetPolicy, PcChanceEvidencePolicy,
        ResourceBudget, SearchOutputPolicy, SearchProblem, SearchProblemBudget, SearchProblemId,
        SearchReplayTracePolicy, TracePolicy,
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
        pub fn queue_observation_policy(&self) -> QueueObservationPolicy {
            self.scenario
                .pc_query()
                .map(crate::query::pc_query::PcQuery::queue_observation_policy)
                .or_else(|| {
                    self.scenario
                        .setup_query()
                        .map(crate::query::setup_query::SetupSearchQuery::queue_observation_policy)
                })
                .unwrap_or_else(|| self.scenario.core_query().queue_observation_policy())
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
        pub fn with_output_policy(mut self, output_policy: SearchOutputPolicy) -> Self {
            self.output_policy = output_policy;
            self
        }
    }
    impl SearchProblem {
        pub fn pc_chance_evidence_policy(&self) -> PcChanceEvidencePolicy {
            self.pc_chance_evidence_policy
        }

        /// Opts an already compiled problem into the private pc-probability.v2
        /// evidence contract without changing any generic Percent compiler.
        pub fn with_pc_chance_probability_v2_evidence(mut self) -> Self {
            if self.pc_chance_evidence_policy != PcChanceEvidencePolicy::PcProbabilityV2 {
                self.pc_chance_evidence_policy = PcChanceEvidencePolicy::PcProbabilityV2;
                self.problem_id =
                    SearchProblemId::new(format!("{}:pc-probability-v2", self.problem_id.as_str()));
            }
            self
        }

        /// Opts an already compiled problem into the private
        /// `pc-minimum-cover.v2` execution-evidence contract.
        pub fn with_pc_minimum_cover_v2_evidence(mut self) -> Self {
            if !self
                .pc_chance_evidence_policy
                .retains_pc_minimum_cover_v2_evidence()
            {
                self.pc_chance_evidence_policy = PcChanceEvidencePolicy::PcMinimumCoverV2;
                self.problem_id = SearchProblemId::new(format!(
                    "{}:pc-minimum-cover-v2",
                    self.problem_id.as_str()
                ));
            }
            self
        }

        /// Opts an already compiled problem into the private, purpose-separated
        /// `pc-score-portfolio.v2` evidence contract. The policy enables both
        /// exact minimum-cover rows and score replay identity without making
        /// either single-purpose evidence mode interchangeable with this one.
        pub fn with_pc_score_portfolio_v2_evidence(mut self) -> Self {
            if !self
                .pc_chance_evidence_policy
                .retains_pc_score_portfolio_v2_evidence()
            {
                self.pc_chance_evidence_policy = PcChanceEvidencePolicy::PcScorePortfolioV2;
                self.problem_id = SearchProblemId::new(format!(
                    "{}:pc-score-portfolio-v2",
                    self.problem_id.as_str()
                ));
            }
            self
        }

        /// Opts an already compiled problem into the private terminal-supply
        /// evidence contract shared by `pc.saves` and `pc.best-save`.
        pub fn with_pc_save_groups_v2_evidence(mut self) -> Self {
            if !self
                .pc_chance_evidence_policy
                .retains_pc_save_groups_v2_evidence()
            {
                self.pc_chance_evidence_policy = PcChanceEvidencePolicy::PcSaveGroupsV2;
                self.problem_id =
                    SearchProblemId::new(format!("{}:pc-save-groups-v2", self.problem_id.as_str()));
            }
            self
        }

        /// Opts an already compiled problem into the private, purpose-separated
        /// `pc-path-family.v2` replay-evidence contract.
        pub fn with_pc_path_v2_evidence(mut self) -> Self {
            if !self.pc_chance_evidence_policy.retains_pc_path_v2_evidence() {
                self.pc_chance_evidence_policy = PcChanceEvidencePolicy::PcPathV2;
                self.problem_id =
                    SearchProblemId::new(format!("{}:pc-path-v2", self.problem_id.as_str()));
            }
            self
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
    use clearra_core_domain::solution::{
        StandardBoard64ColoredTilingIdentity, StandardBoard64TilingIdentity,
    };
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

        pub fn allowed_colored_solution_identities(
            &self,
        ) -> Option<&[StandardBoard64ColoredTilingIdentity]> {
            self.allowed_colored_solution_identities
                .as_deref()
                .or_else(|| {
                    self.scenario
                        .core_query()
                        .allowed_colored_solution_identities()
                })
        }

        pub fn allows_solution_identity(&self, identity: &StandardBoard64TilingIdentity) -> bool {
            self.allowed_colored_solution_identities()
                .is_none_or(|allowed| {
                    let colored =
                        StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(
                            *identity,
                        );
                    allowed.binary_search(&colored).is_ok()
                })
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
    use clearra_core_domain::solution::StandardBoard64ColoredTilingIdentity;
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::PieceWindow;
    use clearra_profiles::{
        board::board_profile::BoardProfile, pieces::piece_set_profile::PieceSetProfile,
    };
    use clearra_rules::spawn::SpawnProfile;

    use crate::{goal::SearchGoal, query::ScenarioQuery};

    use super::{
        BackendPolicy, ContinuationPolicy, CountPolicy, ExactTargetPolicy, HoldAutomatonState,
        KickProfile, OccupancyField, PcChanceEvidencePolicy, PieceSource, ResourceBudget,
        RuleProfileSelection, SearchOutputPolicy, SearchProblemBoard, SearchProblemBudget,
        SearchProblemId, SearchProblemKind, SearchProblemPreset, SearchReplayTracePolicy,
        SupplyProvenance, TracePolicy,
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
        pub(super) pc_chance_evidence_policy: PcChanceEvidencePolicy,
        pub(super) replay_trace_policy: SearchReplayTracePolicy,
        pub(super) trace_policy: TracePolicy,
        pub(super) continuation_policy: ContinuationPolicy,
        pub(super) labels: Vec<String>,
        pub(super) allowed_colored_solution_identities:
            Option<Vec<StandardBoard64ColoredTilingIdentity>>,
    }
}
mod retained_capacity {
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;

    use crate::{goal::SearchGoal, query::scenario_query::ScenarioQuerySource};

    use super::{
        CountPolicy, PcChanceEvidencePolicy, SearchOutputPolicy, SearchProblem, SearchProblemKind,
        SearchProblemPreset,
    };

    impl SearchProblem {
        /// Returns the complete retained byte count of this compiled
        /// `SearchProblem` pointee when it has the finite scenario shape
        /// consumed by BuildProbability.
        ///
        /// This includes `size_of::<SearchProblem>()` once and every nested
        /// heap payload owned by the problem. Callers that already retain the
        /// inline `SearchProblem` in an outer allocation must subtract that
        /// one inline size and add only the nested remainder.
        ///
        /// Opening, setup, template-Build, imported-kick, and non-canonical
        /// output/evidence shapes return `None` rather than silently omitting
        /// an unmeasured owner. A supplied colored-solution allow-list is an
        /// actual Build owner and is counted here by allocation capacity.
        pub fn checked_build_probability_pointee_retained_bytes(&self) -> Option<u128> {
            if self.scenario.core_query().verified_kick_profile().is_some()
                || self.rule_profile.verified_kick_profile().is_some()
                || !matches!(self.search_goal, SearchGoal::ClearToEmpty)
                || self.output_policy != SearchOutputPolicy::Trace
                || self.pc_chance_evidence_policy != PcChanceEvidencePolicy::Disabled
                || !matches!(
                    (self.preset, self.problem_kind, self.scenario.source()),
                    (
                        SearchProblemPreset::ScenarioPc,
                        SearchProblemKind::ScenarioPc,
                        ScenarioQuerySource::ScenarioPreset,
                    )
                )
            {
                return None;
            }

            let mut bytes = core::mem::size_of::<SearchProblem>() as u128;
            bytes = bytes.checked_add(self.problem_id.checked_retained_capacity_bytes()?)?;
            bytes = bytes.checked_add(
                self.scenario
                    .checked_build_probability_retained_capacity_bytes()?,
            )?;
            bytes = bytes.checked_add(self.piece_source.checked_retained_capacity_bytes()?)?;
            bytes = bytes.checked_add(
                self.supply
                    .checked_build_probability_retained_capacity_bytes()?,
            )?;
            bytes = bytes.checked_add(checked_string_vec_retained_capacity_bytes(&self.labels)?)?;
            bytes = bytes.checked_add(
                self.allowed_colored_solution_identities
                    .as_ref()
                    .map_or(Some(0), checked_inline_vec_retained_capacity_bytes)?,
            )?;
            Some(bytes)
        }

        /// Returns the complete retained byte count of this compiled
        /// `SearchProblem` pointee when it has a typed `pc.score` shape.
        ///
        /// This includes `size_of::<SearchProblem>()` once and every nested
        /// heap payload owned by the problem: the problem-id string, compiled
        /// scenario queues/labels/checkpoints, the derived piece source,
        /// supply provenance queue, and the problem-label clone. Heap buffers
        /// are measured by allocation capacity. The caller's `Arc` handle,
        /// allocator metadata/alignment, and the `Arc` control block are
        /// excluded. A pointer-identical pointee must therefore call this
        /// method only once even when several external `Arc` handles retain it.
        ///
        /// Unsupported or malformed preset decomposition, setup/build state,
        /// imported kick profiles, caller-selected solution identities,
        /// non-score queue kinds, and non-score-owned goal/policy variants
        /// return `None` rather than leaving an unmeasured owner out of the
        /// bound.
        pub fn checked_pc_score_pointee_retained_bytes(&self) -> Option<u128> {
            let evidence_policy = self.pc_chance_evidence_policy;
            let evidence_policy_supported = evidence_policy == PcChanceEvidencePolicy::Disabled
                || (evidence_policy.retains_pc_score_portfolio_v2_evidence()
                    && self.objective.kind()
                        == clearra_core_domain::objective::objective_kind::ObjectiveKind::MinimumCover
                    && self.objective.score().requested());
            if self.scenario.setup_query().is_some()
                || self.scenario.build_query().is_some()
                || self.scenario.core_query().verified_kick_profile().is_some()
                || self.rule_profile.verified_kick_profile().is_some()
                || self.allowed_colored_solution_identities.is_some()
                || !matches!(self.search_goal, SearchGoal::ClearToEmpty)
                || self.count_policy != CountPolicy::CountAll
                || self.output_policy != SearchOutputPolicy::Trace
                || !evidence_policy_supported
            {
                return None;
            }

            self.checked_pc_terminal_pointee_retained_bytes()
        }

        /// Complete retained pointee bytes for the canonical `pc.tiling`
        /// problem. Generic tiling (`Trace`) deliberately cannot use this
        /// authority projection.
        pub fn checked_pc_tiling_pointee_retained_bytes(&self) -> Option<u128> {
            if self.scenario.setup_query().is_some()
                || self.scenario.build_query().is_some()
                || self.scenario.core_query().verified_kick_profile().is_some()
                || self.rule_profile.verified_kick_profile().is_some()
                || self.allowed_colored_solution_identities.is_some()
                || !matches!(self.search_goal, SearchGoal::ClearToEmpty)
                || self.output_policy != SearchOutputPolicy::TilingOnly
                || self.objective != ObjectivePolicy::tiling()
                || self.pc_chance_evidence_policy != PcChanceEvidencePolicy::Disabled
                || self.solution_probability_policy().requested()
            {
                return None;
            }

            self.checked_pc_terminal_pointee_retained_bytes()
        }

        fn checked_pc_terminal_pointee_retained_bytes(&self) -> Option<u128> {
            match (self.preset, self.problem_kind, self.scenario.source()) {
                (
                    SearchProblemPreset::OpeningPc,
                    SearchProblemKind::OpeningPc,
                    ScenarioQuerySource::OpeningPreset,
                )
                | (
                    SearchProblemPreset::ScenarioPc,
                    SearchProblemKind::ScenarioPc,
                    ScenarioQuerySource::ScenarioPreset,
                ) => {}
                (
                    SearchProblemPreset::OpeningPc
                    | SearchProblemPreset::ScenarioPc
                    | SearchProblemPreset::Setup
                    | SearchProblemPreset::Build,
                    _,
                    _,
                ) => return None,
            }

            let mut bytes = core::mem::size_of::<SearchProblem>() as u128;
            bytes = bytes.checked_add(self.problem_id.checked_retained_capacity_bytes()?)?;
            bytes = bytes.checked_add(self.scenario.checked_pc_score_retained_capacity_bytes()?)?;
            bytes = bytes.checked_add(self.piece_source.checked_retained_capacity_bytes()?)?;
            bytes = bytes.checked_add(self.supply.checked_pc_score_retained_capacity_bytes()?)?;
            bytes = bytes.checked_add(checked_string_vec_retained_capacity_bytes(&self.labels)?)?;
            Some(bytes)
        }
    }

    fn checked_string_vec_retained_capacity_bytes(values: &Vec<String>) -> Option<u128> {
        let mut bytes = checked_count_bytes(
            values.capacity() as u128,
            core::mem::size_of::<String>() as u128,
        )?;
        for value in values {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        Some(bytes)
    }

    fn checked_inline_vec_retained_capacity_bytes<T>(values: &Vec<T>) -> Option<u128> {
        checked_count_bytes(values.capacity() as u128, core::mem::size_of::<T>() as u128)
    }

    fn checked_count_bytes(count: u128, item_size: u128) -> Option<u128> {
        count.checked_mul(item_size)
    }

    #[cfg(test)]
    mod tests {
        use clearra_core_domain::{
            board::board_size::BoardSize, pc::pc_target::PcTarget, piece::piece_kind::PieceKind,
            solution::StandardBoard64ColoredTilingIdentity,
        };
        use clearra_objectives::policy::objective_policy::ObjectivePolicy;
        use clearra_pc_graph::request::{
            OpeningPcSearchQuery, PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
            PieceWindow,
        };
        use clearra_supply::queue::{
            fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression,
        };

        use super::{checked_count_bytes, SearchProblem};
        use crate::{
            query::{BuildProblemLimits, BuildQuery, BuildTemplateBridge},
            PcChanceEvidencePolicy, ProblemCompiler, SearchProblemPreset,
        };

        fn fixed_queue(capacity: usize) -> PcQueueInput {
            let mut pieces = Vec::with_capacity(capacity);
            pieces.extend([PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S]);
            PcQueueInput::fixed_sequence(FixedSequence::new(pieces))
        }

        fn manual_expected(problem: &SearchProblem) -> Option<u128> {
            let mut bytes = core::mem::size_of::<SearchProblem>() as u128;
            bytes = bytes.checked_add(problem.problem_id.checked_retained_capacity_bytes()?)?;
            bytes = bytes.checked_add(
                problem
                    .scenario
                    .checked_pc_score_retained_capacity_bytes()?,
            )?;
            bytes = bytes.checked_add(problem.piece_source.checked_retained_capacity_bytes()?)?;
            bytes =
                bytes.checked_add(problem.supply.checked_pc_score_retained_capacity_bytes()?)?;
            bytes = bytes.checked_add(
                (problem.labels.capacity() as u128)
                    .checked_mul(core::mem::size_of::<String>() as u128)?,
            )?;
            for label in &problem.labels {
                bytes = bytes.checked_add(label.capacity() as u128)?;
            }
            Some(bytes)
        }

        fn manual_build_expected(problem: &SearchProblem) -> Option<u128> {
            let mut bytes = core::mem::size_of::<SearchProblem>() as u128;
            bytes = bytes.checked_add(problem.problem_id.checked_retained_capacity_bytes()?)?;
            bytes = bytes.checked_add(
                problem
                    .scenario
                    .checked_build_probability_retained_capacity_bytes()?,
            )?;
            bytes = bytes.checked_add(problem.piece_source.checked_retained_capacity_bytes()?)?;
            bytes = bytes.checked_add(
                problem
                    .supply
                    .checked_build_probability_retained_capacity_bytes()?,
            )?;
            bytes = bytes.checked_add(
                (problem.labels.capacity() as u128)
                    .checked_mul(core::mem::size_of::<String>() as u128)?,
            )?;
            for label in &problem.labels {
                bytes = bytes.checked_add(label.capacity() as u128)?;
            }
            if let Some(identities) = &problem.allowed_colored_solution_identities {
                bytes = bytes.checked_add((identities.capacity() as u128).checked_mul(
                    core::mem::size_of::<StandardBoard64ColoredTilingIdentity>() as u128,
                )?)?;
            }
            Some(bytes)
        }

        #[test]
        fn opening_problem_pointee_matches_fieldwise_owner_decomposition() {
            let expression = QueuePatternExpression::parse("P7P7P2", 1_066_867_200)
                .expect("factorized expression");
            let problem = ProblemCompiler::compile_opening_pc(
                &OpeningPcSearchQuery::new(PcTarget::six_lines())
                    .with_queue(PcQueueInput::pattern_expression(expression)),
            )
            .expect("opening problem");

            assert!(problem.scenario.pc_query().is_some());
            assert!(problem.scenario.checkpoint_schedule().is_some());
            assert_eq!(
                problem.checked_pc_score_pointee_retained_bytes(),
                manual_expected(&problem)
            );
        }

        #[test]
        fn scenario_problem_pointee_matches_fieldwise_owner_decomposition() {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0),
                fixed_queue(64),
                PieceWindow::new(4),
            )
            .with_exact_pieces(Some(4))
            .with_allow_hold(false);
            let problem = ProblemCompiler::compile_scenario_pc(&query).expect("scenario problem");

            assert!(problem.scenario.pc_query().is_none());
            assert!(problem.scenario.checkpoint_schedule().is_none());
            assert_eq!(
                problem.checked_pc_score_pointee_retained_bytes(),
                manual_expected(&problem)
            );
        }

        #[test]
        fn score_portfolio_policy_is_id_separated_idempotent_and_accounted() {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0),
                fixed_queue(64),
                PieceWindow::new(4),
            )
            .with_exact_pieces(Some(4))
            .with_allow_hold(false)
            .with_count_policy(PcCountPolicy::CountAll)
            .with_objective(ObjectivePolicy::minimum_cover().with_score_summary());
            let bare = ProblemCompiler::compile_scenario_pc(&query)
                .expect("score-portfolio scenario problem");
            let bare_id = bare.problem_id().as_str().to_owned();
            let portfolio = bare.clone().with_pc_score_portfolio_v2_evidence();

            assert_eq!(
                portfolio.pc_chance_evidence_policy(),
                PcChanceEvidencePolicy::PcScorePortfolioV2
            );
            assert_eq!(
                portfolio.problem_id().as_str(),
                format!("{bare_id}:pc-score-portfolio-v2")
            );
            assert_eq!(
                portfolio.checked_pc_score_pointee_retained_bytes(),
                manual_expected(&portfolio)
            );
            assert_eq!(
                portfolio
                    .clone()
                    .with_pc_score_portfolio_v2_evidence()
                    .problem_id(),
                portfolio.problem_id()
            );

            let wrong_objective = ProblemCompiler::compile_scenario_pc(
                &query
                    .clone()
                    .with_objective(ObjectivePolicy::all().with_score_summary()),
            )
            .expect("wrong score-only objective")
            .with_pc_score_portfolio_v2_evidence();
            assert_eq!(
                wrong_objective.checked_pc_score_pointee_retained_bytes(),
                None
            );

            let missing_score = ProblemCompiler::compile_scenario_pc(
                &query.with_objective(ObjectivePolicy::minimum_cover()),
            )
            .expect("wrong minimum-cover-only objective")
            .with_pc_score_portfolio_v2_evidence();
            assert_eq!(
                missing_score.checked_pc_score_pointee_retained_bytes(),
                None
            );
        }

        #[test]
        fn finite_build_probability_problem_matches_fieldwise_owner_decomposition() {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0),
                fixed_queue(96),
                PieceWindow::new(4),
            )
            .with_exact_pieces(Some(4))
            .with_allow_hold(false);
            let problem = ProblemCompiler::compile_scenario_pc(&query)
                .expect("finite BuildProbability problem");
            let expected =
                manual_build_expected(&problem).expect("canonical fieldwise owners fit in u128");
            let actual = problem
                .checked_build_probability_pointee_retained_bytes()
                .expect("canonical finite BuildProbability shape is supported");

            assert_eq!(actual, expected);
        }

        #[test]
        fn build_probability_pointee_supports_selected_identities_but_not_other_shapes() {
            let opening = ProblemCompiler::compile_opening_pc(&OpeningPcSearchQuery::new(
                PcTarget::four_lines(),
            ))
            .expect("opening problem");
            assert_eq!(
                opening.checked_build_probability_pointee_retained_bytes(),
                None
            );

            let template = BuildQuery::coverage_bridge(
                BuildTemplateBridge::new(
                    "retained-template",
                    BoardSize::new(10, 4).expect("board"),
                    1,
                ),
                1,
                BuildProblemLimits::new(1, 1),
            );
            let template_build =
                ProblemCompiler::compile_build(&template).expect("template Build problem");
            assert_eq!(
                template_build.checked_build_probability_pointee_retained_bytes(),
                None
            );

            let selected_query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0),
                fixed_queue(32),
                PieceWindow::new(4),
            )
            .with_exact_pieces(Some(4))
            .with_allow_hold(false)
            .with_allowed_colored_solution_identities(std::iter::empty());
            let selected = ProblemCompiler::compile_scenario_pc(&selected_query)
                .expect("selected-identity problem");
            assert_eq!(
                selected.checked_build_probability_pointee_retained_bytes(),
                manual_build_expected(&selected)
            );
            assert_eq!(
                selected.allowed_colored_solution_identities(),
                Some(&[][..])
            );
        }

        #[test]
        fn problem_pointee_fails_closed_for_selected_identity_and_wrong_preset() {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0),
                fixed_queue(32),
                PieceWindow::new(4),
            )
            .with_exact_pieces(Some(4))
            .with_allow_hold(false)
            .with_allowed_colored_solution_identities(std::iter::empty());
            let selected =
                ProblemCompiler::compile_scenario_pc(&query).expect("selected-identity problem");
            assert_eq!(selected.checked_pc_score_pointee_retained_bytes(), None);

            let canonical_query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0),
                fixed_queue(32),
                PieceWindow::new(4),
            )
            .with_exact_pieces(Some(4))
            .with_allow_hold(false);
            let mut wrong_preset = ProblemCompiler::compile_scenario_pc(&canonical_query)
                .expect("canonical scenario problem");
            wrong_preset.preset = SearchProblemPreset::Setup;
            assert_eq!(wrong_preset.checked_pc_score_pointee_retained_bytes(), None);
        }

        #[test]
        fn pointee_capacity_arithmetic_fails_closed_on_overflow() {
            assert_eq!(checked_count_bytes(u128::MAX, 2), None);
        }
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
        execution_automaton::SupplyObservationIdentity,
        hold::hold_policy::HoldPolicy,
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

        // Releasing the final held piece consumes one source piece that is
        // never placed. Its identity cannot affect geometry, so a finite
        // fixed or pattern source needs the same terminal projection as an
        // implicit standard bag when it ends at the exact placement window.
        let projects_unplaced_lookahead = query.allow_hold()
            && query.exact_pieces() == Some(geometry_piece_count)
            && source_sequence_length == required_source_pieces
            && automatic_source_pieces > source_sequence_length;

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
        let provenance = SupplyProvenanceId(piece_source.provenance().supply_provenance_id());
        HoldAutomatonState::with_contract(
            piece_source.id(),
            piece_source.kind(),
            bag_state.map_or(0, BagState::generated_count),
            hold_piece,
            if query.allow_hold() {
                HoldPolicy::Allowed
            } else {
                HoldPolicy::Forbidden
            },
            bag_state.map_or(0, BagState::epoch),
            bag_state.map_or(0, BagState::packed_remainder_key),
            SupplyObservationIdentity::new(query.queue_observation_policy(), provenance.0),
            provenance,
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
    KickProfile, PcChanceEvidencePolicy, PieceSource, ResourceBudget, RuleProfileSelection,
    SearchOutputPolicy, SearchProblemBoard, SearchProblemBudget, SearchProblemId,
    SearchProblemKind, SearchReplayTracePolicy, SupplyProvenance, TracePolicy,
};
