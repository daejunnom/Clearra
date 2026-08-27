mod backend_policy {
    pub type BackendPolicy = clearra_pc_graph::request::PcExecutionPolicy;
}
mod continuation_policy {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ContinuationPolicy {
        enabled: bool,
        min_remaining_queue: usize,
    }

    impl ContinuationPolicy {
        pub fn new(enabled: bool, min_remaining_queue: usize) -> Self {
            Self {
                enabled,
                min_remaining_queue,
            }
        }
    }
    impl ContinuationPolicy {
        pub fn enabled(self) -> bool {
            self.enabled
        }
    }
    impl ContinuationPolicy {
        pub fn min_remaining_queue(self) -> usize {
            self.min_remaining_queue
        }
    }

    impl Default for ContinuationPolicy {
        fn default() -> Self {
            Self::new(true, 0)
        }
    }
}
mod count_policy {
    pub type CountPolicy = clearra_pc_graph::request::PcCountPolicy;
}
mod exact_target_policy {
    use clearra_core_domain::pc::pc_target::PcTarget;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ExactTargetPolicy {
        None,
        LabelOnly { target: PcTarget },
    }

    impl ExactTargetPolicy {
        pub fn from_target(target: Option<PcTarget>) -> Self {
            match target {
                Some(target) => Self::LabelOnly { target },
                None => Self::None,
            }
        }
    }
    impl ExactTargetPolicy {
        pub fn target(self) -> Option<PcTarget> {
            match self {
                Self::None => None,
                Self::LabelOnly { target } => Some(target),
            }
        }
    }
    impl ExactTargetPolicy {
        pub fn is_core_success_condition(self) -> bool {
            false
        }
    }
}
mod kick_profile {
    use clearra_rules::{
        kicks::{KickProfileRegistry, KickTableProfileId},
        profile::rule_profile::RuleProfileId,
    };

    use super::RuleProfileSelection;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct KickProfile {
        profile_id: KickTableProfileId,
        source_rule: RuleProfileId,
        verified: bool,
        supports_180: bool,
        transition_count: usize,
    }

    impl KickProfile {
        pub fn from_rule_selection(selection: &RuleProfileSelection) -> Self {
            if let Some(verified) = selection.verified_kick_profile() {
                let profile = verified.profile();
                return Self {
                    profile_id: profile.id(),
                    source_rule: profile.source_rule(),
                    verified: true,
                    supports_180: profile.supports_180(),
                    transition_count: profile.transition_count(),
                };
            }

            let profile_id = match selection.rule().id() {
                RuleProfileId::Srs => KickTableProfileId::Srs90,
                RuleProfileId::SrsPlus => KickTableProfileId::SrsPlus,
                RuleProfileId::SrsX => KickTableProfileId::SrsX,
                RuleProfileId::Jstris180 => KickTableProfileId::Jstris180,
                RuleProfileId::Asc => KickTableProfileId::Asc,
                RuleProfileId::Ars => KickTableProfileId::Ars,
                RuleProfileId::NoKick => KickTableProfileId::NoKick,
                RuleProfileId::Custom => KickTableProfileId::Custom,
            };
            let descriptor = KickProfileRegistry::descriptor(profile_id);

            Self {
                profile_id,
                source_rule: selection.rule().id(),
                verified: descriptor.map(|value| value.verified()).unwrap_or(false),
                supports_180: descriptor
                    .map(|value| value.capability().supports_180())
                    .unwrap_or(false),
                transition_count: descriptor
                    .map(|value| value.transition_count())
                    .unwrap_or(0),
            }
        }
    }
    impl KickProfile {
        pub fn profile_id(self) -> KickTableProfileId {
            self.profile_id
        }
    }
    impl KickProfile {
        pub fn source_rule(self) -> RuleProfileId {
            self.source_rule
        }
    }
    impl KickProfile {
        pub fn verified(self) -> bool {
            self.verified
        }
    }
    impl KickProfile {
        pub fn supports_180(self) -> bool {
            self.supports_180
        }
    }
    impl KickProfile {
        pub fn transition_count(self) -> usize {
            self.transition_count
        }
    }
}
mod resource_budget {
    pub type ResourceBudget = super::SearchProblemBudget;
}
mod rule_profile_selection {
    use clearra_rules::{
        kicks::VerifiedKickTableProfile, profile::rule_profile::RuleProfile, spawn::SpawnProfile,
    };

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct RuleProfileSelection {
        rule: RuleProfile,
        verified_kick_profile: Option<VerifiedKickTableProfile>,
        spawn_profile: SpawnProfile,
    }

