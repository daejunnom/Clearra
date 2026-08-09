use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    SupplyWindowSize,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFieldError,
    BuildProbabilityFinesseRequest, BuildProbabilityQuery, FinesseMetric, FinessePatternKnowledge,
    FinesseScoreRequest,
};
use clearra_rules::profile::rule_profile::RuleProfile;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebBuildProbabilityInput {
    base_words: [u64; 4],
    target_words: [u64; 4],
    visible_height: u16,
    hold_piece: Option<PieceKind>,
    allow_hold: bool,
    source_piece_count: Option<usize>,
    include_horizontal_mirror: bool,
    aggregation: BuildProbabilityAggregation,
    finesse: BuildProbabilityFinesseRequest,
}

impl WebBuildProbabilityInput {
    pub fn new(base_mask: u64, target_cells: u64, visible_height: u16) -> Self {
        Self::from_words(
            [base_mask, 0, 0, 0],
            [target_cells, 0, 0, 0],
            visible_height,
        )
    }

    pub fn from_words(base_words: [u64; 4], target_words: [u64; 4], visible_height: u16) -> Self {
        Self {
            base_words,
            target_words,
            visible_height,
            hold_piece: None,
            allow_hold: true,
            source_piece_count: None,
            include_horizontal_mirror: true,
            aggregation: BuildProbabilityAggregation::Buildability,
            finesse: BuildProbabilityFinesseRequest::Off,
        }
    }

    pub fn with_hold_piece(mut self, hold_piece: Option<PieceKind>) -> Self {
        self.hold_piece = hold_piece;
        self
    }

    pub fn with_allow_hold(mut self, allow_hold: bool) -> Self {
        self.allow_hold = allow_hold;
        self
    }

    pub fn with_source_piece_count(mut self, source_piece_count: usize) -> Self {
        self.source_piece_count = Some(source_piece_count);
        self
    }

    pub fn with_horizontal_mirror_included(mut self, included: bool) -> Self {
        self.include_horizontal_mirror = included;
        self
    }

    pub fn with_aggregation(mut self, aggregation: BuildProbabilityAggregation) -> Self {
        self.aggregation = aggregation;
        self
    }

    pub fn with_finesse(
        mut self,
        metric: FinesseMetric,
        pattern_knowledge: FinessePatternKnowledge,
    ) -> Self {
        self.finesse = match metric {
            FinesseMetric::Off => BuildProbabilityFinesseRequest::Off,
            FinesseMetric::Inputs => BuildProbabilityFinesseRequest::Search { pattern_knowledge },
        };
        self
    }

    pub fn with_finesse_score(mut self, score: FinesseScoreRequest) -> Self {
        self.finesse = BuildProbabilityFinesseRequest::Score {
            pattern_knowledge: self.finesse.pattern_knowledge(),
            request: score,
        };
        self
    }

    pub fn to_query(
        &self,
        queue: PcQueueInput,
        execution_policy: PcExecutionPolicy,
        finite_standard_bag_len: Option<usize>,
        rule: RuleProfile,
        objective: ObjectivePolicy,
    ) -> Result<BuildProbabilityQuery, BuildProbabilityFieldError> {
        let height = u8::try_from(self.visible_height)
            .map_err(|_| BuildProbabilityFieldError::HeightOutOfRange { height: u8::MAX })?;
        let field = if self.finesse.metric().requested() {
            BuildProbabilityField::from_words_preserving_height(
                height,
                self.base_words,
                self.target_words,
            )?
        } else {
            BuildProbabilityField::from_words(height, self.base_words, self.target_words)?
        }
        .with_horizontal_mirror_included(
            self.include_horizontal_mirror
                && !matches!(&self.finesse, BuildProbabilityFinesseRequest::Score { .. }),
        );
        let target_piece_count = self.finesse.score().map_or_else(
            || field.target_piece_count(),
            |score| score.placements().len(),
        );
        let compact_supply_shell = field.compact_base_mask().unwrap_or(0);
        let mut query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(u16::from(field.height()), compact_supply_shell),
            queue,
            PieceWindow::new(target_piece_count),
        )
        .with_rule(rule)
        .with_exact_pieces(Some(target_piece_count))
        .with_min_remaining_queue(0)
        .with_hold_piece(self.hold_piece)
        .with_allow_hold(self.allow_hold)
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_objective(objective)
        .with_retained_trace_limit(1)
        .with_execution_policy(execution_policy);
        // Initial hold is an independent piece, not a queue prefix. The
        // source therefore needs one fewer placed piece when it is occupied.
        let initial_hold_prefix = usize::from(self.allow_hold && self.hold_piece.is_some());
        let automatic_source_pieces = target_piece_count
            .saturating_add(usize::from(self.allow_hold))
            .saturating_sub(initial_hold_prefix);
        let finite_window = finite_standard_bag_len
            .map(|length| SupplyWindowSize::new(length.min(automatic_source_pieces)));
        if let Some(window) = self
            .source_piece_count
            .map(SupplyWindowSize::new)
            .or(finite_window)
        {
            query = query.with_supply_window_size(window);
        }
        let query = BuildProbabilityQuery::new(query, field).with_aggregation(self.aggregation);
        Ok(match &self.finesse {
            BuildProbabilityFinesseRequest::Off => query,
            BuildProbabilityFinesseRequest::Search { pattern_knowledge } => {
                query.with_finesse(FinesseMetric::Inputs, *pattern_knowledge)
            }
            BuildProbabilityFinesseRequest::Score {
                pattern_knowledge,
                request,
            } => query
                .with_finesse(FinesseMetric::Inputs, *pattern_knowledge)
                .with_finesse_score(request.clone()),
        })
    }

    pub const fn hold_piece(&self) -> Option<PieceKind> {
        self.hold_piece
    }

    pub fn with_leading_hold_piece(mut self, piece: PieceKind) -> Self {
        self.hold_piece = Some(piece);
        self
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{PcExecutionPolicy, PcQueueInput};
    use clearra_problem::{
        BuildProbabilityFinesseRequest, FinesseMetric, FinessePatternKnowledge, FinessePlacement,
        FinesseScoreRequest,
    };
    use clearra_rules::profile::builtin_rules::srs_plus;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::WebBuildProbabilityInput;

    #[test]
    fn latest_finesse_variant_replaces_a_prior_score_request() {
        let score = FinesseScoreRequest::new(vec![FinessePlacement::new(
            PieceKind::I,
            RotationState::Zero,
            3,
            0,
        )])
        .unwrap();
        let input = WebBuildProbabilityInput::new(0, 0xf, 4)
            .with_finesse_score(score)
            .with_finesse(FinesseMetric::Off, FinessePatternKnowledge::Oracle);
        let query = input
            .to_query(
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
                PcExecutionPolicy::mvp_default(),
                None,
                srs_plus(),
                ObjectivePolicy::unique(),
            )
            .unwrap();

        assert!(matches!(
            query.finesse_request(),
            BuildProbabilityFinesseRequest::Off
        ));
        assert!(query.finesse_score().is_none());
        assert_eq!(query.field().height(), 1);
        assert!(query.field().includes_horizontal_mirror());
    }
}
