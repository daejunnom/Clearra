//! Finite Boolean board families used by the combinatorial proof experiments.
//!
//! A root denotes a set of assignments, not an enumerated list of boards.
//! Shannon nodes are reduced and interned in one manager. Consequently equal
//! roots in that manager denote equal families. Union, intersection, complement
//! and simultaneous substitution preserve their ordinary set meanings.
//!
//! This checker supports at most 64 variables so assignments and exact support
//! fit in a u64. That is a checker limit, not a restriction of the finite-set
//! proof. IDs belong to the manager that returned them; never mix managers.
//! Refusal is separate from the empty family, and successful intermediate nodes
//! remain valid if a later operation exhausts its budget.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct FamilyId(u32);

impl FamilyId {
    pub(super) const FALSE: Self = Self(0);
    pub(super) const TRUE: Self = Self(1);

    fn index(self) -> usize {
        self.0 as usize
    }

    fn constant(value: bool) -> Self {
        if value {
            Self::TRUE
        } else {
            Self::FALSE
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Refused {
    UnsupportedVariableCount,
    InvalidNodeBudget,
    InvalidVariable,
    InvalidFamily,
    InvalidSubstitution,
    NodeLimit,
    WorkLimit,
    Allocation,
    Cancelled,
}

#[derive(Clone, Copy)]
struct Node {
    variable: u8,
    low: FamilyId,
    high: FamilyId,
    support: u64,
}

impl Node {
    const TERMINAL_VARIABLE: u8 = u8::MAX;

    fn terminal(id: FamilyId) -> Self {
        Self {
            variable: Self::TERMINAL_VARIABLE,
            low: id,
            high: id,
            support: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Operation {
    And,
    Or,
    Xor,
}

type UniqueKey = (u8, FamilyId, FamilyId);
type ApplyKey = (Operation, FamilyId, FamilyId);
type CofactorKey = (FamilyId, u8, bool);

pub(super) struct BooleanFamily {
    variable_count: u8,
    node_limit: usize,
    memo_limit: usize,
    work_limit: usize,
    work_steps: usize,
    cancellation: Option<Arc<AtomicBool>>,
    nodes: Vec<Node>,
    unique: HashMap<UniqueKey, FamilyId>,
    applied: HashMap<ApplyKey, FamilyId>,
    cofactors: HashMap<CofactorKey, FamilyId>,
}

impl BooleanFamily {
    pub(super) fn new(variable_count: u8, node_limit: usize) -> Result<Self, Refused> {
        if variable_count > u64::BITS as u8 {
            return Err(Refused::UnsupportedVariableCount);
        }
        if !(2..=u32::MAX as usize).contains(&node_limit) {
            return Err(Refused::InvalidNodeBudget);
        }
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(2)
            .map_err(|_| Refused::Allocation)?;
        nodes.push(Node::terminal(FamilyId::FALSE));
        nodes.push(Node::terminal(FamilyId::TRUE));
        Ok(Self {
            variable_count,
            node_limit,
            memo_limit: node_limit.saturating_mul(4),
            work_limit: node_limit.saturating_mul(256),
            work_steps: 0,
            cancellation: None,
            nodes,
            unique: HashMap::new(),
            applied: HashMap::new(),
            cofactors: HashMap::new(),
        })
    }

    /// Sets an absolute cumulative limit; this does not reset work accounting.
    pub(super) fn set_work_limit(&mut self, work_limit: usize) {
        self.work_limit = work_limit;
    }

    pub(super) fn set_cancellation(&mut self, cancellation: Arc<AtomicBool>) {
        self.cancellation = Some(cancellation);
    }

    pub(super) fn work_steps(&self) -> usize {
        self.work_steps
    }

    /// Includes the two terminal nodes.
    pub(super) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub(super) fn variable(&mut self, variable: u8) -> Result<FamilyId, Refused> {
        self.validate_variable(variable)?;
        self.tick()?;
        self.intern(variable, FamilyId::FALSE, FamilyId::TRUE)
    }

    pub(super) fn and(&mut self, left: FamilyId, right: FamilyId) -> Result<FamilyId, Refused> {
        self.checked_apply(Operation::And, left, right)
    }

    pub(super) fn or(&mut self, left: FamilyId, right: FamilyId) -> Result<FamilyId, Refused> {
        self.checked_apply(Operation::Or, left, right)
    }

    pub(super) fn xor(&mut self, left: FamilyId, right: FamilyId) -> Result<FamilyId, Refused> {
        self.checked_apply(Operation::Xor, left, right)
    }

    pub(super) fn not(&mut self, root: FamilyId) -> Result<FamilyId, Refused> {
        self.xor(root, FamilyId::TRUE)
    }

    pub(super) fn ite(
        &mut self,
        condition: FamilyId,
        when_true: FamilyId,
        when_false: FamilyId,
    ) -> Result<FamilyId, Refused> {
        self.validate(condition)?;
        self.validate(when_true)?;
        self.validate(when_false)?;
        self.tick()?;
        if condition == FamilyId::TRUE || when_true == when_false {
            return Ok(when_true);
        }
        if condition == FamilyId::FALSE {
            return Ok(when_false);
        }
        let positive = self.and(condition, when_true)?;
        let complement = self.not(condition)?;
        let negative = self.and(complement, when_false)?;
        self.or(positive, negative)
    }

    pub(super) fn cofactor(
        &mut self,
        root: FamilyId,
        variable: u8,
        value: bool,
    ) -> Result<FamilyId, Refused> {
        self.validate(root)?;
        self.validate_variable(variable)?;
        self.restrict(root, variable, value)
    }

    /// Simultaneous pullback: input variable j is replaced by replacements[j].
    ///
    /// Replacement expressions are already in this manager and are not
    /// recursively substituted. Thus swaps such as x0 -> x1, x1 -> x0 retain
    /// simultaneous semantics. ITE restores ordering even when replacements
    /// contain variables earlier than the source node.
    pub(super) fn compose(
        &mut self,
        root: FamilyId,
        replacements: &[FamilyId],
    ) -> Result<FamilyId, Refused> {
        self.validate(root)?;
        if replacements.len() != usize::from(self.variable_count) {
            return Err(Refused::InvalidSubstitution);
        }
        for &replacement in replacements {
            self.validate(replacement)?;
        }
        self.substitute(root, replacements, &mut HashMap::new())
    }

    /// Existential projection of a set of assignment variables. This reuses
    /// the exact cofactors and union rather than enumerating assignments.
    pub(super) fn exists(
        &mut self,
        mut root: FamilyId,
        variables: u64,
    ) -> Result<FamilyId, Refused> {
        self.validate(root)?;
        let universe = if self.variable_count == u64::BITS as u8 {
            u64::MAX
        } else {
            (1_u64 << self.variable_count) - 1
        };
        if variables & !universe != 0 {
            return Err(Refused::InvalidVariable);
        }
        self.tick()?;
        let mut remaining = variables & self.support(root);
        while remaining != 0 {
            let variable = remaining.trailing_zeros() as u8;
            remaining &= remaining - 1;
            let low = self.cofactor(root, variable, false)?;
            let high = self.cofactor(root, variable, true)?;
            root = self.or(low, high)?;
        }
        Ok(root)
    }

    /// Evaluates an ID returned by this manager. Bits outside its variable
    /// universe are ignored. A path has at most variable_count decisions.
    pub(super) fn evaluate(&self, mut root: FamilyId, assignment: u64) -> bool {
        while root != FamilyId::FALSE && root != FamilyId::TRUE {
            let node = self.nodes[root.index()];
            root = if assignment & (1_u64 << node.variable) == 0 {
                node.low
            } else {
                node.high
            };
        }
        root == FamilyId::TRUE
    }

    /// Exact unrestricted variable support, not an independent-variable
    /// certificate under an arbitrary correlated domain restriction.
    pub(super) fn support(&self, root: FamilyId) -> u64 {
        self.nodes[root.index()].support
    }

    fn validate(&self, root: FamilyId) -> Result<(), Refused> {
        if root.index() < self.nodes.len() {
            Ok(())
        } else {
            Err(Refused::InvalidFamily)
        }
    }

    fn validate_variable(&self, variable: u8) -> Result<(), Refused> {
        if variable < self.variable_count {
            Ok(())
        } else {
            Err(Refused::InvalidVariable)
        }
    }

    fn tick(&mut self) -> Result<(), Refused> {
        if self
            .cancellation
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            return Err(Refused::Cancelled);
        }
        if self.work_steps >= self.work_limit {
            return Err(Refused::WorkLimit);
        }
        self.work_steps += 1;
        Ok(())
    }

    fn intern(&mut self, variable: u8, low: FamilyId, high: FamilyId) -> Result<FamilyId, Refused> {
        if low == high {
            return Ok(low);
        }
        debug_assert!(variable < self.variable_count);
        debug_assert!(self.nodes[low.index()].variable > variable);
        debug_assert!(self.nodes[high.index()].variable > variable);
        let key = (variable, low, high);
        if let Some(&root) = self.unique.get(&key) {
            return Ok(root);
        }
        if self.nodes.len() >= self.node_limit {
            return Err(Refused::NodeLimit);
        }
        self.nodes.try_reserve(1).map_err(|_| Refused::Allocation)?;
        self.unique
            .try_reserve(1)
            .map_err(|_| Refused::Allocation)?;
        let root = FamilyId(self.nodes.len() as u32);
        let support = (1_u64 << variable)
            | self.nodes[low.index()].support
            | self.nodes[high.index()].support;
        self.nodes.push(Node {
            variable,
            low,
            high,
            support,
        });
        self.unique.insert(key, root);
        Ok(root)
    }

    fn checked_apply(
        &mut self,
        operation: Operation,
        left: FamilyId,
        right: FamilyId,
    ) -> Result<FamilyId, Refused> {
        self.validate(left)?;
        self.validate(right)?;
        self.apply(operation, left, right)
    }

    fn apply(
        &mut self,
        operation: Operation,
        mut left: FamilyId,
        mut right: FamilyId,
    ) -> Result<FamilyId, Refused> {
        self.tick()?;
        if left > right {
            core::mem::swap(&mut left, &mut right);
        }
        match operation {
            Operation::And => {
                if left == FamilyId::FALSE {
                    return Ok(FamilyId::FALSE);
                }
                if left == FamilyId::TRUE || left == right {
                    return Ok(right);
                }
            }
            Operation::Or => {
                if left == FamilyId::TRUE {
                    return Ok(FamilyId::TRUE);
                }
                if left == FamilyId::FALSE || left == right {
                    return Ok(right);
                }
            }
            Operation::Xor => {
                if left == right {
                    return Ok(FamilyId::FALSE);
                }
                if left == FamilyId::FALSE {
                    return Ok(right);
                }
            }
        }
        let key = (operation, left, right);
        if let Some(&root) = self.applied.get(&key) {
            return Ok(root);
        }
        let left_node = self.nodes[left.index()];
        let right_node = self.nodes[right.index()];
        let variable = left_node.variable.min(right_node.variable);
        debug_assert_ne!(variable, Node::TERMINAL_VARIABLE);
        let (left_low, left_high) = if left_node.variable == variable {
            (left_node.low, left_node.high)
        } else {
            (left, left)
        };
        let (right_low, right_high) = if right_node.variable == variable {
            (right_node.low, right_node.high)
        } else {
            (right, right)
        };
        let low = self.apply(operation, left_low, right_low)?;
        let high = self.apply(operation, left_high, right_high)?;
        let root = self.intern(variable, low, high)?;
        // A missing memo entry cannot change the represented family. Limit
        // memo ownership separately instead of allocating an unbounded cache.
        if self.applied.len() < self.memo_limit && self.applied.try_reserve(1).is_ok() {
            self.applied.insert(key, root);
        }
        Ok(root)
    }

    fn restrict(&mut self, root: FamilyId, variable: u8, value: bool) -> Result<FamilyId, Refused> {
        self.tick()?;
        let node = self.nodes[root.index()];
        if node.variable > variable {
            return Ok(root);
        }
        if node.variable == variable {
            return Ok(if value { node.high } else { node.low });
        }
        let key = (root, variable, value);
        if let Some(&cofactor) = self.cofactors.get(&key) {
            return Ok(cofactor);
        }
        let low = self.restrict(node.low, variable, value)?;
        let high = self.restrict(node.high, variable, value)?;
        let cofactor = self.intern(node.variable, low, high)?;
        if self.cofactors.len() < self.memo_limit && self.cofactors.try_reserve(1).is_ok() {
            self.cofactors.insert(key, cofactor);
        }
        Ok(cofactor)
    }

    fn substitute(
        &mut self,
        root: FamilyId,
        replacements: &[FamilyId],
        memo: &mut HashMap<FamilyId, FamilyId>,
    ) -> Result<FamilyId, Refused> {
        self.tick()?;
        if root == FamilyId::FALSE || root == FamilyId::TRUE {
            return Ok(root);
        }
        if let Some(&replacement) = memo.get(&root) {
            return Ok(replacement);
        }
        let node = self.nodes[root.index()];
        let low = self.substitute(node.low, replacements, memo)?;
        let high = self.substitute(node.high, replacements, memo)?;
        let result = self.ite(replacements[usize::from(node.variable)], high, low)?;
        if memo.len() < self.memo_limit && memo.try_reserve(1).is_ok() {
            memo.insert(root, result);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Construct a root from its independent truth table using Shannon nodes,
    // without calling the Boolean apply operations under differential test.
    fn truth_root(family: &mut BooleanFamily, table: u8, variable: u8, assignment: u8) -> FamilyId {
        if variable == 3 {
            return FamilyId::constant(table & (1 << assignment) != 0);
        }
        let low = truth_root(family, table, variable + 1, assignment);
        let high = truth_root(family, table, variable + 1, assignment | (1 << variable));
        family.intern(variable, low, high).unwrap()
    }

    #[test]
    fn boolean_family_all_three_variable_functions_match_set_operations() {
        let mut family = BooleanFamily::new(3, 2048).unwrap();
        family.set_work_limit(20_000_000);
        let roots: Vec<_> = (0..=u8::MAX)
            .map(|table| truth_root(&mut family, table, 0, 0))
            .collect();
        for (table, &root) in roots.iter().enumerate() {
            for assignment in 0..8_u64 {
                assert_eq!(
                    family.evaluate(root, assignment),
                    table & (1 << assignment) != 0
                );
            }
        }
        for left in 0..=u8::MAX {
            for right in 0..=u8::MAX {
                let intersection = family
                    .and(roots[usize::from(left)], roots[usize::from(right)])
                    .unwrap();
                let union = family
                    .or(roots[usize::from(left)], roots[usize::from(right)])
                    .unwrap();
                let symmetric_difference = family
                    .xor(roots[usize::from(left)], roots[usize::from(right)])
                    .unwrap();
                assert_eq!(intersection, roots[usize::from(left & right)]);
                assert_eq!(union, roots[usize::from(left | right)]);
                assert_eq!(symmetric_difference, roots[usize::from(left ^ right)]);
            }
            assert_eq!(
                family.not(roots[usize::from(left)]).unwrap(),
                roots[usize::from(!left)]
            );
        }
        assert_eq!(family.node_count(), 256);
        assert!(family.work_steps() < 20_000_000);
    }

    #[test]
    fn boolean_family_cofactor_projection_and_pullback_match_assignments() {
        let mut family = BooleanFamily::new(3, 2048).unwrap();
        family.set_work_limit(2_000_000);
        let variables: Vec<_> = (0..3).map(|v| family.variable(v).unwrap()).collect();
        let not_x0 = family.not(variables[0]).unwrap();
        let x0_and_x1 = family.and(variables[0], variables[1]).unwrap();
        let x1_xor_x2 = family.xor(variables[1], variables[2]).unwrap();
        let substitutions = [
            [variables[1], variables[0], variables[2]],
            [variables[2], variables[0], variables[1]],
            [variables[0], variables[0], not_x0],
            [x0_and_x1, x1_xor_x2, FamilyId::TRUE],
            [FamilyId::FALSE, not_x0, FamilyId::FALSE],
        ];
        for table in 0..=u8::MAX {
            let root = truth_root(&mut family, table, 0, 0);
            for variable in 0..3 {
                for value in [false, true] {
                    let cofactor = family.cofactor(root, variable, value).unwrap();
                    for assignment in 0..8_u64 {
                        let substituted =
                            (assignment & !(1 << variable)) | (u64::from(value) << variable);
                        assert_eq!(
                            family.evaluate(cofactor, assignment),
                            table & (1 << substituted) != 0
                        );
                    }
                }
            }
            for forgotten in 0..8_u64 {
                let projected = family.exists(root, forgotten).unwrap();
                for assignment in 0..8_u64 {
                    let expected = (0..8_u64).any(|completion| {
                        completion & !forgotten == assignment & !forgotten
                            && table & (1 << completion) != 0
                    });
                    assert_eq!(family.evaluate(projected, assignment), expected);
                }
            }
            for replacements in substitutions {
                let pulled_back = family.compose(root, &replacements).unwrap();
                for assignment in 0..8_u64 {
                    let substituted = replacements.iter().enumerate().fold(
                        0_u64,
                        |bits, (variable, &expression)| {
                            bits | (u64::from(family.evaluate(expression, assignment)) << variable)
                        },
                    );
                    assert_eq!(
                        family.evaluate(pulled_back, assignment),
                        table & (1 << substituted) != 0
                    );
                }
            }
        }
        assert_eq!(
            family.compose(variables[0], &substitutions[0]).unwrap(),
            variables[1]
        );
    }

    #[test]
    fn boolean_family_reduction_removes_exactly_irrelevant_variables() {
        let mut family = BooleanFamily::new(3, 1024).unwrap();
        let x0 = family.variable(0).unwrap();
        let x1 = family.variable(1).unwrap();
        let x2 = family.variable(2).unwrap();
        let not_x0 = family.not(x0).unwrap();
        let left = family.and(x0, x1).unwrap();
        let right = family.and(not_x0, x1).unwrap();
        let joined = family.or(left, right).unwrap();
        assert_eq!(joined, x1);
        assert_eq!(family.support(joined), 1 << 1);
        let joined = family.or(joined, x2).unwrap();
        assert_eq!(family.support(joined), (1 << 1) | (1 << 2));
        for table in 0..=u8::MAX {
            let root = truth_root(&mut family, table, 0, 0);
            let mut semantic_support = 0;
            for variable in 0..3 {
                if (0..8).any(|assignment| {
                    (table & (1 << assignment) != 0)
                        != (table & (1 << (assignment ^ (1 << variable))) != 0)
                }) {
                    semantic_support |= 1 << variable;
                }
            }
            assert_eq!(family.support(root), semantic_support);
        }
    }

    #[test]
    fn boolean_family_correlated_domain_needs_joint_forgetting() {
        let mut family = BooleanFamily::new(2, 64).unwrap();
        let x0 = family.variable(0).unwrap();
        let x1 = family.variable(1).unwrap();
        let unequal = family.xor(x0, x1).unwrap();
        let domain = family.not(unequal).unwrap();
        for variable in 0..2 {
            let domain_low = family.cofactor(domain, variable, false).unwrap();
            let domain_high = family.cofactor(domain, variable, true).unwrap();
            let both_endpoints = family.and(domain_low, domain_high).unwrap();
            let low = family.cofactor(x0, variable, false).unwrap();
            let high = family.cofactor(x0, variable, true).unwrap();
            let difference = family.xor(low, high).unwrap();
            assert_eq!(
                family.and(both_endpoints, difference).unwrap(),
                FamilyId::FALSE
            );
        }
        assert!(family.evaluate(domain, 0));
        assert!(family.evaluate(domain, 3));
        assert!(!family.evaluate(x0, 0));
        assert!(family.evaluate(x0, 3));
        // The empty retained signature identifies these two different values.
        // Individually vacuous toggle tests cannot justify deleting both bits.
        let positive = family.and(domain, x0).unwrap();
        let not_x0 = family.not(x0).unwrap();
        let negative = family.and(domain, not_x0).unwrap();
        for forgotten in [1, 2] {
            let positive_projection = family.exists(positive, forgotten).unwrap();
            let negative_projection = family.exists(negative, forgotten).unwrap();
            assert_eq!(
                family
                    .and(positive_projection, negative_projection)
                    .unwrap(),
                FamilyId::FALSE
            );
        }
        let positive_projection = family.exists(positive, 3).unwrap();
        let negative_projection = family.exists(negative, 3).unwrap();
        assert_eq!(
            family
                .and(positive_projection, negative_projection)
                .unwrap(),
            FamilyId::TRUE
        );
    }

    #[test]
    fn boolean_family_refuses_resources_and_cancellation_without_false_results() {
        let mut small = BooleanFamily::new(1, 2).unwrap();
        assert_eq!(small.variable(0), Err(Refused::NodeLimit));
        assert_eq!(small.node_count(), 2);
        assert!(small.evaluate(FamilyId::TRUE, 0));

        let mut family = BooleanFamily::new(1, 16).unwrap();
        family.set_work_limit(0);
        assert_eq!(family.variable(0), Err(Refused::WorkLimit));
        family.set_work_limit(100);
        let x0 = family.variable(0).unwrap();
        let cancelled = Arc::new(AtomicBool::new(true));
        family.set_cancellation(Arc::clone(&cancelled));
        assert_eq!(family.not(x0), Err(Refused::Cancelled));
        assert!(family.evaluate(x0, 1));
        cancelled.store(false, Ordering::Relaxed);
        let complement = family.not(x0).unwrap();
        assert!(!family.evaluate(complement, 1));
        assert_eq!(family.variable(1), Err(Refused::InvalidVariable));
        assert_eq!(family.compose(x0, &[]), Err(Refused::InvalidSubstitution));
        assert_eq!(family.exists(x0, 2), Err(Refused::InvalidVariable));
        assert_eq!(
            family.and(FamilyId(u32::MAX), x0),
            Err(Refused::InvalidFamily)
        );
    }

    #[test]
    fn boolean_family_assignment_boundary_is_explicit() {
        assert!(matches!(
            BooleanFamily::new(65, 128),
            Err(Refused::UnsupportedVariableCount)
        ));
        let mut family = BooleanFamily::new(64, 128).unwrap();
        let highest = family.variable(63).unwrap();
        assert_eq!(family.support(highest), 1_u64 << 63);
        assert!(!family.evaluate(highest, (1_u64 << 63) - 1));
        assert!(family.evaluate(highest, 1_u64 << 63));
        let empty = BooleanFamily::new(0, 2).unwrap();
        assert!(!empty.evaluate(FamilyId::FALSE, 0));
        assert!(empty.evaluate(FamilyId::TRUE, u64::MAX));
    }
}
