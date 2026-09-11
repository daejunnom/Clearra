//! Experimental CPU minimum synthesis over a physical common-diagram oracle.
//! No coverage matrix or legacy complete-source result is constructed here.
//! The provider owns physical, score, observation-policy and universe semantics.
//! An unfinished provider query must return Unknown, never ProvedEmpty.

use std::{collections::HashMap, ops::Range};

use clearra_core_domain::execution_cancellation::ExecutionControl;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImplicitSourceBinding {
    pub query_sha256: [u8; 32],
    pub original_identity_sha256: [u8; 32],
    pub original_row_count: usize,
    pub pattern_universe_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImplicitMinimumError {
    Cancelled,
    CapacityExceeded,
    WorkLimit,
    Unknown,
    SourceChanged,
    InvalidOracle,
    NotSolved,
}

/// A witness is one original logical diagram supporting *every* required queue.
/// Different legal physical histories for those queues are allowed. ProvedEmpty
/// requires exhaustion of all original identities and every temporal family in
/// `original_rows`; an unexpanded parent/family is Unknown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImplicitJointDecision {
    Witness(usize),
    ProvedEmpty,
    Unknown,
}

#[derive(Clone, Copy, Debug)]
pub struct ImplicitOracleBudget {
    pub remaining_work_steps: u64,
    /// Includes the oracle's own retained source, cache and transient peak.
    pub max_retained_bytes: u128,
}

/// Providers must enforce the supplied budget before work/allocation. Reporting
/// it only after an unbounded query is not a conforming implementation.
pub struct ImplicitOracleObservation<T> {
    /// Usage survives cancellation, an unfinished family and a provider error.
    pub result: Result<T, ImplicitMinimumError>,
    pub work_steps: u64,
    pub peak_retained_bytes: u128,
}

/// A complete original identity dictionary is retained in this first version.
/// Its ordering and source hash must be stable throughout a session. Required
/// pattern indices refer to the provider's immutable universe mapping, which
/// must retain the original full PatternIds (including sparse score universes).
///
/// Implementations are trusted local solver components, not certificate input
/// decoders. This experimental interface never grants a legacy v2 result or
/// production/release authority to a fixture or caller-supplied oracle.
pub trait ImplicitDiagramOracle {
    fn binding(&self) -> ImplicitSourceBinding;
    fn checked_retained_bytes(&self) -> Option<u128>;
    fn joint(
        &mut self,
        original_rows: Range<usize>,
        required_patterns: &[usize],
        budget: ImplicitOracleBudget,
        control: &ExecutionControl,
    ) -> ImplicitOracleObservation<ImplicitJointDecision>;
    /// None is valid only after a complete exact counterexample search over the
    /// required universe, including any global observation-policy finalizer.
    fn counterexample(
        &mut self,
        original_rows: &[usize],
        budget: ImplicitOracleBudget,
        control: &ExecutionControl,
    ) -> ImplicitOracleObservation<Option<usize>>;
}

#[derive(Clone, Copy, Debug)]
pub struct ImplicitMinimumLimits {
    pub max_work_steps: u64,
    pub max_retained_bytes: u128,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImplicitMinimumStatistics {
    pub assignment_nodes: u64,
    pub joint_queries: u64,
    pub counterexample_queries: u64,
    pub counterexamples_added: u64,
    pub negative_k_queries: u64,
    pub canonical_queries: u64,
    pub oracle_work_steps: u64,
    pub joint_cache_hits: u64,
    pub joint_cache_misses: u64,
    pub joint_cache_resets: u64,
    pub pairwise_queries: u64,
    pub pairwise_conflicts: u64,
    pub pairwise_pruned_choices: u64,
    pub pairwise_clique_lower_bound: usize,
    pub peak_retained_upper_bound_bytes: u128,
}

/// A bounded graph of exact two-queue failures over the complete original
/// domain. An edge only says that those queues cannot share one diagram; it
/// never substitutes for validating a positive multi-queue witness.
struct PairConflictGraph {
    words: Vec<u64>,
    row_words: usize,
    capacity: usize,
    prepared_patterns: usize,
    clique_bound: usize,
}

impl PairConflictGraph {
    fn retained_bytes(&self) -> Option<u128> {
        (self.words.capacity() as u128).checked_mul(core::mem::size_of::<u64>() as u128)
    }

    fn conflicts(&self, left: usize, right: usize) -> bool {
        left < self.capacity
            && right < self.capacity
            && self.words[left * self.row_words + right / 64] & (1 << (right % 64)) != 0
    }

    fn degree(&self, pattern: usize) -> usize {
        if pattern >= self.capacity {
            return 0;
        }
        self.words[pattern * self.row_words..(pattern + 1) * self.row_words]
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    fn insert(&mut self, left: usize, right: usize) {
        self.words[left * self.row_words + right / 64] |= 1 << (right % 64);
        self.words[right * self.row_words + left / 64] |= 1 << (left % 64);
    }
}

#[derive(Eq, Hash, PartialEq)]
struct JointQueryKey {
    start: usize,
    end: usize,
    patterns: Vec<usize>,
}

/// Exact memoization only: no learned inequality, subset implication or new
/// constraint. The enclosing controller checks the immutable source binding.
struct JointQueryCache {
    entries: HashMap<JointQueryKey, ImplicitJointDecision>,
    key_bytes: u128,
    max_entries: usize,
    max_bytes: u128,
}

impl JointQueryCache {
    fn bucket_bytes(capacity: usize) -> Option<u128> {
        if capacity == 0 {
            return Some(0);
        }
        // Conservative hash-table capacity/control/alignment upper bound.
        (capacity as u128)
            .checked_add(1)?
            .checked_mul(2)?
            .checked_mul(
                (core::mem::size_of::<JointQueryKey>()
                    + core::mem::size_of::<ImplicitJointDecision>()
                    + 1) as u128,
            )?
            .checked_add(64)
    }

