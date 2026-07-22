use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_rules::profile::{
    builtin_rules::srs_plus,
    rule_profile::{RuleProfile, RuleProfileId},
};

use crate::variant::build_variant::BuildVariant;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostPcScenarioInput {
    board: PcScenarioBoard,
    remaining_queue: PcQueueInput,
    hold_piece: Option<PieceKind>,
    piece_window: PieceWindow,
    min_remaining_queue: usize,
    allow_hold: bool,
    requires_180: bool,
    requires_180_modeled: bool,
    rule: Option<RuleProfile>,
    count_policy: PcCountPolicy,
    retained_trace_limit: Option<usize>,
}

#[cfg(test)]
#[path = "post_pc_scenario_input_tests.rs"]
mod tests;

impl PostPcScenarioInput {
    pub fn new(
        board: PcScenarioBoard,
        remaining_queue: PcQueueInput,
        piece_window: PieceWindow,
    ) -> Self {
        Self {
            board,
            remaining_queue,
            hold_piece: None,
            piece_window,
            min_remaining_queue: 0,
            allow_hold: true,
            requires_180: false,
            requires_180_modeled: false,
            rule: None,
            count_policy: PcCountPolicy::CountAll,
            retained_trace_limit: None,
        }
    }
}
impl PostPcScenarioInput {
    pub fn from_build_variant(
        variant: &BuildVariant,
        visible_height: u16,
        remaining_queue: PcQueueInput,
        max_pieces: usize,
    ) -> Self {
        Self::new(
            PcScenarioBoard::standard_10(visible_height, variant.identity().occupied_shape()),
            remaining_queue,
            PieceWindow::new(max_pieces),
        )
        .with_hold_piece(variant.identity().hold_requirement())
    }
}
impl PostPcScenarioInput {
    pub fn with_hold_piece(mut self, hold_piece: Option<PieceKind>) -> Self {
        self.hold_piece = hold_piece;
        self
    }
}
impl PostPcScenarioInput {
    pub fn with_min_remaining_queue(mut self, min_remaining_queue: usize) -> Self {
        self.min_remaining_queue = min_remaining_queue;
        self
    }
}
impl PostPcScenarioInput {
    pub fn with_allow_hold(mut self, allow_hold: bool) -> Self {
        self.allow_hold = allow_hold;
        self
    }
}
impl PostPcScenarioInput {
    pub fn with_requires_180(mut self, requires_180: bool) -> Self {
        self.requires_180 = requires_180;
        self.requires_180_modeled = true;
        self
    }
}
impl PostPcScenarioInput {
    pub fn with_rule(mut self, rule: RuleProfile) -> Self {
        self.rule = Some(rule);
        self
    }
}
impl PostPcScenarioInput {
    pub fn requires_180(&self) -> bool {
        self.requires_180
    }
}
impl PostPcScenarioInput {
    pub fn requires_180_modeled(&self) -> bool {
        self.requires_180_modeled
    }
}
impl PostPcScenarioInput {
    pub fn rule_profile_id(&self) -> Option<RuleProfileId> {
        self.rule.map(|rule| rule.id())
    }
}
impl PostPcScenarioInput {
    pub fn effective_rule_profile_id(&self) -> RuleProfileId {
        self.rule_profile_id().unwrap_or_else(|| srs_plus().id())
    }
}
impl PostPcScenarioInput {
    pub fn with_count_policy(mut self, count_policy: PcCountPolicy) -> Self {
        self.count_policy = count_policy;
        self
    }
}
impl PostPcScenarioInput {
    pub fn with_retained_trace_limit(mut self, retained_trace_limit: usize) -> Self {
        self.retained_trace_limit = Some(retained_trace_limit);
        self
    }
}
impl PostPcScenarioInput {
    pub fn into_query(self) -> PcScenarioQuery {
        let mut query = PcScenarioQuery::new(self.board, self.remaining_queue, self.piece_window)
            .with_hold_piece(self.hold_piece)
            .with_min_remaining_queue(self.min_remaining_queue)
            .with_allow_hold(self.allow_hold)
            .with_requires_180(self.requires_180)
            .with_count_policy(self.count_policy);

        if let Some(rule) = self.rule {
            query = query.with_rule(rule);
        }
        if let Some(retained_trace_limit) = self.retained_trace_limit {
            query = query.with_retained_trace_limit(retained_trace_limit);
        }

        query
    }
}
