//! Shared apply operations between selected-set ZDDs and Boolean predicates.
//!
//! A missing ZDD variable is false, whereas a missing BDD variable is free.
//! Both conversions below preserve that distinction. Queue variables remain
//! attached during filtering; only the physical-domain projection removes them.

use std::collections::HashMap;

use clearra_core_domain::execution_cancellation::ExecutionControl;

use super::{
    geometry_partial_family::{ZddArena, ZddId},
    symbolic_boolean::{BooleanArena, BooleanId},
    symbolic_family::SymbolicFamilyError,
};

pub(super) struct FamilyBridge {
    row_count: u32,
    max_work: u64,
    max_memo: usize,
    work: u64,
}

enum DomainTask {
    Visit(ZddId),
    Finish(ZddId),
}

enum FilterTask {
    Visit(ZddId, BooleanId),
    Finish((ZddId, BooleanId), u32),
}

impl FamilyBridge {
    pub fn new(row_count: u32, max_work: u64, max_memo: usize) -> Self {
        Self {
            row_count,
            max_work,
            max_memo,
            work: 0,
        }
    }

    pub const fn work_steps(&self) -> u64 {
        self.work
    }

    /// Existentially project source variables, then encode the exact selected
    /// placement family as a Boolean characteristic function. Only the live
    /// rank is converted; the original geometry expression is never converted.
    pub fn domain(
        &mut self,
        sets: &ZddArena,
        predicates: &mut BooleanArena,
        root: ZddId,
        control: &ExecutionControl,
    ) -> Result<BooleanId, SymbolicFamilyError> {
        if control.is_cancelled() {
            return Err(SymbolicFamilyError::Cancelled);
        }
        let mut memo = HashMap::new();
        let mut zeros = HashMap::new();
        let mut tasks = Vec::new();
        tasks.try_reserve(1).map_err(allocation)?;
        tasks.push(DomainTask::Visit(root));
        while let Some(task) = tasks.pop() {
            self.tick(control)?;
            match task {
                DomainTask::Visit(id) => {
                    if memo.contains_key(&id) {
                        continue;
                    }
                    let terminal = if id == ZddId::EMPTY {
                        Some(BooleanId::FALSE)
                    } else if id == ZddId::UNIT
                        || sets
                            .node(id)?
                            .is_some_and(|node| node.variable >= self.row_count)
                    {
                        Some(BooleanId::TRUE)
                    } else {
                        None
                    };
                    if let Some(value) = terminal {
                        self.insert(&mut memo, id, value)?;
                        continue;
                    }
                    let node = sets.node(id)?.ok_or(SymbolicFamilyError::InvalidProblem(
                        "symbolic_domain_zdd_root",
                    ))?;
                    tasks.try_reserve(3).map_err(allocation)?;
                    tasks.push(DomainTask::Finish(id));
                    tasks.push(DomainTask::Visit(node.high));
                    tasks.push(DomainTask::Visit(node.low));
                }
                DomainTask::Finish(id) => {
                    let node = sets.node(id)?.expect("visited nonterminal");
                    let low = self.with_zero_gap(
                        predicates,
                        memo[&node.low],
                        node.variable + 1,
                        placement_top(sets, node.low, self.row_count)?,
                        &mut zeros,
                        control,
                    )?;
                    let high = self.with_zero_gap(
                        predicates,
                        memo[&node.high],
                        node.variable + 1,
                        placement_top(sets, node.high, self.row_count)?,
                        &mut zeros,
                        control,
                    )?;
                    let value = predicates.mk(node.variable, low, high)?;
                    self.insert(&mut memo, id, value)?;
                }
            }
        }
        self.with_zero_gap(
            predicates,
            memo[&root],
            0,
            placement_top(sets, root, self.row_count)?,
            &mut zeros,
            control,
        )
    }