    fn retained_bytes(&self) -> Option<u128> {
        Self::bucket_bytes(self.entries.capacity())?.checked_add(self.key_bytes)
    }
}

/// This is a separate typed experimental result. In particular, it does not
/// set coverage_rows_complete in an existing product result. Cardinality and
/// canonical evidence are minted only by the controller below.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImplicitMinimumResult {
    binding: ImplicitSourceBinding,
    original_rows: Vec<usize>,
    minimum_cardinality: usize,
}

impl ImplicitMinimumResult {
    pub const CONTRACT: &'static str = "physical-minimum-implicit.experimental.v1";
    pub const fn binding(&self) -> ImplicitSourceBinding {
        self.binding
    }
    pub fn original_rows(&self) -> &[usize] {
        &self.original_rows
    }
    pub const fn minimum_cardinality(&self) -> usize {
        self.minimum_cardinality
    }
}

pub struct ImplicitMinimumSearch<O> {
    oracle: O,
    binding: ImplicitSourceBinding,
    limits: ImplicitMinimumLimits,
    observed: Vec<usize>,
    work_steps: u64,
    statistics: ImplicitMinimumStatistics,
    minimum: Option<usize>,
    first: Option<Vec<usize>>,
    last: Option<Vec<usize>>,
    alternatives_exhausted: bool,
    joint_cache: Option<JointQueryCache>,
    pair_conflicts: Option<PairConflictGraph>,
}

fn reserved<T>(capacity: usize) -> Result<Vec<T>, ImplicitMinimumError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(capacity)
        .map_err(|_| ImplicitMinimumError::CapacityExceeded)?;
    Ok(output)
}

fn copied<T: Clone>(values: &[T]) -> Result<Vec<T>, ImplicitMinimumError> {
    let mut output = reserved(values.len())?;
    output.extend_from_slice(values);
    Ok(output)
}

