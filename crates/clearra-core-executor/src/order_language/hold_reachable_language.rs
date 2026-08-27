use clearra_core_domain::operation::operation::OperationId;
use clearra_coverage::pattern::pattern_id::PatternId;
use clearra_supply::{hold_automaton::HoldAutomatonState, piece_source::PieceSourceId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HoldTransitionGraph {
    reachable_orders: Vec<Vec<OperationId>>,
    supports_long_carryover: bool,
}

impl HoldTransitionGraph {
    pub fn from_orders(orders: Vec<Vec<OperationId>>, supports_long_carryover: bool) -> Self {
        Self {
            reachable_orders: orders,
            supports_long_carryover,
        }
    }
}

impl HoldTransitionGraph {
    pub fn accepts_order(&self, order: &[OperationId]) -> bool {
        self.reachable_orders
            .iter()
            .any(|reachable| reachable.as_slice() == order)
    }
}

impl HoldTransitionGraph {
    pub fn supports_long_carryover(&self) -> bool {
        self.supports_long_carryover
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HoldReachableLanguage {
    pub piece_source_id: PieceSourceId,
    pub pattern_id: PatternId,
    pub initial_hold_state: HoldAutomatonState,
    pub transitions: HoldTransitionGraph,
}

impl HoldReachableLanguage {
    pub fn from_orders(
        piece_source_id: PieceSourceId,
        pattern_id: PatternId,
        initial_hold_state: HoldAutomatonState,
        orders: Vec<Vec<OperationId>>,
        supports_long_carryover: bool,
    ) -> Self {
        Self {
            piece_source_id,
            pattern_id,
            initial_hold_state,
            transitions: HoldTransitionGraph::from_orders(orders, supports_long_carryover),
        }
    }
}

impl HoldReachableLanguage {
    pub fn accepts_order(&self, order: &[OperationId]) -> bool {
        self.transitions.accepts_order(order)
    }
}

impl HoldReachableLanguage {
    pub fn supports_long_carryover(&self) -> bool {
        self.transitions.supports_long_carryover()
    }

    pub fn orders(&self) -> &[Vec<OperationId>] {
        &self.transitions.reachable_orders
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_supply::hold_automaton::SupplyProvenanceId;

    use super::*;

    #[test]
    fn hold_reachable_orders_support_long_carryover() {
        let piece_source_id = PieceSourceId::new(11);
        let hold_state = HoldAutomatonState::new(
            piece_source_id,
            6,
            Some(PieceKind::L),
            2,
            0xfeed,
            SupplyProvenanceId(77),
        );
        let language = HoldReachableLanguage::from_orders(
            piece_source_id,
            PatternId::new(3),
            hold_state,
            vec![vec![
                OperationId(9),
                OperationId(4),
                OperationId(8),
                OperationId(1),
            ]],
            true,
        );

        assert!(language.supports_long_carryover());
        assert_eq!(language.initial_hold_state.hold_piece, Some(PieceKind::L));
        assert_eq!(language.initial_hold_state.bag_epoch, 2);
        assert_eq!(language.initial_hold_state.bag_remainder_key, 0xfeed);
        assert!(language.accepts_order(&[
            OperationId(9),
            OperationId(4),
            OperationId(8),
            OperationId(1)
        ]));
    }
}