    impl RuleProfileSelection {
        pub fn new(
            rule: RuleProfile,
            verified_kick_profile: Option<VerifiedKickTableProfile>,
            spawn_profile: SpawnProfile,
        ) -> Self {
            Self {
                rule,
                verified_kick_profile,
                spawn_profile,
            }
        }
    }
    impl RuleProfileSelection {
        pub fn rule(&self) -> RuleProfile {
            self.rule
        }
    }
    impl RuleProfileSelection {
        pub fn verified_kick_profile(&self) -> Option<&VerifiedKickTableProfile> {
            self.verified_kick_profile.as_ref()
        }
    }
    impl RuleProfileSelection {
        pub fn spawn_profile(&self) -> SpawnProfile {
            self.spawn_profile
        }
    }
}
mod search_output_policy {
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub enum SearchOutputPolicy {
        #[default]
        Summary,
        Trace,
        /// Dedicated geometry-family projection for the canonical `pc.tiling`
        /// product contract. It retains the complete normalized solution set
        /// while excluding representative BuildUp traces and candidate-digest
        /// evidence that do not belong to the geometry-only result.
        TilingOnly,
        CoverageRows,
        CoverageSummary,
    }

    impl SearchOutputPolicy {
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::Summary => "summary",
                Self::Trace => "trace",
                Self::TilingOnly => "tiling-only",
                Self::CoverageRows => "coverage-rows",
                Self::CoverageSummary => "coverage-summary",
            }
        }

        pub const fn retains_solution_set(self) -> bool {
            !matches!(self, Self::CoverageSummary)
        }

        pub const fn retains_representative_trace(self) -> bool {
            !matches!(self, Self::TilingOnly | Self::CoverageSummary)
        }

        pub const fn retains_candidate_digest(self) -> bool {
            !matches!(self, Self::TilingOnly | Self::CoverageSummary)
        }
    }
}
mod pc_chance_evidence_policy {
    /// Closed, non-wire policy for retaining product-private PC chance evidence.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub enum PcChanceEvidencePolicy {
        #[default]
        Disabled,
        PcProbabilityV2,
        PcMinimumCoverV2,
        /// Purpose-separated private evidence for the typed
        /// `pc.score-minimals` producer. This is not interchangeable with
        /// either the score-only or minimum-cover-only product contract.
        PcScorePortfolioV2,
        /// Purpose-separated private execution evidence for `pc.saves` and
        /// `pc.best-save`. The product consumes terminal hold/cursor states,
        /// so ordinary Trace output is not sufficient authority.
        PcSaveGroupsV2,
        /// Purpose-separated private replay evidence for `pc.path`. The
        /// product consumes the complete legal replay family without enabling
        /// scoring or treating attack as an ordering coordinate.
        PcPathV2,
    }

    impl PcChanceEvidencePolicy {
        pub const fn retains_pc_probability_v2_evidence(self) -> bool {
            matches!(self, Self::PcProbabilityV2)
        }

        pub const fn retains_pc_coverage_evidence(self) -> bool {
            matches!(
                self,
                Self::PcProbabilityV2 | Self::PcMinimumCoverV2 | Self::PcScorePortfolioV2
            )
        }

        pub const fn retains_pc_minimum_cover_v2_evidence(self) -> bool {
            matches!(self, Self::PcMinimumCoverV2 | Self::PcScorePortfolioV2)
        }

        pub const fn retains_pc_score_portfolio_v2_evidence(self) -> bool {
            matches!(self, Self::PcScorePortfolioV2)
        }

        pub const fn retains_pc_save_groups_v2_evidence(self) -> bool {
            matches!(self, Self::PcSaveGroupsV2)
        }

        pub const fn retains_pc_path_v2_evidence(self) -> bool {
            matches!(self, Self::PcPathV2)
        }
    }
}
mod search_problem_board {
    use clearra_pc_graph::request::PcScenarioBoard;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct SearchProblemBoard {
        initial_board: PcScenarioBoard,
        search_height: u16,
    }

    impl SearchProblemBoard {
        pub fn new(initial_board: PcScenarioBoard, search_height: u16) -> Self {
            Self {
                initial_board,
                search_height,
            }
        }
    }
    impl SearchProblemBoard {
        pub fn initial_board(&self) -> &PcScenarioBoard {
            &self.initial_board
        }
    }
    impl SearchProblemBoard {
        pub fn visible_height(&self) -> u16 {
            self.initial_board.visible_height()
        }
    }
    impl SearchProblemBoard {
        pub fn search_height(&self) -> u16 {
            self.search_height
        }
    }
    impl SearchProblemBoard {
        pub fn occupied_mask(&self) -> u64 {
            self.initial_board.occupied_mask()
        }
    }
}
mod search_problem_budget {
    use clearra_profiles::search::search_defaults::SearchDefaults;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct SearchProblemBudget {
        max_nodes: usize,
        max_seconds: u64,
        max_results: usize,
        max_patterns: usize,
    }

