use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_pc_graph::{
    classification::{ChainClass, ChainClassifier},
    dag::CheckpointSchedule,
    request::{PcCompletionGoal, PcScenarioBoard, PcScenarioQuery, PieceWindow},
};

use super::{build_query::BuildQuery, pc_query::PcQuery, setup_query::SetupSearchQuery};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioQuerySource {
    OpeningPreset,
    ScenarioPreset,
    SetupPreset,
    BuildPreset,
}

impl ScenarioQuerySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpeningPreset => "opening-preset",
            Self::ScenarioPreset => "scenario-preset",
            Self::SetupPreset => "setup-preset",
            Self::BuildPreset => "build-preset",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioQuery {
    source: ScenarioQuerySource,
    core_query: PcScenarioQuery,
    labels: Vec<String>,
    checkpoint_schedule: Option<CheckpointSchedule>,
    chain_class: ChainClass,
    exact_target_policy: Option<PcTarget>,
    pc_query: Option<PcQuery>,
    setup_query: Option<SetupSearchQuery>,
    build_query: Option<BuildQuery>,
}

impl ScenarioQuery {
    pub fn opening_preset(
        core_query: PcScenarioQuery,
        target: PcTarget,
        labels: Vec<String>,
        pc_query: PcQuery,
    ) -> Self {
        Self {
            source: ScenarioQuerySource::OpeningPreset,
            core_query,
            labels,
            checkpoint_schedule: CheckpointSchedule::for_opening_target(target).ok(),
            chain_class: ChainClassifier::opening(target),
            exact_target_policy: Some(target),
            pc_query: Some(pc_query),
            setup_query: None,
            build_query: None,
        }
    }
}
impl ScenarioQuery {
    pub fn scenario_preset(core_query: PcScenarioQuery) -> Self {
        Self {
            source: ScenarioQuerySource::ScenarioPreset,
            core_query,
            labels: Vec::new(),
            checkpoint_schedule: None,
            chain_class: ChainClassifier::scenario(),
            exact_target_policy: None,
            pc_query: None,
            setup_query: None,
            build_query: None,
        }
    }
}
impl ScenarioQuery {
    pub fn setup_preset(core_query: PcScenarioQuery, setup_query: SetupSearchQuery) -> Self {
        Self {
            source: ScenarioQuerySource::SetupPreset,
            core_query,
            labels: vec!["setup".to_owned()],
            checkpoint_schedule: None,
            chain_class: ChainClassifier::scenario(),
            exact_target_policy: None,
            pc_query: None,
            setup_query: Some(setup_query),
            build_query: None,
        }
    }
}
impl ScenarioQuery {
    pub fn build_preset(core_query: PcScenarioQuery, build_query: BuildQuery) -> Self {
        let label = format!("build:{}", build_query.template().id());
        Self {
            source: ScenarioQuerySource::BuildPreset,
            core_query,
            labels: vec![label],
            checkpoint_schedule: None,
            chain_class: ChainClassifier::scenario(),
            exact_target_policy: None,
            pc_query: None,
            setup_query: None,
            build_query: Some(build_query),
        }
    }
}
impl ScenarioQuery {
    pub fn source(&self) -> ScenarioQuerySource {
        self.source
    }
}
impl ScenarioQuery {
    pub fn core_query(&self) -> &PcScenarioQuery {
        &self.core_query
    }
}
impl ScenarioQuery {
    pub fn labels(&self) -> &[String] {
        &self.labels
    }
}
impl ScenarioQuery {
    pub fn checkpoint_schedule(&self) -> Option<&CheckpointSchedule> {
        self.checkpoint_schedule.as_ref()
    }
}
impl ScenarioQuery {
    pub fn chain_class(&self) -> ChainClass {
        self.chain_class
    }
}
impl ScenarioQuery {
    pub fn exact_target_policy(&self) -> Option<PcTarget> {
        self.exact_target_policy
    }
}
impl ScenarioQuery {
    pub fn pc_query(&self) -> Option<&PcQuery> {
        self.pc_query.as_ref()
    }
}
impl ScenarioQuery {
    pub fn setup_query(&self) -> Option<&SetupSearchQuery> {
        self.setup_query.as_ref()
    }
}
impl ScenarioQuery {
    pub fn build_query(&self) -> Option<&BuildQuery> {
        self.build_query.as_ref()
    }
}
impl ScenarioQuery {
    pub fn initial_board(&self) -> &PcScenarioBoard {
        self.core_query.initial_board()
    }
}
impl ScenarioQuery {
    pub fn piece_window(&self) -> PieceWindow {
        self.core_query.piece_window()
    }
}
impl ScenarioQuery {
    pub fn exact_pieces(&self) -> Option<usize> {
        self.core_query.exact_pieces()
    }
}
impl ScenarioQuery {
    pub fn goal(&self) -> PcCompletionGoal {
        self.core_query.completion_goal()
    }