    /// Intersect a joint row/source family with a predicate on rows. A true
    /// predicate returns the original joint suffix, including all source bits.
    pub fn filter(
        &mut self,
        sets: &mut ZddArena,
        predicates: &BooleanArena,
        root: ZddId,
        condition: BooleanId,
        control: &ExecutionControl,
    ) -> Result<ZddId, SymbolicFamilyError> {
        if control.is_cancelled() {
            return Err(SymbolicFamilyError::Cancelled);
        }
        predicates.validate(condition)?;
        let mut memo = HashMap::new();
        let mut tasks = Vec::new();
        let mut values = Vec::new();
        tasks.try_reserve(1).map_err(allocation)?;
        tasks.push(FilterTask::Visit(root, condition));
        while let Some(task) = tasks.pop() {
            self.tick(control)?;
            match task {
                FilterTask::Visit(joint, mut predicate) => {
                    let top = sets.node(joint)?.map_or(u32::MAX, |node| node.variable);
                    // No selected variable occurs before top. Follow false
                    // cofactors, including when joint has reached its queue.
                    while let Some(node) = predicates.node(predicate) {
                        if node.variable >= self.row_count {
                            return Err(SymbolicFamilyError::InvalidProblem(
                                "symbolic_predicate_contains_source_variable",
                            ));
                        }
                        if node.variable >= top {
                            break;
                        }
                        self.tick(control)?;
                        predicate = node.low;
                    }
                    let key = (joint, predicate);
                    let terminal = if joint == ZddId::EMPTY || predicate == BooleanId::FALSE {
                        Some(ZddId::EMPTY)
                    } else if predicate == BooleanId::TRUE {
                        Some(joint)
                    } else {
                        memo.get(&key).copied()
                    };
                    if let Some(value) = terminal {
                        values.try_reserve(1).map_err(allocation)?;
                        values.push(value);
                        continue;
                    }
                    let node = sets
                        .node(joint)?
                        .ok_or(SymbolicFamilyError::InvalidProblem(
                            "symbolic_filter_zdd_root",
                        ))?;
                    let predicate_node = predicates.node(predicate).expect("nonterminal predicate");
                    let (low, high) = if node.variable == predicate_node.variable {
                        (predicate_node.low, predicate_node.high)
                    } else {
                        (predicate, predicate)
                    };
                    tasks.try_reserve(3).map_err(allocation)?;
                    tasks.push(FilterTask::Finish(key, node.variable));
                    tasks.push(FilterTask::Visit(node.high, high));
                    tasks.push(FilterTask::Visit(node.low, low));
                }
                FilterTask::Finish(key, variable) => {
                    let high = values.pop().expect("high child evaluated");
                    let low = values.pop().expect("low child evaluated");
                    let value = sets.mk(variable, low, high)?;
                    self.insert(&mut memo, key, value)?;
                    values.push(value);
                }
            }
        }
        values.pop().ok_or(SymbolicFamilyError::InvalidProblem(
            "symbolic_filter_result",
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn with_zero_gap(
        &mut self,
        arena: &mut BooleanArena,
        base: BooleanId,
        start: u32,
        stop: u32,
        memo: &mut HashMap<(BooleanId, u32, u32), BooleanId>,
        control: &ExecutionControl,
    ) -> Result<BooleanId, SymbolicFamilyError> {
        if base == BooleanId::FALSE || start == stop {
            return Ok(base);
        }
        if let Some(&value) = memo.get(&(base, start, stop)) {
            return Ok(value);
        }
        let mut value = base;
        for variable in (start..stop).rev() {
            self.tick(control)?;
            if let Some(&cached) = memo.get(&(base, variable, stop)) {
                value = cached;
            } else {
                value = arena.mk(variable, value, BooleanId::FALSE)?;
                self.insert(memo, (base, variable, stop), value)?;
            }
        }
        Ok(value)
    }

    fn insert<K: Eq + core::hash::Hash, V>(
        &self,
        memo: &mut HashMap<K, V>,
        key: K,
        value: V,
    ) -> Result<(), SymbolicFamilyError> {
        if self.max_memo != 0 && memo.len() >= self.max_memo && !memo.contains_key(&key) {
            return Err(SymbolicFamilyError::Resource("symbolic_bridge_memo_limit"));
        }
        memo.try_reserve(1).map_err(allocation)?;
        memo.insert(key, value);
        Ok(())
    }

    fn tick(&mut self, control: &ExecutionControl) -> Result<(), SymbolicFamilyError> {
        if self.work & 255 == 0 && control.is_cancelled() {
            return Err(SymbolicFamilyError::Cancelled);
        }
        if self.max_work != 0 && self.work >= self.max_work {
            return Err(SymbolicFamilyError::Resource("symbolic_bridge_work_limit"));
        }
        self.work = self
            .work
            .checked_add(1)
            .ok_or(SymbolicFamilyError::Resource(
                "symbolic_bridge_work_overflow",
            ))?;
        Ok(())
    }
}

fn placement_top(
    arena: &ZddArena,
    root: ZddId,
    row_count: u32,
) -> Result<u32, SymbolicFamilyError> {
    Ok(arena
        .node(root)?
        .map_or(row_count, |node| node.variable.min(row_count)))
}

fn allocation(_: std::collections::TryReserveError) -> SymbolicFamilyError {
    SymbolicFamilyError::Resource("symbolic_bridge_allocation")
}

#[cfg(test)]
mod tests {
    use super::super::{geometry_partial_family::ZddLimits, symbolic_boolean::BooleanLimits};
    use super::*;

    #[test]
    fn symbolic_bridge_preserves_source_correlations_and_missing_false_variables() {
        let mut sets = ZddArena::new(ZddLimits::default());
        let mut predicates = BooleanArena::new(101, BooleanLimits::default()).unwrap();
        let queue_a = sets.add(ZddId::UNIT, 101).unwrap();
        let queue_b = sets.add(ZddId::UNIT, 102).unwrap();
        let a = sets.add(queue_a, 0).unwrap();
        let b = sets.add(queue_b, 100).unwrap();
        let root = sets.union(a, b).unwrap();
        let mut bridge = FamilyBridge::new(101, 0, 0);
        let control = ExecutionControl::default();
        let domain = bridge
            .domain(&sets, &mut predicates, root, &control)
            .unwrap();
        assert!(predicates.evaluate(domain, |v| v == 0));
        assert!(predicates.evaluate(domain, |v| v == 100));
        assert!(!predicates.evaluate(domain, |v| v == 0 || v == 50));
        assert!(!predicates.evaluate(domain, |_| false));
        let selected = predicates.variable(100).unwrap();
        assert_eq!(
            bridge
                .filter(&mut sets, &predicates, root, selected, &control)
                .unwrap(),
            b
        );
        let absent = predicates.variable(50).unwrap();
        assert_eq!(
            bridge
                .filter(&mut sets, &predicates, root, absent, &control)
                .unwrap(),
            ZddId::EMPTY
        );
    }
}
