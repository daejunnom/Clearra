use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    SupplyWindowSize,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFieldError,
    BuildProbabilityQuery,
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

    pub fn to_query(
        &self,
        queue: PcQueueInput,
        execution_policy: PcExecutionPolicy,
        finite_standard_bag_len: Option<usize>,
        rule: RuleProfile,
    ) -> Result<BuildProbabilityQuery, BuildProbabilityFieldError> {
        let height = u8::try_from(self.visible_height)
            .map_err(|_| BuildProbabilityFieldError::HeightOutOfRange { height: u8::MAX })?;
        let field = BuildProbabilityField::from_words(height, self.base_words, self.target_words)?
            .with_horizontal_mirror_included(self.include_horizontal_mirror);
        let target_piece_count = field.target_piece_count();
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
        .with_retained_trace_limit(1)
        .with_execution_policy(execution_policy);
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
        Ok(BuildProbabilityQuery::new(query, field).with_aggregation(self.aggregation))
    }

    pub const fn hold_piece(&self) -> Option<PieceKind> {
        self.hold_piece
    }

    pub fn with_leading_hold_piece(mut self, piece: PieceKind) -> Self {
        self.hold_piece = Some(piece);
        self
    }
}
