use clearra_core_domain::operation::operation::OperationId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CandidateId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OperationSetKey(pub u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationDependencyGraph {
    accepted_orders: Vec<Vec<OperationId>>,
}

impl OperationDependencyGraph {
    pub fn from_orders(orders: Vec<Vec<OperationId>>) -> Self {
        Self {
            accepted_orders: orders,
        }
    }
}

impl OperationDependencyGraph {
    pub fn accepts_order(&self, order: &[OperationId]) -> bool {
        self.accepted_orders
            .iter()
            .any(|accepted| accepted.as_slice() == order)
    }
}

impl OperationDependencyGraph {
    pub fn order_count(&self) -> usize {
        self.accepted_orders.len()
    }
}

impl OperationDependencyGraph {
    pub fn orders(&self) -> &[Vec<OperationId>] {
        &self.accepted_orders
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LineClearConstraintSet {
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReachabilityConstraintSet {
    pub complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildOrderLanguage {
    pub candidate_id: CandidateId,
    pub operation_set_key: OperationSetKey,
    pub dependency_constraints: OperationDependencyGraph,
    pub line_clear_constraints: LineClearConstraintSet,
    pub reachability_constraints: ReachabilityConstraintSet,
}

impl BuildOrderLanguage {
    pub fn from_orders(
        candidate_id: CandidateId,
        operation_set_key: OperationSetKey,
        orders: Vec<Vec<OperationId>>,
    ) -> Self {
        Self {
            candidate_id,
            operation_set_key,
            dependency_constraints: OperationDependencyGraph::from_orders(orders),
            line_clear_constraints: LineClearConstraintSet { complete: true },
            reachability_constraints: ReachabilityConstraintSet { complete: true },
        }
    }
}

impl BuildOrderLanguage {
    pub fn accepts_order(&self, order: &[OperationId]) -> bool {
        self.dependency_constraints.accepts_order(order)
    }
}

impl BuildOrderLanguage {
    pub fn order_count(&self) -> usize {
        self.dependency_constraints.order_count()
    }
}

impl BuildOrderLanguage {
    pub fn orders(&self) -> &[Vec<OperationId>] {
        self.dependency_constraints.orders()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_orders_not_representative_only() {
        let language = BuildOrderLanguage::from_orders(
            CandidateId(7),
            OperationSetKey(0xabc),
            vec![
                vec![OperationId(1), OperationId(2), OperationId(3)],
                vec![OperationId(2), OperationId(1), OperationId(3)],
            ],
        );

        assert_eq!(language.order_count(), 2);
        assert!(language.accepts_order(&[OperationId(2), OperationId(1), OperationId(3)]));
    }
}