    /// Returns only heap payload retained by the canonical compiled scenario
    /// shape consumed by BuildProbability. The inline `ScenarioQuery` and its
    /// inline field owners are excluded.
    pub fn checked_build_probability_retained_capacity_bytes(&self) -> Option<u128> {
        if !matches!(self.source, ScenarioQuerySource::ScenarioPreset)
            || self.pc_query.is_some()
            || self.setup_query.is_some()
            || self.build_query.is_some()
            || self.checkpoint_schedule.is_some()
            || self.exact_target_policy.is_some()
        {
            return None;
        }

        self.core_query
            .checked_build_probability_retained_capacity_bytes()?
            .checked_add(checked_string_vec_retained_capacity_bytes(&self.labels)?)
    }

    /// Returns only heap payload retained by this compiled query when it has
    /// the canonical opening- or scenario-`pc.score` shape.
    ///
    /// Queue, label, checkpoint, and nested opening-query allocations are
    /// measured by allocation capacity. The inline `ScenarioQuery`,
    /// `PcScenarioQuery`, `PcQuery`, and `CheckpointSchedule` owners are
    /// excluded. Unsupported sources, malformed preset decomposition,
    /// imported kick profiles, caller-selected solution identities, and queue
    /// kinds outside the typed score contract return `None`.
    pub fn checked_pc_score_retained_capacity_bytes(&self) -> Option<u128> {
        if self.setup_query.is_some()
            || self.build_query.is_some()
            || self.core_query.verified_kick_profile().is_some()
            || self
                .core_query
                .allowed_colored_solution_identities()
                .is_some()
        {
            return None;
        }

        match self.source {
            ScenarioQuerySource::OpeningPreset
                if self.pc_query.is_some()
                    && self.checkpoint_schedule.is_some()
                    && self.exact_target_policy.is_some() => {}
            ScenarioQuerySource::ScenarioPreset
                if self.pc_query.is_none()
                    && self.checkpoint_schedule.is_none()
                    && self.exact_target_policy.is_none() => {}
            ScenarioQuerySource::OpeningPreset
            | ScenarioQuerySource::ScenarioPreset
            | ScenarioQuerySource::SetupPreset
            | ScenarioQuerySource::BuildPreset => return None,
        }

        let mut bytes = self
            .core_query
            .remaining_queue()
            .checked_pc_score_retained_capacity_bytes()?;
        bytes = bytes.checked_add(checked_string_vec_retained_capacity_bytes(&self.labels)?)?;
        if let Some(schedule) = &self.checkpoint_schedule {
            bytes = bytes.checked_add(schedule.checked_retained_capacity_bytes()?)?;
        }
        if let Some(pc_query) = &self.pc_query {
            if pc_query.verified_kick_profile().is_some() {
                return None;
            }
            bytes = bytes.checked_add(
                pc_query
                    .queue()
                    .checked_pc_score_retained_capacity_bytes()?,
            )?;
        }
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

fn checked_count_bytes(count: u128, item_size: u128) -> Option<u128> {
    count.checked_mul(item_size)
}

#[cfg(test)]
mod retained_capacity_tests {
    use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    };
    use clearra_rules::kicks::{SrsKicks, VerifiedKickTableProfile};
    use clearra_supply::queue::{
        fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression,
    };

    use super::{checked_count_bytes, PcQuery, ScenarioQuery};
    use crate::preset::OpeningPreset;

    fn fixed_queue(capacity: usize) -> PcQueueInput {
        let mut pieces = Vec::with_capacity(capacity);
        pieces.extend([PieceKind::I, PieceKind::O]);
        PcQueueInput::fixed_sequence(FixedSequence::new(pieces))
    }

    #[test]
    fn opening_retained_capacity_counts_core_and_pc_queue_copies_and_schedule() {
        let opening = OpeningPcSearchQuery::new(PcTarget::four_lines()).with_queue(fixed_queue(64));
        let scenario = OpeningPreset::try_from_pc_query(PcQuery::from_opening_query(&opening))
            .expect("opening preset")
            .into_scenario_query();

        let core_queue_bytes = scenario
            .core_query
            .remaining_queue()
            .checked_pc_score_retained_capacity_bytes()
            .expect("accepted core queue");
        let pc_queue_bytes = scenario
            .pc_query
            .as_ref()
            .expect("opening pc query")
            .queue()
            .checked_pc_score_retained_capacity_bytes()
            .expect("accepted opening queue");
        let label_outer_bytes = (scenario.labels.capacity() as u128)
            .checked_mul(core::mem::size_of::<String>() as u128)
            .expect("label outer capacity fits u128");
        let label_payload_bytes = scenario
            .labels
            .iter()
            .try_fold(0_u128, |bytes, label| {
                bytes.checked_add(label.capacity() as u128)
            })
            .expect("label payload fits u128");
        let schedule_bytes = scenario
            .checkpoint_schedule
            .as_ref()
            .expect("opening checkpoint schedule")
            .checked_retained_capacity_bytes()
            .expect("schedule capacity fits u128");
        let expected = core_queue_bytes
            .checked_add(pc_queue_bytes)
            .and_then(|bytes| bytes.checked_add(label_outer_bytes))
            .and_then(|bytes| bytes.checked_add(label_payload_bytes))
            .and_then(|bytes| bytes.checked_add(schedule_bytes));

        assert_eq!(
            scenario.checked_pc_score_retained_capacity_bytes(),
            expected
        );
        assert_eq!(core_queue_bytes, pc_queue_bytes);
    }

    #[test]
    fn scenario_retained_capacity_counts_one_factorized_queue_and_no_opening_state() {
        let expression =
            QueuePatternExpression::parse("P7P7P2", 1_066_867_200).expect("factorized expression");
        let queue_bytes = expression
            .checked_retained_capacity_bytes()
            .expect("expression capacity fits u128");
        let scenario = ScenarioQuery::scenario_preset(PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::pattern_expression(expression),
            PieceWindow::new(16),
        ));

        assert!(scenario.pc_query.is_none());
        assert!(scenario.checkpoint_schedule.is_none());
        assert_eq!(scenario.labels.capacity(), 0);
        assert_eq!(
            scenario.checked_pc_score_retained_capacity_bytes(),
            Some(queue_bytes)
        );
    }

