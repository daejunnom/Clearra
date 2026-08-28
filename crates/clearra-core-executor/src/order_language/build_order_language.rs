// SRP rationale: this module has one change reason: parsing and evaluating the canonical Build order language.
use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
};

use clearra_core_domain::operation::operation::OperationId;

pub const MAX_OPERATION_ORDER_OPERATIONS: usize = 4_096;
pub const DEFAULT_OPERATION_ORDER_TIMEOUT_SECONDS: u16 = 900;
pub const MAX_OPERATION_ORDER_TIMEOUT_SECONDS: u16 = 900;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CandidateId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationSetKey(pub u64);

/// A dynamically sized exact operation set. Unused high bits are kept zero.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationBitSet {
    bit_len: usize,
    words: Vec<u64>,
}

impl OperationBitSet {
    pub fn new(bit_len: usize) -> Result<Self, OperationOrderLanguageError> {
        if bit_len > MAX_OPERATION_ORDER_OPERATIONS {
            return Err(OperationOrderLanguageError::OperationLimitExceeded {
                operation_count: bit_len,
                maximum: MAX_OPERATION_ORDER_OPERATIONS,
            });
        }
        Ok(Self {
            bit_len,
            words: vec![0; bit_len.div_ceil(64)],
        })
    }

    pub fn full(bit_len: usize) -> Result<Self, OperationOrderLanguageError> {
        let mut value = Self::new(bit_len)?;
        value.words.fill(u64::MAX);
        if let Some(last) = value.words.last_mut() {
            let live = bit_len % 64;
            if live != 0 {
                *last &= (1_u64 << live) - 1;
            }
        }
        Ok(value)
    }

    pub fn bit_len(&self) -> usize {
        self.bit_len
    }
    pub fn words(&self) -> &[u64] {
        &self.words
    }
    pub fn contains(&self, index: usize) -> bool {
        index < self.bit_len && self.words[index / 64] & (1_u64 << (index % 64)) != 0
    }
    pub fn insert(&mut self, index: usize) -> Result<bool, OperationOrderLanguageError> {
        self.check_index(index)?;
        let mask = 1_u64 << (index % 64);
        let word = &mut self.words[index / 64];
        let changed = *word & mask == 0;
        *word |= mask;
        Ok(changed)
    }
    pub fn remove(&mut self, index: usize) -> Result<bool, OperationOrderLanguageError> {
        self.check_index(index)?;
        let mask = 1_u64 << (index % 64);
        let word = &mut self.words[index / 64];
        let changed = *word & mask != 0;
        *word &= !mask;
        Ok(changed)
    }
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|word| *word == 0)
    }
    pub fn count_ones(&self) -> usize {
        self.words
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }
    pub fn is_subset_of(&self, other: &Self) -> bool {
        self.bit_len == other.bit_len
            && self
                .words
                .iter()
                .zip(&other.words)
                .all(|(left, right)| left & !right == 0)
    }
    pub fn intersect_assign(&mut self, other: &Self) -> Result<(), OperationOrderLanguageError> {
        self.check_compatible(other)?;
        for (left, right) in self.words.iter_mut().zip(&other.words) {
            *left &= *right;
        }
        Ok(())
    }
    pub fn union_assign(&mut self, other: &Self) -> Result<(), OperationOrderLanguageError> {
        self.check_compatible(other)?;
        for (left, right) in self.words.iter_mut().zip(&other.words) {
            *left |= *right;
        }
        Ok(())
    }
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words
            .iter()
            .enumerate()
            .flat_map(move |(word_index, word)| {
                let mut remaining = *word;
                std::iter::from_fn(move || {
                    if remaining == 0 {
                        return None;
                    }
                    let bit = remaining.trailing_zeros() as usize;
                    remaining &= remaining - 1;
                    let index = word_index * 64 + bit;
                    (index < self.bit_len).then_some(index)
                })
            })
    }
    fn check_index(&self, index: usize) -> Result<(), OperationOrderLanguageError> {
        if index >= self.bit_len {
            return Err(OperationOrderLanguageError::BitIndexOutOfBounds {
                index,
                bit_len: self.bit_len,
            });
        }
        Ok(())
    }
    fn check_compatible(&self, other: &Self) -> Result<(), OperationOrderLanguageError> {
        if self.bit_len != other.bit_len {
            return Err(OperationOrderLanguageError::IncompatibleBitSets {
                left: self.bit_len,
                right: other.bit_len,
            });
        }
        Ok(())
    }
}

/// Minimal arbitrary-precision unsigned integer for exact decimal language counts.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExactDecimalCount {
    limbs: Vec<u32>,
}