    impl SearchProblemBudget {
        pub fn new(
            max_nodes: usize,
            max_seconds: u64,
            max_results: usize,
            max_patterns: usize,
        ) -> Self {
            Self {
                max_nodes,
                max_seconds,
                max_results,
                max_patterns,
            }
        }
    }
    impl SearchProblemBudget {
        pub fn max_nodes(self) -> usize {
            self.max_nodes
        }
    }
    impl SearchProblemBudget {
        pub fn max_seconds(self) -> u64 {
            self.max_seconds
        }
    }
    impl SearchProblemBudget {
        pub fn max_results(self) -> usize {
            self.max_results
        }
    }
    impl SearchProblemBudget {
        pub fn max_patterns(self) -> usize {
            self.max_patterns
        }
    }

    impl Default for SearchProblemBudget {
        fn default() -> Self {
            let defaults = SearchDefaults::MVP1;
            Self {
                max_nodes: defaults.max_nodes(),
                max_seconds: defaults.max_seconds(),
                max_results: defaults.setup_max_results(),
                max_patterns: defaults.setup_max_patterns(),
            }
        }
    }
}
mod search_problem_id {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct SearchProblemId {
        value: String,
    }

    impl SearchProblemId {
        pub fn new(value: impl Into<String>) -> Self {
            Self {
                value: value.into(),
            }
        }
    }
    impl SearchProblemId {
        pub fn as_str(&self) -> &str {
            &self.value
        }

        /// Returns only the heap payload retained by the identifier string,
        /// measured by `String` allocation capacity. The inline identifier is
        /// excluded.
        pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
            Some(self.value.capacity() as u128)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::SearchProblemId;

        #[test]
        fn retained_capacity_counts_identifier_allocation_capacity() {
            let mut value = String::with_capacity(256);
            value.push_str("opening-pc");
            let expected = value.capacity() as u128;
            let id = SearchProblemId::new(value);

            assert_eq!(id.checked_retained_capacity_bytes(), Some(expected));
        }
    }
}
mod search_problem_kind {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum SearchProblemKind {
        OpeningPc,
        ScenarioPc,
        SetupPostPc,
        BuildCoverage,
    }

    impl SearchProblemKind {
        pub fn as_str(self) -> &'static str {
            match self {
                Self::OpeningPc => "opening-pc",
                Self::ScenarioPc => "scenario-pc",
                Self::SetupPostPc => "setup-post-pc",
                Self::BuildCoverage => "build-coverage",
            }
        }
    }
}
mod search_replay_trace_policy {
    use clearra_core_domain::objective::trace_policy::TracePolicy as CoreTracePolicy;
    use clearra_profiles::search::search_defaults::SearchDefaults;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct SearchReplayTracePolicy {
        trace_policy: CoreTracePolicy,
        retained_trace_limit: usize,
    }

    impl SearchReplayTracePolicy {
        pub fn new(trace_policy: CoreTracePolicy, retained_trace_limit: usize) -> Self {
            Self {
                trace_policy,
                retained_trace_limit,
            }
        }
    }
    impl SearchReplayTracePolicy {
        pub fn trace_policy(self) -> CoreTracePolicy {
            self.trace_policy
        }
    }
    impl SearchReplayTracePolicy {
        pub fn retained_trace_limit(self) -> usize {
            self.retained_trace_limit
        }
    }

    impl Default for SearchReplayTracePolicy {
        fn default() -> Self {
            Self {
                trace_policy: CoreTracePolicy::Keep,
                retained_trace_limit: SearchDefaults::MVP1.scenario_retained_trace_limit(),
            }
        }
    }
}
mod supply_provenance {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::PcQueueInput;
    use clearra_profiles::bag::bag_profile::BagProfile;
    use clearra_supply::hold::hold_slot::HoldSlot;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct SupplyProvenance {
        queue: PcQueueInput,
        hold_state: HoldSlot,
        hold_enabled: bool,
        source_sequence_length: usize,
        projects_unplaced_lookahead: bool,
        bag: BagProfile,
    }