impl<O: ImplicitDiagramOracle> ImplicitMinimumSearch<O> {
    pub fn new(oracle: O, limits: ImplicitMinimumLimits) -> Result<Self, ImplicitMinimumError> {
        let binding = oracle.binding();
        let observed_bytes = (binding.pattern_universe_count as u128)
            .checked_mul(core::mem::size_of::<usize>() as u128)
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Self>() as u128))
            .and_then(|bytes| bytes.checked_add(oracle.checked_retained_bytes()?))
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        if observed_bytes > limits.max_retained_bytes {
            return Err(ImplicitMinimumError::CapacityExceeded);
        }
        Ok(Self {
            oracle,
            binding,
            limits,
            observed: reserved(binding.pattern_universe_count)?,
            work_steps: 0,
            statistics: ImplicitMinimumStatistics::default(),
            minimum: None,
            first: None,
            last: None,
            alternatives_exhausted: false,
            joint_cache: None,
            pair_conflicts: None,
        })
    }

    pub const fn statistics(&self) -> ImplicitMinimumStatistics {
        self.statistics
    }
    pub fn oracle(&self) -> &O {
        &self.oracle
    }

    /// Configure once before searching. All complete positive/negative answers
    /// are scoped to this source, exact row range and exact required-pattern
    /// slice. Capacity pressure clears only this optional memo, never proof
    /// state. Unknown and errored observations are never inserted.
    pub fn set_joint_query_cache(
        &mut self,
        max_entries: usize,
        max_bytes: u128,
    ) -> Result<(), ImplicitMinimumError> {
        if self.work_steps != 0 {
            return Err(ImplicitMinimumError::InvalidOracle);
        }
        self.joint_cache = (max_entries != 0 && max_bytes != 0).then(|| JointQueryCache {
            entries: HashMap::new(),
            key_bytes: 0,
            max_entries,
            max_bytes,
        });
        Ok(())
    }

    fn joint_cache_retained_bytes(&self) -> Option<u128> {
        self.joint_cache
            .as_ref()
            .map_or(Some(0), JointQueryCache::retained_bytes)
    }

    /// Optional native experiment, configured before any search. Only the first
    /// `max_patterns` counterexamples receive pairwise propagation. Later ones
    /// still undergo the unchanged exact joint query, with no presumed edge.
    pub fn set_pairwise_conflict_ordering(
        &mut self,
        max_patterns: usize,
    ) -> Result<(), ImplicitMinimumError> {
        if self.work_steps != 0 {
            return Err(ImplicitMinimumError::InvalidOracle);
        }
        let capacity = max_patterns.min(self.binding.pattern_universe_count);
        if capacity == 0 {
            self.pair_conflicts = None;
            return Ok(());
        }
        let row_words = capacity.div_ceil(64);
        let count = capacity
            .checked_mul(row_words)
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        let future = (count as u128)
            .checked_mul(core::mem::size_of::<u64>() as u128)
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        // The current graph remains accounted for while a replacement allocates.
        let budget = self.oracle_budget(0)?;
        let oracle_bytes = self
            .oracle
            .checked_retained_bytes()
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        if future
            .checked_add(oracle_bytes)
            .ok_or(ImplicitMinimumError::CapacityExceeded)?
            > budget.max_retained_bytes
        {
            return Err(ImplicitMinimumError::CapacityExceeded);
        }
        let mut words = reserved(count)?;
        words.resize(count, 0);
        self.pair_conflicts = Some(PairConflictGraph {
            words,
            row_words,
            capacity,
            prepared_patterns: 0,
            clique_bound: 0,
        });
        Ok(())
    }

    fn pair_conflicts_retained_bytes(&self) -> Option<u128> {
        self.pair_conflicts
            .as_ref()
            .map_or(Some(0), PairConflictGraph::retained_bytes)
    }

    fn charge_work(&mut self, steps: u64) -> Result<(), ImplicitMinimumError> {
        self.work_steps = self
            .work_steps
            .checked_add(steps)
            .ok_or(ImplicitMinimumError::WorkLimit)?;
        if self.work_steps > self.limits.max_work_steps {
            return Err(ImplicitMinimumError::WorkLimit);
        }
        Ok(())
    }

    fn check(
        &mut self,
        control: &ExecutionControl,
        scratch: u128,
    ) -> Result<(), ImplicitMinimumError> {
        if control.is_cancelled() {
            return Err(ImplicitMinimumError::Cancelled);
        }
        if self.oracle.binding() != self.binding {
            return Err(ImplicitMinimumError::SourceChanged);
        }
        let retained = (self.observed.capacity() as u128)
            .checked_add(self.last.as_ref().map_or(0, |rows| rows.capacity()) as u128)
            .and_then(|count| {
                count.checked_add(self.first.as_ref().map_or(0, |rows| rows.capacity()) as u128)
            })
            .and_then(|count| count.checked_mul(core::mem::size_of::<usize>() as u128))
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Self>() as u128))
            .and_then(|bytes| bytes.checked_add(self.joint_cache_retained_bytes()?))
            .and_then(|bytes| bytes.checked_add(self.pair_conflicts_retained_bytes()?))
            .and_then(|bytes| bytes.checked_add(self.oracle.checked_retained_bytes()?))
            .and_then(|bytes| bytes.checked_add(scratch))
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        if retained > self.limits.max_retained_bytes {
            return Err(ImplicitMinimumError::CapacityExceeded);
        }
        self.statistics.peak_retained_upper_bound_bytes = self
            .statistics
            .peak_retained_upper_bound_bytes
            .max(retained);
        self.work_steps = self
            .work_steps
            .checked_add(1)
            .ok_or(ImplicitMinimumError::WorkLimit)?;
        if self.work_steps > self.limits.max_work_steps {
            return Err(ImplicitMinimumError::WorkLimit);
        }
        Ok(())
    }

    fn scratch_upper_bound(&self, slots: usize) -> Result<u128, ImplicitMinimumError> {
        // Slot requirements plus explicit DFS frames (no native/WASM recursion).
        // Also cover bounded-query ranges, witnesses and canonical prefix copies.
        let words = (slots as u128)
            .checked_mul(self.binding.pattern_universe_count as u128)
            .and_then(|n| {
                n.checked_add(
                    (self.binding.pattern_universe_count as u128)
                        .checked_mul(if self.joint_cache.is_some() { 9 } else { 8 })?,
                )
            })
            .and_then(|n| n.checked_add((slots as u128).checked_mul(20)?))
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        words
            .checked_mul(core::mem::size_of::<usize>() as u128)
            .ok_or(ImplicitMinimumError::CapacityExceeded)
    }

    fn oracle_budget(&self, scratch: u128) -> Result<ImplicitOracleBudget, ImplicitMinimumError> {
        let engine_bytes = (self.observed.capacity() as u128)
            .checked_add(self.last.as_ref().map_or(0, |rows| rows.capacity()) as u128)
            .and_then(|count| {
                count.checked_add(self.first.as_ref().map_or(0, |rows| rows.capacity()) as u128)
            })
            .and_then(|count| count.checked_mul(core::mem::size_of::<usize>() as u128))
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Self>() as u128))
            .and_then(|bytes| bytes.checked_add(self.joint_cache_retained_bytes()?))
            .and_then(|bytes| bytes.checked_add(self.pair_conflicts_retained_bytes()?))
            .and_then(|bytes| bytes.checked_add(scratch))
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        Ok(ImplicitOracleBudget {
            remaining_work_steps: self
                .limits
                .max_work_steps
                .checked_sub(self.work_steps)
                .ok_or(ImplicitMinimumError::WorkLimit)?,
            max_retained_bytes: self
                .limits
                .max_retained_bytes
                .checked_sub(engine_bytes)
                .ok_or(ImplicitMinimumError::CapacityExceeded)?,
        })
    }

    fn accept_observation<T>(
        &mut self,
        observation: ImplicitOracleObservation<T>,
        budget: ImplicitOracleBudget,
    ) -> Result<T, ImplicitMinimumError> {
        self.work_steps = self
            .work_steps
            .checked_add(observation.work_steps)
            .ok_or(ImplicitMinimumError::WorkLimit)?;
        self.statistics.oracle_work_steps = self
            .statistics
            .oracle_work_steps
            .checked_add(observation.work_steps)
            .ok_or(ImplicitMinimumError::WorkLimit)?;
        let total_peak = (self.limits.max_retained_bytes - budget.max_retained_bytes)
            .checked_add(observation.peak_retained_bytes)
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        self.statistics.peak_retained_upper_bound_bytes = self
            .statistics
            .peak_retained_upper_bound_bytes
            .max(total_peak);
        if observation.work_steps > budget.remaining_work_steps {
            return Err(ImplicitMinimumError::WorkLimit);
        }
        if observation.peak_retained_bytes > budget.max_retained_bytes {
            return Err(ImplicitMinimumError::CapacityExceeded);
        }
        observation.result
    }

    fn joint_query(
        &mut self,
        range: Range<usize>,
        patterns: &[usize],
        scratch: u128,
        control: &ExecutionControl,
    ) -> Result<ImplicitJointDecision, ImplicitMinimumError> {
        let key = if self.joint_cache.is_some() {
            // The scratch bound already reserves one universe-sized key.
            self.work_steps = self
                .work_steps
                .checked_add(patterns.len() as u64)
                .ok_or(ImplicitMinimumError::WorkLimit)?;
            if self.work_steps > self.limits.max_work_steps {
                return Err(ImplicitMinimumError::WorkLimit);
            }
            let key = JointQueryKey {
                start: range.start,
                end: range.end,
                patterns: copied(patterns)?,
            };
            if let Some(answer) = self
                .joint_cache
                .as_ref()
                .unwrap()
                .entries
                .get(&key)
                .copied()
            {
                self.statistics.joint_cache_hits += 1;
                return Ok(answer);
            }
            self.statistics.joint_cache_misses += 1;
            Some(key)
        } else {
            None
        };
        let budget = self.oracle_budget(scratch)?;
        let observation = self.oracle.joint(range.clone(), patterns, budget, control);
        let answer = self.accept_observation(observation, budget)?;
        if control.is_cancelled() {
            return Err(ImplicitMinimumError::Cancelled);
        }
        if self.oracle.binding() != self.binding {
            return Err(ImplicitMinimumError::SourceChanged);
        }
        if matches!(answer, ImplicitJointDecision::Witness(row) if !range.contains(&row)) {
            return Err(ImplicitMinimumError::InvalidOracle);
        }
        if let Some(key) = key {
            if answer != ImplicitJointDecision::Unknown {
                self.memoize_joint(key, answer, scratch)?;
            }
        }
        Ok(answer)
    }

    fn memoize_joint(
        &mut self,
        key: JointQueryKey,
        answer: ImplicitJointDecision,
        scratch: u128,
    ) -> Result<(), ImplicitMinimumError> {
        let cache = self.joint_cache.as_mut().unwrap();
        if cache.entries.len() == cache.max_entries {
            cache.entries.clear();
            cache.key_bytes = 0;
            self.statistics.joint_cache_resets += 1;
        }
        let key_bytes = (key.patterns.capacity() as u128)
            .checked_mul(core::mem::size_of::<usize>() as u128)
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        let growing = cache.entries.len() == cache.entries.capacity();
        let next_capacity = if growing {
            cache
                .entries
                .capacity()
                .checked_mul(2)
                .and_then(|n| n.checked_add(1))
                .ok_or(ImplicitMinimumError::CapacityExceeded)?
                .max(7)
        } else {
            cache.entries.capacity()
        };
        let next_buckets = JointQueryCache::bucket_bytes(next_capacity)
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        let next_retained = next_buckets
            .checked_add(cache.key_bytes)
            .and_then(|n| n.checked_add(key_bytes))
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        if next_retained > cache.max_bytes {
            return Ok(());
        }
        // Reserve old/new table coexistence before a possible reallocation.
        // The temporary key is also in scratch; double-counting it here is
        // conservative and keeps optional memo storage inside parent bounds.
        let future = key_bytes
            .checked_add(if growing { next_buckets } else { 0 })
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        let budget = self.oracle_budget(scratch)?;
        let oracle_retained = self
            .oracle
            .checked_retained_bytes()
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        let Some(available) = budget.max_retained_bytes.checked_sub(oracle_retained) else {
            return Ok(());
        };
        if future > available {
            return Ok(());
        }
        let peak = (self.limits.max_retained_bytes - budget.max_retained_bytes)
            .checked_add(oracle_retained)
            .and_then(|n| n.checked_add(future))
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        self.statistics.peak_retained_upper_bound_bytes =
            self.statistics.peak_retained_upper_bound_bytes.max(peak);
        let cache = self.joint_cache.as_mut().unwrap();
        if cache.entries.try_reserve(1).is_err() {
            return Ok(());
        }
        if cache.entries.capacity() > next_capacity {
            return Err(ImplicitMinimumError::CapacityExceeded);
        }
        cache.key_bytes = cache
            .key_bytes
            .checked_add(key_bytes)
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        cache.entries.insert(key, answer);
        Ok(())
    }

    fn prepare_pair_conflicts(
        &mut self,
        scratch: u128,
        control: &ExecutionControl,
    ) -> Result<(), ImplicitMinimumError> {
        let Some(graph) = self.pair_conflicts.as_ref() else {
            return Ok(());
        };
        let count = self.observed.len().min(graph.capacity);
        let start = graph.prepared_patterns;
        if start == count {
            return Ok(());
        }
        for right in start..count {
            for left in 0..right {
                self.check(control, scratch)?;
                let patterns = [self.observed[left], self.observed[right]];
                self.statistics.joint_queries += 1;
                self.statistics.pairwise_queries += 1;
                let outcome = self.joint_query(
                    0..self.binding.original_row_count,
                    &patterns,
                    scratch,
                    control,
                )?;
                self.check(control, scratch)?;
                match outcome {
                    ImplicitJointDecision::ProvedEmpty => {
                        let graph = self.pair_conflicts.as_mut().unwrap();
                        if !graph.conflicts(left, right) {
                            graph.insert(left, right);
                            self.statistics.pairwise_conflicts += 1;
                        }
                    }
                    ImplicitJointDecision::Witness(_) => {}
                    ImplicitJointDecision::Unknown => return Err(ImplicitMinimumError::Unknown),
                }
            }
            self.pair_conflicts.as_mut().unwrap().prepared_patterns = right + 1;
        }
        // Each greedily constructed clique is independently a necessary number
        // of slots. A missed larger clique loses pruning, never exactness. Charge
        // the degree/sort scans and every possible adjacency check beforehand.
        let n = count as u128;
        let row_words = self.pair_conflicts.as_ref().unwrap().row_words as u128;
        let work = n
            .checked_mul(n)
            .and_then(|square| square.checked_mul(n.checked_add(row_words)?.checked_add(2)?))
            .and_then(|work| u64::try_from(work).ok())
            .ok_or(ImplicitMinimumError::WorkLimit)?;
        self.charge_work(work)?;
        self.check(control, scratch)?;
        let mut order: Vec<usize> = reserved(count)?;
        order.extend(0..count);
        let mut clique: Vec<usize> = reserved(count)?;
        let graph = self.pair_conflicts.as_ref().unwrap();
        order.sort_unstable_by_key(|&pattern| (std::cmp::Reverse(graph.degree(pattern)), pattern));
        let mut lower = graph.clique_bound;
        for &seed in &order {
            clique.clear();
            clique.push(seed);
            for &pattern in &order {
                if pattern != seed && clique.iter().all(|&other| graph.conflicts(pattern, other)) {
                    clique.push(pattern);
                }
            }
            lower = lower.max(clique.len());
        }
        self.pair_conflicts.as_mut().unwrap().clique_bound = lower;
        self.statistics.pairwise_clique_lower_bound = lower;
        Ok(())
    }

    fn select_pattern(
        &mut self,
        slot_for_pattern: &[usize],
        blocked_slots: &mut [bool],
    ) -> Result<(usize, usize), ImplicitMinimumError> {
        let n = self.observed.len() as u128;
        let work = n
            .checked_mul(
                n.checked_add(
                    (blocked_slots.len() as u128)
                        .checked_mul(2)
                        .ok_or(ImplicitMinimumError::WorkLimit)?,
                )
                .and_then(|n| {
                    n.checked_add(self.pair_conflicts.as_ref().unwrap().row_words as u128)
                })
                .ok_or(ImplicitMinimumError::WorkLimit)?,
            )
            .and_then(|work| u64::try_from(work).ok())
            .ok_or(ImplicitMinimumError::WorkLimit)?;
        self.charge_work(work)?;
        let graph = self.pair_conflicts.as_ref().unwrap();
        let mut best = None;
        for pattern in 0..self.observed.len() {
            if slot_for_pattern[pattern] != usize::MAX {
                continue;
            }
            blocked_slots.fill(false);
            for (other, &slot) in slot_for_pattern.iter().enumerate() {
                if slot != usize::MAX && graph.conflicts(pattern, other) {
                    blocked_slots[slot] = true;
                }
            }
            let available = blocked_slots.iter().filter(|&&blocked| !blocked).count();
            let key = (available, std::cmp::Reverse(graph.degree(pattern)), pattern);
            if best.is_none_or(|old| key < old) {
                best = Some(key);
            }
        }
        best.map(|(available, _, pattern)| (pattern, available))
            .ok_or(ImplicitMinimumError::InvalidOracle)
    }

    fn synthesis(
        &mut self,
        bounds: &[Range<usize>],
        control: &ExecutionControl,
    ) -> Result<Option<Vec<usize>>, ImplicitMinimumError> {
        let scratch = self.scratch_upper_bound(bounds.len())?;
        self.check(control, scratch)?;
        self.prepare_pair_conflicts(scratch, control)?;
        if self
            .pair_conflicts
            .as_ref()
            .is_some_and(|graph| graph.clique_bound > bounds.len())
        {
            return Ok(None);
        }
        if bounds
            .iter()
            .any(|range| range.start >= range.end || range.end > self.binding.original_row_count)
        {
            return Ok(None);
        }
        let mut requirements: Vec<Vec<usize>> = reserved(bounds.len())?;
        let mut witnesses = reserved(bounds.len())?;
        for range in bounds {
            requirements.push(reserved(self.observed.len())?);
            // Empty queue constraints are vacuous over an existing original ID.
            witnesses.push(range.start);
        }
        let frame_count = self
            .observed
            .len()
            .checked_add(1)
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        let mut choices = reserved(frame_count)?;
        choices.resize(frame_count, 0_usize);
        let mut assigned = reserved(self.observed.len())?;
        assigned.resize(self.observed.len(), 0_usize);
        let mut previous = reserved(self.observed.len())?;
        previous.resize(self.observed.len(), 0_usize);
        let mut pattern_at_depth = reserved(self.observed.len())?;
        pattern_at_depth.resize(self.observed.len(), 0_usize);
        let mut slot_for_pattern = reserved(self.observed.len())?;
        slot_for_pattern.resize(self.observed.len(), usize::MAX);
        let mut blocked_slots = reserved(bounds.len())?;
        blocked_slots.resize(bounds.len(), false);
        let mut depth = 0;
        loop {
            self.check(control, scratch)?;
            if depth == self.observed.len() {
                witnesses.sort_unstable();
                witnesses.dedup();
                return Ok(Some(witnesses));
            }
            if choices[depth] == 0 {
                let (pattern, available) = if self.pair_conflicts.is_some() {
                    self.select_pattern(&slot_for_pattern, &mut blocked_slots)?
                } else {
                    (depth, bounds.len())
                };
                pattern_at_depth[depth] = pattern;
                if available == 0 {
                    self.statistics.pairwise_pruned_choices += bounds.len() as u64;
                    choices[depth] = bounds.len();
                }
            }
            if choices[depth] == bounds.len() {
                if depth == 0 {
                    return Ok(None);
                }
                depth -= 1;
                let slot = assigned[depth];
                requirements[slot].pop();
                witnesses[slot] = previous[depth];
                slot_for_pattern[pattern_at_depth[depth]] = usize::MAX;
                continue;
            }
            let slot = choices[depth];
            choices[depth] += 1;
            // Only indistinguishable empty slots may be permuted. Different
            // identity bounds during canonical selection are not symmetric.
            if requirements[slot].is_empty()
                && (0..slot)
                    .any(|other| bounds[other] == bounds[slot] && requirements[other].is_empty())
            {
                continue;
            }
            let pattern = pattern_at_depth[depth];
            if self.pair_conflicts.is_some() {
                self.charge_work(self.observed.len() as u64)?;
                let graph = self.pair_conflicts.as_ref().unwrap();
                if slot_for_pattern
                    .iter()
                    .enumerate()
                    .any(|(other, &assigned)| assigned == slot && graph.conflicts(pattern, other))
                {
                    self.statistics.pairwise_pruned_choices += 1;
                    continue;
                }
            }
            requirements[slot].push(self.observed[pattern]);
            self.statistics.assignment_nodes += 1;
            self.statistics.joint_queries += 1;
            let outcome =
                self.joint_query(bounds[slot].clone(), &requirements[slot], scratch, control)?;
            self.check(control, scratch)?;
            match outcome {
                ImplicitJointDecision::Witness(row) => {
                    if !bounds[slot].contains(&row) {
                        return Err(ImplicitMinimumError::InvalidOracle);
                    }
                    assigned[depth] = slot;
                    previous[depth] = witnesses[slot];
                    witnesses[slot] = row;
                    slot_for_pattern[pattern] = slot;
                    depth += 1;
                    choices[depth] = 0;
                }
                ImplicitJointDecision::ProvedEmpty => {
                    requirements[slot].pop();
                }
                ImplicitJointDecision::Unknown => return Err(ImplicitMinimumError::Unknown),
            }
        }
    }

    fn feasible(
        &mut self,
        bounds: &[Range<usize>],
        control: &ExecutionControl,
    ) -> Result<Option<Vec<usize>>, ImplicitMinimumError> {
        loop {
            let Some(witness) = self.synthesis(bounds, control)? else {
                return Ok(None);
            };
            self.check(control, self.scratch_upper_bound(bounds.len())?)?;
            self.statistics.counterexample_queries += 1;
            let budget = self.oracle_budget(self.scratch_upper_bound(bounds.len())?)?;
            let observation = self.oracle.counterexample(&witness, budget, control);
            let counterexample = self.accept_observation(observation, budget)?;
            self.check(control, self.scratch_upper_bound(bounds.len())?)?;
            match counterexample {
                None => return Ok(Some(witness)),
                Some(pattern) => {
                    if pattern >= self.binding.pattern_universe_count
                        || self.observed.contains(&pattern)
                    {
                        return Err(ImplicitMinimumError::InvalidOracle);
                    }
                    self.observed.push(pattern);
                    self.statistics.counterexamples_added += 1;
                }
            }
        }
    }

    fn bounds(
        &self,
        prefix: &[usize],
        lower: usize,
        slots: usize,
    ) -> Result<Vec<Range<usize>>, ImplicitMinimumError> {
        let mut result = reserved(slots)?;
        for &row in prefix {
            result.push(
                row..row
                    .checked_add(1)
                    .ok_or(ImplicitMinimumError::CapacityExceeded)?,
            );
        }
        for _ in prefix.len()..slots {
            result.push(lower..self.binding.original_row_count);
        }
        Ok(result)
    }

    fn canonical_completion(
        &mut self,
        mut prefix: Vec<usize>,
        mut lower: usize,
        mut witness: Vec<usize>,
        slots: usize,
        control: &ExecutionControl,
    ) -> Result<Vec<usize>, ImplicitMinimumError> {
        prefix
            .try_reserve_exact(slots.saturating_sub(prefix.len()))
            .map_err(|_| ImplicitMinimumError::CapacityExceeded)?;
        while prefix.len() < slots {
            let mut lo = lower;
            let mut hi = witness
                .iter()
                .copied()
                .find(|&row| row >= lower)
                .ok_or(ImplicitMinimumError::InvalidOracle)?;
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let mut bounds = self.bounds(&prefix, lower, slots)?;
                bounds[prefix.len()] = lower..mid + 1;
                self.statistics.canonical_queries += 1;
                if let Some(found) = self.feasible(&bounds, control)? {
                    hi = found
                        .iter()
                        .copied()
                        .find(|&row| row >= lower)
                        .ok_or(ImplicitMinimumError::InvalidOracle)?;
                    if hi > mid {
                        return Err(ImplicitMinimumError::InvalidOracle);
                    }
                    witness = found;
                } else {
                    lo = mid + 1;
                }
            }
            if !witness.contains(&lo) {
                return Err(ImplicitMinimumError::InvalidOracle);
            }
            prefix.push(lo);
            lower = lo
                .checked_add(1)
                .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        }
        if prefix != witness {
            return Err(ImplicitMinimumError::InvalidOracle);
        }
        Ok(prefix)
    }

    /// Native experimental controller. A failed/limited call returns no proof.
    /// Browser adoption additionally requires a resumable scheduler adapter.
    pub fn solve_minimum(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<Option<ImplicitMinimumResult>, ImplicitMinimumError> {
        self.check(control, 0)?;
        if let (Some(minimum), Some(first)) = (self.minimum, self.first.as_ref()) {
            return Ok(Some(ImplicitMinimumResult {
                binding: self.binding,
                original_rows: copied(first)?,
                minimum_cardinality: minimum,
            }));
        }
        let maximum = self
            .binding
            .original_row_count
            .min(self.binding.pattern_universe_count);
        for slots in 0..=maximum {
            self.check(control, self.scratch_upper_bound(slots)?)?;
            let bounds = self.bounds(&[], 0, slots)?;
            if let Some(witness) = self.feasible(&bounds, control)? {
                if witness.len() != slots {
                    return Err(ImplicitMinimumError::InvalidOracle);
                }
                let canonical =
                    self.canonical_completion(Vec::new(), 0, witness, slots, control)?;
                let result = ImplicitMinimumResult {
                    binding: self.binding,
                    original_rows: copied(&canonical)?,
                    minimum_cardinality: slots,
                };
                let first = copied(&canonical)?;
                self.minimum = Some(slots);
                self.first = Some(first);
                self.last = Some(canonical);
                return Ok(Some(result));
            }
            self.statistics.negative_k_queries += 1;
        }
        Ok(None)
    }

    /// Enumerates every original-ID optimum in lexical order, including equal
    /// and dominated rows. Each page is checked against the same physical source;
    /// only an exhausted exact search returns None.
    pub fn next_alternative(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<Option<ImplicitMinimumResult>, ImplicitMinimumError> {
        self.check(control, 0)?;
        let minimum = self.minimum.ok_or(ImplicitMinimumError::NotSolved)?;
        if self.alternatives_exhausted {
            return Ok(None);
        }
        let previous = copied(self.last.as_ref().ok_or(ImplicitMinimumError::NotSolved)?)?;
        for pivot in (0..minimum).rev() {
            let prefix = copied(&previous[..pivot])?;
            let lower = previous[pivot]
                .checked_add(1)
                .ok_or(ImplicitMinimumError::CapacityExceeded)?;
            let bounds = self.bounds(&prefix, lower, minimum)?;
            self.statistics.canonical_queries += 1;
            if let Some(witness) = self.feasible(&bounds, control)? {
                let canonical =
                    self.canonical_completion(prefix, lower, witness, minimum, control)?;
                if canonical <= previous || canonical.len() != minimum {
                    return Err(ImplicitMinimumError::InvalidOracle);
                }
                let result = ImplicitMinimumResult {
                    binding: self.binding,
                    original_rows: copied(&canonical)?,
                    minimum_cardinality: minimum,
                };
                self.last = Some(canonical);
                return Ok(Some(result));
            }
        }
        self.alternatives_exhausted = true;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MatrixOracle {
        rows: Vec<u64>,
        patterns: usize,
        open: bool,
        changed: bool,
    }
    impl ImplicitDiagramOracle for MatrixOracle {
        fn binding(&self) -> ImplicitSourceBinding {
            ImplicitSourceBinding {
                query_sha256: [u8::from(self.changed); 32],
                original_identity_sha256: [2; 32],
                original_row_count: self.rows.len(),
                pattern_universe_count: self.patterns,
            }
        }
        fn checked_retained_bytes(&self) -> Option<u128> {
            Some((self.rows.capacity() * 8) as u128)
        }
        fn joint(
            &mut self,
            rows: Range<usize>,
            patterns: &[usize],
            budget: ImplicitOracleBudget,
            _: &ExecutionControl,
        ) -> ImplicitOracleObservation<ImplicitJointDecision> {
            let work = (rows.len() as u64) * patterns.len() as u64;
            if work > budget.remaining_work_steps {
                return ImplicitOracleObservation {
                    result: Err(ImplicitMinimumError::WorkLimit),
                    work_steps: 0,
                    peak_retained_bytes: self.checked_retained_bytes().unwrap(),
                };
            }
            let decision = match rows
                .into_iter()
                .find(|&row| patterns.iter().all(|&p| self.rows[row] & (1 << p) != 0))
            {
                Some(row) => ImplicitJointDecision::Witness(row),
                None if self.open => ImplicitJointDecision::Unknown,
                None => ImplicitJointDecision::ProvedEmpty,
            };
            ImplicitOracleObservation {
                result: Ok(decision),
                work_steps: work,
                peak_retained_bytes: self.checked_retained_bytes().unwrap(),
            }
        }
        fn counterexample(
            &mut self,
            rows: &[usize],
            budget: ImplicitOracleBudget,
            _: &ExecutionControl,
        ) -> ImplicitOracleObservation<Option<usize>> {
            let work = (rows.len() as u64) * self.patterns as u64;
            if work > budget.remaining_work_steps {
                return ImplicitOracleObservation {
                    result: Err(ImplicitMinimumError::WorkLimit),
                    work_steps: 0,
                    peak_retained_bytes: self.checked_retained_bytes().unwrap(),
                };
            }
            let decision = (0..self.patterns)
                .find(|&p| rows.iter().all(|&row| self.rows[row] & (1 << p) == 0));
            ImplicitOracleObservation {
                result: Ok(decision),
                work_steps: work,
                peak_retained_bytes: self.checked_retained_bytes().unwrap(),
            }
        }
    }

    fn search(rows: Vec<u64>, patterns: usize) -> ImplicitMinimumSearch<MatrixOracle> {
        ImplicitMinimumSearch::new(
            MatrixOracle {
                rows,
                patterns,
                open: false,
                changed: false,
            },
            ImplicitMinimumLimits {
                max_work_steps: 1_000_000,
                max_retained_bytes: 1 << 20,
            },
        )
        .unwrap()
    }

    fn reference(rows: &[u64], patterns: usize) -> Vec<Vec<usize>> {
        let required = (1 << patterns) - 1;
        let mut portfolios = Vec::new();
        for subset in 0_u64..1 << rows.len() {
            let indices: Vec<_> = (0..rows.len())
                .filter(|&row| subset & (1 << row) != 0)
                .collect();
            let covered = indices.iter().fold(0, |mask, &row| mask | rows[row]);
            if covered & required == required {
                portfolios.push(indices);
            }
        }
        let Some(minimum) = portfolios.iter().map(Vec::len).min() else {
            return Vec::new();
        };
        portfolios.retain(|rows| rows.len() == minimum);
        portfolios.sort();
        portfolios
    }

    #[test]
    fn counterexample_synthesis_and_every_original_optimum_match_all_small_matrices() {
        let control = ExecutionControl::default();
        for encoded in 0..512_u64 {
            let rows: Vec<_> = (0..3).map(|row| (encoded >> (row * 3)) & 7).collect();
            let expected = reference(&rows, 3);
            for (capacity, graph_capacity) in [0, 1, 64].into_iter().flat_map(|capacity| {
                [0, 1, 2, 3]
                    .into_iter()
                    .map(move |graph_capacity| (capacity, graph_capacity))
            }) {
                let mut engine = search(rows.clone(), 3);
                engine.set_joint_query_cache(capacity, 64 * 1024).unwrap();
                engine
                    .set_pairwise_conflict_ordering(graph_capacity)
                    .unwrap();
                let mut actual = Vec::new();
                if let Some(first) = engine.solve_minimum(&control).unwrap() {
                    actual.push(first.original_rows().to_vec());
                    while let Some(next) = engine.next_alternative(&control).unwrap() {
                        actual.push(next.original_rows().to_vec());
                    }
                }
                assert_eq!(
                    actual, expected,
                    "matrix {encoded}, memo capacity {capacity}, graph capacity {graph_capacity}"
                );
                if let Some(first) = expected.first() {
                    assert_eq!(
                        engine
                            .solve_minimum(&control)
                            .unwrap()
                            .unwrap()
                            .original_rows(),
                        first
                    );
                }
                assert!(engine.joint_cache_retained_bytes().unwrap() <= 64 * 1024);
            }
        }
    }

    #[test]
    fn common_diagram_quantifier_and_duplicate_dominated_ids_are_preserved() {
        let control = ExecutionControl::default();
        for rows in [
            vec![3, 5, 6],
            vec![1, 3, 3, 6, 6],
            vec![0, 7, 7],
            vec![1, 2, 4],
        ] {
            let expected = reference(&rows, 3);
            let mut engine = search(rows, 3);
            let first = engine.solve_minimum(&control).unwrap().unwrap();
            let mut actual = vec![first.original_rows().to_vec()];
            while let Some(next) = engine.next_alternative(&control).unwrap() {
                actual.push(next.original_rows().to_vec());
            }
            assert_eq!(actual, expected);
        }
        let mut empty = search(Vec::new(), 0);
        assert!(empty
            .solve_minimum(&control)
            .unwrap()
            .unwrap()
            .original_rows()
            .is_empty());
        assert!(empty.next_alternative(&control).unwrap().is_none());
    }

    #[test]
    fn unfinished_queries_limits_cancellation_and_source_changes_never_mint_proof() {
        let control = ExecutionControl::default();
        let mut open = search(vec![1], 2);
        open.set_joint_query_cache(32, 16 * 1024).unwrap();
        open.set_pairwise_conflict_ordering(2).unwrap();
        open.oracle.open = true;
        assert_eq!(
            open.solve_minimum(&control),
            Err(ImplicitMinimumError::Unknown)
        );
        assert!(open.minimum.is_none());
        assert!(open
            .pair_conflicts
            .as_ref()
            .unwrap()
            .words
            .iter()
            .all(|&word| word == 0));
        assert!(open
            .joint_cache
            .as_ref()
            .unwrap()
            .entries
            .values()
            .all(|answer| matches!(answer, ImplicitJointDecision::Witness(_))));
        let mut limited = search(vec![7], 3);
        limited.limits.max_work_steps = 1;
        assert_eq!(
            limited.solve_minimum(&control),
            Err(ImplicitMinimumError::WorkLimit)
        );
        let mut memory = search(vec![7], 3);
        memory.limits.max_retained_bytes = 0;
        assert_eq!(
            memory.solve_minimum(&control),
            Err(ImplicitMinimumError::CapacityExceeded)
        );
        let mut changed = search(vec![7], 3);
        changed.set_joint_query_cache(32, 16 * 1024).unwrap();
        changed.set_pairwise_conflict_ordering(3).unwrap();
        assert!(changed.solve_minimum(&control).unwrap().is_some());
        changed.oracle.changed = true;
        assert_eq!(
            changed.solve_minimum(&control),
            Err(ImplicitMinimumError::SourceChanged)
        );
        let mut failed_call = search(vec![7], 3);
        let before = failed_call.work_steps;
        let budget = failed_call.oracle_budget(0).unwrap();
        assert_eq!(
            failed_call.accept_observation::<()>(
                ImplicitOracleObservation {
                    result: Err(ImplicitMinimumError::Unknown),
                    work_steps: 7,
                    peak_retained_bytes: 256,
                },
                budget
            ),
            Err(ImplicitMinimumError::Unknown)
        );
        assert_eq!(failed_call.work_steps, before + 7);
        assert_eq!(failed_call.statistics().oracle_work_steps, 7);
        assert!(failed_call.minimum.is_none());
        let cancelled = ExecutionControl::default();
        cancelled.cancellation.handle().cancel();
        assert_eq!(
            search(vec![7], 3).solve_minimum(&cancelled),
            Err(ImplicitMinimumError::Cancelled)
        );
    }
}