impl ExactDecimalCount {
    const BASE: u64 = 1_000_000_000;
    pub fn zero() -> Self {
        Self { limbs: vec![0] }
    }
    pub fn one() -> Self {
        Self { limbs: vec![1] }
    }
    pub fn from_usize(value: usize) -> Self {
        let mut remaining = value as u128;
        let mut limbs = Vec::new();
        while remaining != 0 {
            limbs.push((remaining % Self::BASE as u128) as u32);
            remaining /= Self::BASE as u128;
        }
        if limbs.is_empty() {
            limbs.push(0);
        }
        Self { limbs }
    }
    pub fn factorial(value: usize) -> Self {
        let mut result = Self::one();
        for factor in 2..=value {
            result.multiply_small(factor as u32);
        }
        result
    }
    pub fn add_assign(&mut self, other: &Self) {
        let length = self.limbs.len().max(other.limbs.len());
        self.limbs.resize(length, 0);
        let mut carry = 0_u64;
        for index in 0..length {
            let rhs = u64::from(*other.limbs.get(index).unwrap_or(&0));
            let sum = u64::from(self.limbs[index]) + rhs + carry;
            self.limbs[index] = (sum % Self::BASE) as u32;
            carry = sum / Self::BASE;
        }
        if carry != 0 {
            self.limbs.push(carry as u32);
        }
    }
    pub fn multiply(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        let mut result = Self::zero();
        result.limbs = vec![0; self.limbs.len() + other.limbs.len()];
        for (left_index, left) in self.limbs.iter().enumerate() {
            let mut carry = 0_u64;
            for (right_index, right) in other.limbs.iter().enumerate() {
                let index = left_index + right_index;
                let value =
                    u64::from(result.limbs[index]) + u64::from(*left) * u64::from(*right) + carry;
                result.limbs[index] = (value % Self::BASE) as u32;
                carry = value / Self::BASE;
            }
            let mut index = left_index + other.limbs.len();
            while carry != 0 {
                let value = u64::from(result.limbs[index]) + carry;
                result.limbs[index] = (value % Self::BASE) as u32;
                carry = value / Self::BASE;
                index += 1;
                if index == result.limbs.len() && carry != 0 {
                    result.limbs.push(0);
                }
            }
        }
        while result.limbs.len() > 1 && result.limbs.last() == Some(&0) {
            result.limbs.pop();
        }
        result
    }
    pub fn is_zero(&self) -> bool {
        self.limbs.len() == 1 && self.limbs[0] == 0
    }
    pub fn checked_usize(&self) -> Option<usize> {
        let mut value = 0_usize;
        for limb in self.limbs.iter().rev() {
            value = value.checked_mul(Self::BASE as usize)?;
            value = value.checked_add(*limb as usize)?;
        }
        Some(value)
    }
    fn multiply_small(&mut self, factor: u32) {
        let mut carry = 0_u64;
        for limb in &mut self.limbs {
            let value = u64::from(*limb) * u64::from(factor) + carry;
            *limb = (value % Self::BASE) as u32;
            carry = value / Self::BASE;
        }
        while carry != 0 {
            self.limbs.push((carry % Self::BASE) as u32);
            carry /= Self::BASE;
        }
    }
}