    impl SupplyProvenance {
        pub fn new(
            queue: PcQueueInput,
            hold_state: HoldSlot,
            hold_enabled: bool,
            source_sequence_length: usize,
            projects_unplaced_lookahead: bool,
            bag: BagProfile,
        ) -> Self {
            Self {
                queue,
                hold_state,
                hold_enabled,
                source_sequence_length,
                projects_unplaced_lookahead,
                bag,
            }
        }
    }
    impl SupplyProvenance {
        pub fn queue(&self) -> &PcQueueInput {
            &self.queue
        }

        /// Returns only heap payload retained by the queue provenance when it
        /// is accepted by the typed `pc.score` contract.
        ///
        /// Queue buffers are measured by allocation capacity. The inline
        /// `SupplyProvenance` and `PcQueueInput` owners are excluded. Queue
        /// kinds outside the score contract return `None`.
        pub fn checked_pc_score_retained_capacity_bytes(&self) -> Option<u128> {
            self.queue.checked_pc_score_retained_capacity_bytes()
        }

        /// Returns heap payload retained by the BuildProbability supply queue.
        /// The inline provenance and queue owners are excluded.
        pub fn checked_build_probability_retained_capacity_bytes(&self) -> Option<u128> {
            self.queue
                .checked_build_probability_retained_capacity_bytes()
        }
    }
    impl SupplyProvenance {
        pub fn queue_mode(&self) -> &'static str {
            self.queue.mode()
        }
    }
    impl SupplyProvenance {
        pub fn hold_state(&self) -> HoldSlot {
            self.hold_state
        }
    }
    impl SupplyProvenance {
        pub fn hold_piece(&self) -> Option<PieceKind> {
            self.hold_state.piece()
        }
    }
    impl SupplyProvenance {
        pub fn hold_enabled(&self) -> bool {
            self.hold_enabled
        }
    }
    impl SupplyProvenance {
        pub const fn source_sequence_length(&self) -> usize {
            self.source_sequence_length
        }
    }
    impl SupplyProvenance {
        pub const fn projects_unplaced_lookahead(&self) -> bool {
            self.projects_unplaced_lookahead
        }
    }
    impl SupplyProvenance {
        pub fn projects_standard_bag_lookahead(&self) -> bool {
            self.projects_unplaced_lookahead && matches!(self.queue, PcQueueInput::Standard7Bag)
        }
    }
    impl SupplyProvenance {
        pub const fn supply_window_resolution(&self) -> &'static str {
            if self.projects_unplaced_lookahead {
                "projected-terminal-lookahead"
            } else {
                "materialized-source"
            }
        }
    }
    impl SupplyProvenance {
        pub fn bag(&self) -> BagProfile {
            self.bag
        }
    }
}
mod trace_policy {
    use clearra_core_domain::objective::trace_policy::TracePolicy as CoreTracePolicy;

    use super::SearchReplayTracePolicy;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct TracePolicy {
        replay_trace_policy: SearchReplayTracePolicy,
    }

    impl TracePolicy {
        pub fn new(replay_trace_policy: SearchReplayTracePolicy) -> Self {
            Self {
                replay_trace_policy,
            }
        }
    }
    impl TracePolicy {
        pub fn core_trace_policy(self) -> CoreTracePolicy {
            self.replay_trace_policy.trace_policy()
        }
    }
    impl TracePolicy {
        pub fn retained_trace_limit(self) -> usize {
            self.replay_trace_policy.retained_trace_limit()
        }
    }
    impl TracePolicy {
        pub fn replay_trace_policy(self) -> SearchReplayTracePolicy {
            self.replay_trace_policy
        }
    }
}

pub use backend_policy::BackendPolicy;
pub use clearra_supply::{hold_automaton::HoldAutomatonState, piece_source::PieceSource};
pub use continuation_policy::ContinuationPolicy;
pub use count_policy::CountPolicy;
pub use exact_target_policy::ExactTargetPolicy;
pub use kick_profile::KickProfile;
pub use pc_chance_evidence_policy::PcChanceEvidencePolicy;
pub use resource_budget::ResourceBudget;
pub use rule_profile_selection::RuleProfileSelection;
pub use search_output_policy::SearchOutputPolicy;
pub use search_problem_board::SearchProblemBoard;
pub use search_problem_budget::SearchProblemBudget;
pub use search_problem_id::SearchProblemId;
pub use search_problem_kind::SearchProblemKind;
pub use search_replay_trace_policy::SearchReplayTracePolicy;
pub use supply_provenance::SupplyProvenance;
pub use trace_policy::TracePolicy;