    #[test]
    fn pc_score_retained_capacity_fails_closed_for_noncanonical_query_owners() {
        let selected = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            fixed_queue(16),
            PieceWindow::new(10),
        )
        .with_allowed_colored_solution_identities(std::iter::empty());
        let mut malformed_opening = ScenarioQuery::scenario_preset(selected);
        malformed_opening.source = super::ScenarioQuerySource::OpeningPreset;

        assert_eq!(
            ScenarioQuery::scenario_preset(malformed_opening.core_query.clone())
                .checked_pc_score_retained_capacity_bytes(),
            None
        );
        assert_eq!(
            malformed_opening.checked_pc_score_retained_capacity_bytes(),
            None
        );

        let imported = VerifiedKickTableProfile::try_new(SrsKicks::srs_plus_profile())
            .expect("verified imported kick profile");
        let imported = ScenarioQuery::scenario_preset(
            PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0),
                fixed_queue(16),
                PieceWindow::new(4),
            )
            .with_verified_kick_table_profile(imported),
        );
        assert_eq!(imported.checked_pc_score_retained_capacity_bytes(), None);
    }

    #[test]
    fn retained_capacity_arithmetic_fails_closed_on_overflow() {
        assert_eq!(checked_count_bytes(u128::MAX, 2), None);
    }
}