impl fmt::Display for ExactDecimalCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.limbs.last().copied().unwrap_or(0))?;
        for limb in self.limbs.iter().rev().skip(1) {
            write!(formatter, "{limb:09}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationDependencyEdge {
    pub predecessor: OperationId,
    pub successor: OperationId,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OperationDependencyEvidenceKind {
    UniversalLanguage,
    BoardCollision,
    LineClearRemap,
    Reachability,
    FirstSuccessKick,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationDependencyEvidence {
    pub edge: OperationDependencyEdge,
    pub kinds: Vec<OperationDependencyEvidenceKind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationDependencyGraph {
    operation_ids: Vec<OperationId>,
    universal_predecessors: Vec<OperationBitSet>,
    universal_precedence_closure: Vec<OperationDependencyEdge>,
    transitive_reduction: Vec<OperationDependencyEdge>,
    evidence: Vec<OperationDependencyEvidence>,
    exact_order_count: ExactDecimalCount,
    explored_state_count: usize,
    live_transition_count: usize,
    complete: bool,
}

impl OperationDependencyGraph {
    pub fn operation_ids(&self) -> &[OperationId] {
        &self.operation_ids
    }
    pub fn universal_predecessors(&self) -> &[OperationBitSet] {
        &self.universal_predecessors
    }
    pub fn universal_precedence_closure(&self) -> &[OperationDependencyEdge] {
        &self.universal_precedence_closure
    }
    pub fn transitive_reduction(&self) -> &[OperationDependencyEdge] {
        &self.transitive_reduction
    }
    pub fn evidence(&self) -> &[OperationDependencyEvidence] {
        &self.evidence
    }
    pub fn exact_order_count(&self) -> &ExactDecimalCount {
        &self.exact_order_count
    }
    pub fn explored_state_count(&self) -> usize {
        self.explored_state_count
    }
    pub fn live_transition_count(&self) -> usize {
        self.live_transition_count
    }
    pub fn complete(&self) -> bool {
        self.complete
    }
    pub fn independent_pairs(&self) -> impl Iterator<Item = (OperationId, OperationId)> + '_ {
        self.operation_ids
            .iter()
            .enumerate()
            .flat_map(move |(left, left_id)| {
                self.operation_ids
                    .iter()
                    .enumerate()
                    .skip(left + 1)
                    .filter_map(move |(right, right_id)| {
                        (!self.universal_predecessors[right].contains(left)
                            && !self.universal_predecessors[left].contains(right))
                        .then_some((*left_id, *right_id))
                    })
            })
    }
    pub fn independent_pair_count(&self) -> usize {
        self.independent_pairs().count()
    }
    pub fn accepts_order(&self, order: &[OperationId]) -> bool {
        if self.exact_order_count.is_zero() {
            return false;
        }
        if order.len() != self.operation_ids.len() {
            return false;
        }
        let mut placed = match OperationBitSet::new(order.len()) {
            Ok(value) => value,
            Err(_) => return false,
        };
        for operation in order {
            let Ok(index) = self.operation_ids.binary_search(operation) else {
                return false;
            };
            if placed.contains(index) || !self.universal_predecessors[index].is_subset_of(&placed) {
                return false;
            }
            if placed.insert(index).is_err() {
                return false;
            }
        }
        true
    }
    pub fn from_complete_analysis(
        operation_ids: Vec<OperationId>,
        universal_predecessors: Vec<OperationBitSet>,
        exact_order_count: ExactDecimalCount,
        explored_state_count: usize,
        live_transition_count: usize,
        evidence_kinds: BTreeMap<OperationDependencyEdge, Vec<OperationDependencyEvidenceKind>>,
    ) -> Result<Self, OperationOrderLanguageError> {
        validate_operation_ids(&operation_ids)?;
        let count = operation_ids.len();
        if universal_predecessors.len() != count
            || universal_predecessors
                .iter()
                .any(|set| set.bit_len() != count)
        {
            return Err(OperationOrderLanguageError::InvalidDependencyGraph);
        }
        let closure = closure_edges(&operation_ids, &universal_predecessors);
        let reduction = transitive_reduction(&operation_ids, &universal_predecessors);
        let evidence = closure
            .iter()
            .copied()
            .map(|edge| {
                let mut kinds = evidence_kinds.get(&edge).cloned().unwrap_or_default();
                if !kinds.contains(&OperationDependencyEvidenceKind::UniversalLanguage) {
                    kinds.push(OperationDependencyEvidenceKind::UniversalLanguage);
                }
                kinds.sort_unstable();
                kinds.dedup();
                OperationDependencyEvidence { edge, kinds }
            })
            .collect();
        Ok(Self {
            operation_ids,
            universal_predecessors,
            universal_precedence_closure: closure,
            transitive_reduction: reduction,
            evidence,
            exact_order_count,
            explored_state_count,
            live_transition_count,
            complete: true,
        })
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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationOrderAutomatonTransition {
    pub operation_id: OperationId,
    pub target_state: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationOrderAutomatonState {
    pub accepting: bool,
    pub transitions: Vec<OperationOrderAutomatonTransition>,
}

/// A deterministic acyclic automaton whose accepted words are exact operation orders.
///
/// States may converge when distinct prefixes produce the same board state, but every
/// path consumes each operation id at most once. The constructor verifies that all
/// states are reachable, every convergence has the same consumed operation set, and
/// terminal acceptance occurs only after the entire operation set has been consumed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactOperationOrderAutomaton {
    operation_ids: Vec<OperationId>,
    root_state: usize,
    states: Vec<OperationOrderAutomatonState>,
    exact_order_count: ExactDecimalCount,
}

impl ExactOperationOrderAutomaton {
    pub fn try_new(
        operation_ids: Vec<OperationId>,
        root_state: usize,
        states: Vec<OperationOrderAutomatonState>,
    ) -> Result<Self, OperationOrderLanguageError> {
        validate_operation_ids(&operation_ids)?;
        if states.is_empty() || root_state >= states.len() {
            return Err(OperationOrderLanguageError::InvalidOrderAutomaton);
        }

        let mut consumed = vec![None; states.len()];
        consumed[root_state] = Some(OperationBitSet::new(operation_ids.len())?);
        let mut queue = VecDeque::from([root_state]);
        while let Some(state_index) = queue.pop_front() {
            let state_consumed = consumed[state_index]
                .clone()
                .ok_or(OperationOrderLanguageError::InvalidOrderAutomaton)?;
            let state = &states[state_index];
            if state.accepting != (state_consumed.count_ones() == operation_ids.len()) {
                return Err(OperationOrderLanguageError::InvalidOrderAutomaton);
            }
            if state
                .transitions
                .windows(2)
                .any(|pair| pair[0].operation_id >= pair[1].operation_id)
            {
                return Err(OperationOrderLanguageError::InvalidOrderAutomaton);
            }
            for transition in &state.transitions {
                if transition.target_state >= states.len() {
                    return Err(OperationOrderLanguageError::InvalidOrderAutomaton);
                }
                let operation_index = operation_ids
                    .binary_search(&transition.operation_id)
                    .map_err(|_| {
                        OperationOrderLanguageError::UnknownOperationId(transition.operation_id)
                    })?;
                if state_consumed.contains(operation_index) {
                    return Err(OperationOrderLanguageError::InvalidOrderAutomaton);
                }
                let mut child_consumed = state_consumed.clone();
                child_consumed.insert(operation_index)?;
                match &consumed[transition.target_state] {
                    Some(existing) if existing != &child_consumed => {
                        return Err(OperationOrderLanguageError::InvalidOrderAutomaton);
                    }
                    Some(_) => {}
                    None => {
                        consumed[transition.target_state] = Some(child_consumed);
                        queue.push_back(transition.target_state);
                    }
                }
            }
        }
        if consumed.iter().any(Option::is_none) {
            return Err(OperationOrderLanguageError::InvalidOrderAutomaton);
        }

        let mut topological: Vec<_> = (0..states.len()).collect();
        topological.sort_unstable_by_key(|index| {
            consumed[*index]
                .as_ref()
                .map_or(usize::MAX, OperationBitSet::count_ones)
        });
        let mut path_counts = vec![ExactDecimalCount::zero(); states.len()];
        path_counts[root_state] = ExactDecimalCount::one();
        for state_index in topological {
            let source_count = path_counts[state_index].clone();
            for transition in &states[state_index].transitions {
                path_counts[transition.target_state].add_assign(&source_count);
            }
        }
        let mut exact_order_count = ExactDecimalCount::zero();
        for (state, path_count) in states.iter().zip(path_counts) {
            if state.accepting {
                exact_order_count.add_assign(&path_count);
            }
        }

        Ok(Self {
            operation_ids,
            root_state,
            states,
            exact_order_count,
        })
    }

    pub fn operation_ids(&self) -> &[OperationId] {
        &self.operation_ids
    }
    pub fn root_state(&self) -> usize {
        self.root_state
    }
    pub fn states(&self) -> &[OperationOrderAutomatonState] {
        &self.states
    }
    pub fn exact_order_count(&self) -> &ExactDecimalCount {
        &self.exact_order_count
    }

    pub fn accepts_order(&self, order: &[OperationId]) -> bool {
        if order.len() != self.operation_ids.len() {
            return false;
        }
        let mut state_index = self.root_state;
        for operation_id in order {
            let transitions = &self.states[state_index].transitions;
            let Ok(index) =
                transitions.binary_search_by_key(operation_id, |edge| edge.operation_id)
            else {
                return false;
            };
            state_index = transitions[index].target_state;
        }
        self.states[state_index].accepting
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationOrderLanguage {
    pub candidate_id: CandidateId,
    pub operation_set_key: OperationSetKey,
    pub dependency_constraints: OperationDependencyGraph,
    pub line_clear_constraints: LineClearConstraintSet,
    pub reachability_constraints: ReachabilityConstraintSet,
    materialized_orders: Option<Vec<Vec<OperationId>>>,
    exact_automaton: Option<ExactOperationOrderAutomaton>,
}

pub type BuildOrderLanguage = OperationOrderLanguage;

impl OperationOrderLanguage {
    pub fn from_orders(
        candidate_id: CandidateId,
        operation_set_key: OperationSetKey,
        mut orders: Vec<Vec<OperationId>>,
    ) -> Self {
        orders.sort();
        orders.dedup();
        let operation_ids = orders.first().cloned().unwrap_or_default();
        let graph =
            graph_from_orders(&operation_ids, &orders).expect("legacy order language is valid");
        Self {
            candidate_id,
            operation_set_key,
            dependency_constraints: graph,
            line_clear_constraints: LineClearConstraintSet { complete: true },
            reachability_constraints: ReachabilityConstraintSet { complete: true },
            materialized_orders: Some(orders),
            exact_automaton: None,
        }
    }
    pub fn from_precedence_edges(
        candidate_id: CandidateId,
        operation_set_key: OperationSetKey,
        mut operation_ids: Vec<OperationId>,
        edges: &[OperationDependencyEdge],
    ) -> Result<Self, OperationOrderLanguageError> {
        operation_ids.sort_unstable();
        validate_operation_ids(&operation_ids)?;
        let required = predecessor_closure(&operation_ids, edges)?;
        let count = count_linear_extensions(&required)?;
        let graph = OperationDependencyGraph::from_complete_analysis(
            operation_ids,
            required,
            count,
            0,
            0,
            BTreeMap::new(),
        )?;
        Ok(Self::from_complete_graph(
            candidate_id,
            operation_set_key,
            graph,
        ))
    }
    pub fn from_complete_graph(
        candidate_id: CandidateId,
        operation_set_key: OperationSetKey,
        graph: OperationDependencyGraph,
    ) -> Self {
        Self {
            candidate_id,
            operation_set_key,
            dependency_constraints: graph,
            line_clear_constraints: LineClearConstraintSet { complete: true },
            reachability_constraints: ReachabilityConstraintSet { complete: true },
            materialized_orders: None,
            exact_automaton: None,
        }
    }
    pub fn from_complete_automaton(
        candidate_id: CandidateId,
        operation_set_key: OperationSetKey,
        graph: OperationDependencyGraph,
        automaton: ExactOperationOrderAutomaton,
    ) -> Result<Self, OperationOrderLanguageError> {
        if graph.operation_ids() != automaton.operation_ids()
            || graph.exact_order_count() != automaton.exact_order_count()
        {
            return Err(OperationOrderLanguageError::InvalidOrderAutomaton);
        }
        Ok(Self {
            candidate_id,
            operation_set_key,
            dependency_constraints: graph,
            line_clear_constraints: LineClearConstraintSet { complete: true },
            reachability_constraints: ReachabilityConstraintSet { complete: true },
            materialized_orders: None,
            exact_automaton: Some(automaton),
        })
    }
    pub fn accepts_order(&self, order: &[OperationId]) -> bool {
        if let Some(orders) = &self.materialized_orders {
            return orders.iter().any(|accepted| accepted == order);
        }
        if let Some(automaton) = &self.exact_automaton {
            return automaton.accepts_order(order);
        }
        self.dependency_constraints.accepts_order(order)
    }
    pub fn exact_order_count(&self) -> &ExactDecimalCount {
        self.dependency_constraints.exact_order_count()
    }
    pub fn order_count(&self) -> Option<usize> {
        self.exact_order_count().checked_usize()
    }
    pub fn exact_automaton(&self) -> Option<&ExactOperationOrderAutomaton> {
        self.exact_automaton.as_ref()
    }
    pub fn orders(&self) -> OperationOrderIter<'_> {
        if self.exact_order_count().is_zero() {
            return OperationOrderIter::Empty;
        }
        if let Some(orders) = &self.materialized_orders {
            return OperationOrderIter::Materialized(orders.iter());
        }
        if let Some(automaton) = &self.exact_automaton {
            return OperationOrderIter::Automaton(ExactAutomatonOrderIter::new(automaton));
        }
        OperationOrderIter::Topological(TopologicalOrderIter::new(
            self.dependency_constraints.operation_ids(),
            self.dependency_constraints.universal_predecessors(),
        ))
    }
}

pub enum OperationOrderIter<'a> {
    Empty,
    Materialized(std::slice::Iter<'a, Vec<OperationId>>),
    Automaton(ExactAutomatonOrderIter<'a>),
    Topological(TopologicalOrderIter<'a>),
}
impl Iterator for OperationOrderIter<'_> {
    type Item = Vec<OperationId>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::Materialized(iter) => iter.next().cloned(),
            Self::Automaton(iter) => iter.next(),
            Self::Topological(iter) => iter.next(),
        }
    }
}

struct ExactAutomatonOrderFrame {
    state_index: usize,
    next_transition: usize,
    accepting_yielded: bool,
}

pub struct ExactAutomatonOrderIter<'a> {
    automaton: &'a ExactOperationOrderAutomaton,
    frames: Vec<ExactAutomatonOrderFrame>,
    prefix: Vec<OperationId>,
}

impl<'a> ExactAutomatonOrderIter<'a> {
    fn new(automaton: &'a ExactOperationOrderAutomaton) -> Self {
        Self {
            automaton,
            frames: vec![ExactAutomatonOrderFrame {
                state_index: automaton.root_state,
                next_transition: 0,
                accepting_yielded: false,
            }],
            prefix: Vec::with_capacity(automaton.operation_ids.len()),
        }
    }
}

impl Iterator for ExactAutomatonOrderIter<'_> {
    type Item = Vec<OperationId>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let frame = self.frames.last_mut()?;
            let state = &self.automaton.states[frame.state_index];
            if state.accepting && !frame.accepting_yielded {
                frame.accepting_yielded = true;
                return Some(self.prefix.clone());
            }
            if let Some(transition) = state.transitions.get(frame.next_transition).copied() {
                frame.next_transition += 1;
                self.prefix.push(transition.operation_id);
                self.frames.push(ExactAutomatonOrderFrame {
                    state_index: transition.target_state,
                    next_transition: 0,
                    accepting_yielded: false,
                });
                continue;
            }
            self.frames.pop();
            if !self.frames.is_empty() {
                self.prefix.pop();
            }
        }
    }
}

pub struct TopologicalOrderIter<'a> {
    operation_ids: &'a [OperationId],
    required: &'a [OperationBitSet],
    prefix: Vec<usize>,
    placed: OperationBitSet,
    next_candidates: Vec<usize>,
    finished: bool,
}
impl<'a> TopologicalOrderIter<'a> {
    fn new(operation_ids: &'a [OperationId], required: &'a [OperationBitSet]) -> Self {
        Self {
            operation_ids,
            required,
            prefix: Vec::with_capacity(operation_ids.len()),
            placed: OperationBitSet::new(operation_ids.len()).expect("validated operation count"),
            next_candidates: vec![0],
            finished: false,
        }
    }
}
impl Iterator for TopologicalOrderIter<'_> {
    type Item = Vec<OperationId>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        loop {
            if self.prefix.len() == self.operation_ids.len() {
                let result = self
                    .prefix
                    .iter()
                    .map(|index| self.operation_ids[*index])
                    .collect();
                if let Some(index) = self.prefix.pop() {
                    let _ = self.placed.remove(index);
                    self.next_candidates.pop();
                } else {
                    self.finished = true;
                }
                return Some(result);
            }
            let depth = self.prefix.len();
            let mut candidate = self.next_candidates[depth];
            while candidate < self.operation_ids.len()
                && (self.placed.contains(candidate)
                    || !self.required[candidate].is_subset_of(&self.placed))
            {
                candidate += 1;
            }
            if candidate < self.operation_ids.len() {
                self.next_candidates[depth] = candidate + 1;
                let _ = self.placed.insert(candidate);
                self.prefix.push(candidate);
                self.next_candidates.push(0);
                continue;
            }
            self.next_candidates[depth] = 0;
            if let Some(index) = self.prefix.pop() {
                let _ = self.placed.remove(index);
                self.next_candidates.pop();
            } else {
                self.finished = true;
                return None;
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationOrderLanguageError {
    OperationLimitExceeded {
        operation_count: usize,
        maximum: usize,
    },
    DuplicateOperationId(OperationId),
    UnknownOperationId(OperationId),
    BitIndexOutOfBounds {
        index: usize,
        bit_len: usize,
    },
    IncompatibleBitSets {
        left: usize,
        right: usize,
    },
    DependencyCycle,
    InvalidDependencyGraph,
    InvalidOrderAutomaton,
    InvalidTimeoutSeconds {
        requested: u16,
        maximum: u16,
    },
    Cancelled,
    TimedOut {
        timeout_seconds: u16,
    },
    Incomplete {
        reason: &'static str,
    },
}
impl fmt::Display for OperationOrderLanguageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for OperationOrderLanguageError {}

fn validate_operation_ids(ids: &[OperationId]) -> Result<(), OperationOrderLanguageError> {
    if ids.len() > MAX_OPERATION_ORDER_OPERATIONS {
        return Err(OperationOrderLanguageError::OperationLimitExceeded {
            operation_count: ids.len(),
            maximum: MAX_OPERATION_ORDER_OPERATIONS,
        });
    }
    for pair in ids.windows(2) {
        if pair[0] == pair[1] {
            return Err(OperationOrderLanguageError::DuplicateOperationId(pair[0]));
        }
        if pair[0] > pair[1] {
            return Err(OperationOrderLanguageError::InvalidDependencyGraph);
        }
    }
    Ok(())
}
fn graph_from_orders(
    operation_ids: &[OperationId],
    orders: &[Vec<OperationId>],
) -> Result<OperationDependencyGraph, OperationOrderLanguageError> {
    let mut ids = operation_ids.to_vec();
    ids.sort_unstable();
    validate_operation_ids(&ids)?;
    let mut required = vec![OperationBitSet::full(ids.len())?; ids.len()];
    for (index, set) in required.iter_mut().enumerate() {
        set.remove(index)?;
    }
    for order in orders {
        let mut seen = OperationBitSet::new(ids.len())?;
        for operation in order {
            let index = ids
                .binary_search(operation)
                .map_err(|_| OperationOrderLanguageError::UnknownOperationId(*operation))?;
            required[index].intersect_assign(&seen)?;
            seen.insert(index)?;
        }
    }
    OperationDependencyGraph::from_complete_analysis(
        ids,
        required,
        ExactDecimalCount::from_usize(orders.len()),
        0,
        0,
        BTreeMap::new(),
    )
}
fn predecessor_closure(
    operation_ids: &[OperationId],
    edges: &[OperationDependencyEdge],
) -> Result<Vec<OperationBitSet>, OperationOrderLanguageError> {
    let mut required = vec![OperationBitSet::new(operation_ids.len())?; operation_ids.len()];
    for edge in edges {
        let predecessor = operation_ids
            .binary_search(&edge.predecessor)
            .map_err(|_| OperationOrderLanguageError::UnknownOperationId(edge.predecessor))?;
        let successor = operation_ids
            .binary_search(&edge.successor)
            .map_err(|_| OperationOrderLanguageError::UnknownOperationId(edge.successor))?;
        if predecessor == successor {
            return Err(OperationOrderLanguageError::DependencyCycle);
        }
        required[successor].insert(predecessor)?;
    }
    for intermediate in 0..operation_ids.len() {
        let inherited = required[intermediate].clone();
        for successor in 0..operation_ids.len() {
            if required[successor].contains(intermediate) {
                required[successor].union_assign(&inherited)?;
            }
        }
    }
    if required
        .iter()
        .enumerate()
        .any(|(index, set)| set.contains(index))
    {
        return Err(OperationOrderLanguageError::DependencyCycle);
    }
    Ok(required)
}
fn closure_edges(
    operation_ids: &[OperationId],
    required: &[OperationBitSet],
) -> Vec<OperationDependencyEdge> {
    let mut result = Vec::new();
    for (successor, predecessors) in required.iter().enumerate() {
        for predecessor in predecessors.iter() {
            result.push(OperationDependencyEdge {
                predecessor: operation_ids[predecessor],
                successor: operation_ids[successor],
            });
        }
    }
    result.sort_unstable();
    result
}
fn transitive_reduction(
    operation_ids: &[OperationId],
    required: &[OperationBitSet],
) -> Vec<OperationDependencyEdge> {
    let mut result = Vec::new();
    let mut topological: Vec<_> = (0..operation_ids.len()).collect();
    topological
        .sort_unstable_by_key(|index| (required[*index].count_ones(), operation_ids[*index]));
    for successor in 0..operation_ids.len() {
        let mut covered =
            OperationBitSet::new(operation_ids.len()).expect("validated operation count");
        for predecessor in topological.iter().rev().copied() {
            if required[successor].contains(predecessor) && !covered.contains(predecessor) {
                result.push(OperationDependencyEdge {
                    predecessor: operation_ids[predecessor],
                    successor: operation_ids[successor],
                });
                covered
                    .union_assign(&required[predecessor])
                    .expect("same operation span");
                covered
                    .insert(predecessor)
                    .expect("validated operation index");
            }
        }
    }
    result.sort_unstable();
    result
}
fn count_linear_extensions(
    required: &[OperationBitSet],
) -> Result<ExactDecimalCount, OperationOrderLanguageError> {
    let count = required.len();
    if required.iter().all(OperationBitSet::is_empty) {
        return Ok(ExactDecimalCount::factorial(count));
    }
    if required
        .iter()
        .map(OperationBitSet::count_ones)
        .sum::<usize>()
        == count.saturating_mul(count.saturating_sub(1)) / 2
    {
        return Ok(ExactDecimalCount::one());
    }
    fn visit(
        required: &[OperationBitSet],
        placed: &OperationBitSet,
        memo: &mut BTreeMap<OperationBitSet, ExactDecimalCount>,
    ) -> Result<ExactDecimalCount, OperationOrderLanguageError> {
        if placed.count_ones() == required.len() {
            return Ok(ExactDecimalCount::one());
        }
        if let Some(value) = memo.get(placed) {
            return Ok(value.clone());
        }
        let mut total = ExactDecimalCount::zero();
        for index in 0..required.len() {
            if !placed.contains(index) && required[index].is_subset_of(placed) {
                let mut next = placed.clone();
                next.insert(index)?;
                total.add_assign(&visit(required, &next, memo)?);
            }
        }
        memo.insert(placed.clone(), total.clone());
        Ok(total)
    }
    visit(
        required,
        &OperationBitSet::new(count)?,
        &mut BTreeMap::new(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_operations_cross_word_boundary() {
        let ids = (0..=64).map(OperationId).collect();
        let language = OperationOrderLanguage::from_precedence_edges(
            CandidateId(1),
            OperationSetKey(2),
            ids,
            &[],
        )
        .unwrap();
        assert_eq!(
            language.exact_order_count().to_string(),
            ExactDecimalCount::factorial(65).to_string()
        );
        assert_eq!(
            language.dependency_constraints.independent_pair_count(),
            65 * 64 / 2
        );
    }
    #[test]
    fn maximum_chain_has_exact_closure_and_reduction() {
        let ids: Vec<_> = (0..MAX_OPERATION_ORDER_OPERATIONS)
            .map(|index| OperationId(index as u16))
            .collect();
        let edges: Vec<_> = ids
            .windows(2)
            .map(|pair| OperationDependencyEdge {
                predecessor: pair[0],
                successor: pair[1],
            })
            .collect();
        let language = OperationOrderLanguage::from_precedence_edges(
            CandidateId(3),
            OperationSetKey(4),
            ids,
            &edges,
        )
        .unwrap();
        assert_eq!(language.exact_order_count().to_string(), "1");
        assert_eq!(
            language.dependency_constraints.transitive_reduction().len(),
            4095
        );
        assert_eq!(
            language
                .dependency_constraints
                .universal_precedence_closure()
                .len(),
            4096 * 4095 / 2
        );
    }
    #[test]
    fn diamond_is_exact_and_lazy() {
        let edges = [
            OperationDependencyEdge {
                predecessor: OperationId(0),
                successor: OperationId(1),
            },
            OperationDependencyEdge {
                predecessor: OperationId(0),
                successor: OperationId(2),
            },
            OperationDependencyEdge {
                predecessor: OperationId(1),
                successor: OperationId(3),
            },
            OperationDependencyEdge {
                predecessor: OperationId(2),
                successor: OperationId(3),
            },
        ];
        let language = OperationOrderLanguage::from_precedence_edges(
            CandidateId(1),
            OperationSetKey(1),
            (0..4).map(OperationId).collect(),
            &edges,
        )
        .unwrap();
        assert_eq!(language.exact_order_count().to_string(), "2");
        assert_eq!(
            language.dependency_constraints.transitive_reduction(),
            &edges
        );
        assert_eq!(language.orders().take(2).count(), 2);
    }
    #[test]
    fn legacy_materialized_language_remains_exact() {
        let language = BuildOrderLanguage::from_orders(
            CandidateId(7),
            OperationSetKey(0xabc),
            vec![
                vec![OperationId(1), OperationId(2), OperationId(3)],
                vec![OperationId(2), OperationId(1), OperationId(3)],
            ],
        );
        assert_eq!(language.order_count(), Some(2));
        assert!(language.accepts_order(&[OperationId(2), OperationId(1), OperationId(3)]));
    }

    #[test]
    fn exact_automaton_preserves_non_poset_language() {
        let operation_ids = vec![OperationId(0), OperationId(1), OperationId(2)];
        let accepted_orders = vec![
            vec![OperationId(0), OperationId(1), OperationId(2)],
            vec![OperationId(1), OperationId(2), OperationId(0)],
        ];
        let graph = graph_from_orders(&operation_ids, &accepted_orders).unwrap();
        assert!(graph.accepts_order(&[OperationId(1), OperationId(0), OperationId(2),]));

        let automaton = ExactOperationOrderAutomaton::try_new(
            operation_ids,
            0,
            vec![
                OperationOrderAutomatonState {
                    accepting: false,
                    transitions: vec![
                        OperationOrderAutomatonTransition {
                            operation_id: OperationId(0),
                            target_state: 1,
                        },
                        OperationOrderAutomatonTransition {
                            operation_id: OperationId(1),
                            target_state: 4,
                        },
                    ],
                },
                OperationOrderAutomatonState {
                    accepting: false,
                    transitions: vec![OperationOrderAutomatonTransition {
                        operation_id: OperationId(1),
                        target_state: 2,
                    }],
                },
                OperationOrderAutomatonState {
                    accepting: false,
                    transitions: vec![OperationOrderAutomatonTransition {
                        operation_id: OperationId(2),
                        target_state: 3,
                    }],
                },
                OperationOrderAutomatonState {
                    accepting: true,
                    transitions: vec![],
                },
                OperationOrderAutomatonState {
                    accepting: false,
                    transitions: vec![OperationOrderAutomatonTransition {
                        operation_id: OperationId(2),
                        target_state: 5,
                    }],
                },
                OperationOrderAutomatonState {
                    accepting: false,
                    transitions: vec![OperationOrderAutomatonTransition {
                        operation_id: OperationId(0),
                        target_state: 6,
                    }],
                },
                OperationOrderAutomatonState {
                    accepting: true,
                    transitions: vec![],
                },
            ],
        )
        .unwrap();
        let language = OperationOrderLanguage::from_complete_automaton(
            CandidateId(8),
            OperationSetKey(9),
            graph,
            automaton,
        )
        .unwrap();
        assert_eq!(language.exact_order_count().to_string(), "2");
        assert_eq!(language.orders().collect::<Vec<_>>(), accepted_orders);
        assert!(!language.accepts_order(&[OperationId(1), OperationId(0), OperationId(2),]));
    }
}
