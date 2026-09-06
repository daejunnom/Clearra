// SRP rationale: this module has one change reason: enumerate every original-
// row minimum-cover portfolio in numeric lexicographic order through one
// bounded paging, cancellation, and restart contract.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::pattern::pattern_bitset::PatternBitSet;

use super::exact_at_most_parallel::{
    ExactAtMostCoordinator, ExactAtMostParallelDecision, ExactAtMostParallelError,
    ExactAtMostQuery, ExactAtMostQueryIdentity, ExactAtMostReceipt, ExactAtMostTask,
};
use super::exact_minimum_cover::{
    ExactCoverSearchSession, ExactMinimumCoverError, ExactMinimumCoverResult,
    ExactMinimumCoverSession, ExactMinimumCoverSessionAdvance,
    WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET,
};

fn parallel_error(error: ExactAtMostParallelError) -> ExactMinimumCoverPortfolioError {
    match error {
        ExactAtMostParallelError::Exact(error) => {
            ExactMinimumCoverPortfolioError::MinimumCover(error)
        }
        _ => ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof,
    }
}

struct ParallelConfiguration {
    partitions: usize,
    matrix_id: [u8; 32],
    next_query_id: AtomicU64,
}

impl Clone for ParallelConfiguration {
    fn clone(&self) -> Self {
        Self {
            partitions: self.partitions,
            matrix_id: self.matrix_id,
            // A cloned frontier owns its own counter snapshot, just as the
            // previous Cell clone did; no cross-frontier mutable counter.
            next_query_id: AtomicU64::new(self.next_query_id.load(Ordering::Relaxed)),
        }
    }
}

#[derive(Clone)]
struct ParallelOracle {
    coordinator: ExactAtMostCoordinator,
    next_task: usize,
    warm_session: Option<ExactCoverSearchSession>,
    warm_remaining_steps: u64,
}

enum ParallelWarmAdvance {
    Pending(u64),
    Cancelled(u64),
}

impl ParallelOracle {
    fn published_query(&self) -> Option<&ExactAtMostQuery> {
        (self.warm_session.is_none()
            && (self.next_task != 0
                || matches!(
                    self.coordinator.decision(),
                    ExactAtMostParallelDecision::Pending { .. }
                )))
        .then(|| self.coordinator.query())
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        self.coordinator
            .checked_retained_bytes()?
            .checked_add(self.warm_session.as_ref().map_or(
                Some(0),
                ExactCoverSearchSession::checked_retained_capacity_bytes,
            )?)
    }

    fn advance_warm(
        &mut self,
        max_nodes: u64,
        external_live: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<Option<ParallelWarmAdvance>, ExactMinimumCoverPortfolioError> {
        let Some(session) = self.warm_session.as_mut() else {
            return Ok(None);
        };
        let base = external_live
            .checked_add(
                self.coordinator
                    .checked_retained_bytes()
                    .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?,
            )
            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?;
        memory_guard(
            base.checked_add(
                session
                    .checked_retained_capacity_bytes()
                    .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?,
            )
            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?,
        )
        .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        let advance = session
            .advance(
                max_nodes.min(self.warm_remaining_steps),
                &mut |owned| {
                    memory_guard(
                        base.checked_add(owned)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    )
                },
                cancelled,
            )
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        match advance {
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes } => {
                self.warm_remaining_steps = self.warm_remaining_steps.saturating_sub(visited_nodes);
                if self.warm_remaining_steps == 0 || session.witness_shortcut_exhausted() {
                    #[cfg(all(feature = "diagnostic-probes", not(target_arch = "wasm32")))]
                    eprintln!("{{\"phase\":\"global_warm_terminal\",\"query\":{},\"limit\":{},\"outcome\":\"miss\",\"work\":{}}}",
                        self.coordinator.query().identity().query_id, self.coordinator.query().limit(),
                        WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET as u64 - self.warm_remaining_steps);
                    self.warm_session = None;
                }
                Ok(Some(ParallelWarmAdvance::Pending(visited_nodes)))
            }
            ExactMinimumCoverSessionAdvance::Found {
                result,
                visited_nodes,
            } => {
                let warm_retained = session
                    .checked_retained_capacity_bytes()
                    .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?;
                #[cfg(all(feature = "diagnostic-probes", not(target_arch = "wasm32")))]
                eprintln!("{{\"phase\":\"global_warm_terminal\",\"query\":{},\"limit\":{},\"outcome\":\"found\",\"work\":{}}}",
                    self.coordinator.query().identity().query_id, self.coordinator.query().limit(),
                    WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET as u64 - self.warm_remaining_steps + visited_nodes);
                self.coordinator
                    .accept_warm_witness(result.into_parts().0, &mut |owned| {
                        memory_guard(
                            external_live
                                .checked_add(warm_retained)
                                .and_then(|base| base.checked_add(owned))
                                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                        )
                    })
                    .map_err(parallel_error)?;
                self.warm_session = None;
                Ok(Some(ParallelWarmAdvance::Pending(visited_nodes)))
            }
            // No negative answer from the advisory prepass is accepted as
            // exhaustive cube authority. Cancellation stays with its caller.
            ExactMinimumCoverSessionAdvance::ProvedNone { visited_nodes } => {
                self.warm_session = None;
                Ok(Some(ParallelWarmAdvance::Pending(visited_nodes)))
            }
            ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes } => {
                self.warm_session = None;
                Ok(Some(ParallelWarmAdvance::Cancelled(visited_nodes)))
            }
            ExactMinimumCoverSessionAdvance::Finished => {
                Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)
            }
        }
    }

    fn waiting_for_issued(&self) -> bool {
        !self.coordinator.issued_prefix_complete(self.next_task)
    }

    fn take_task(&mut self) -> Option<ExactAtMostTask> {
        if self.warm_session.is_some()
            || !matches!(
                self.coordinator.decision(),
                ExactAtMostParallelDecision::Pending { .. }
            )
        {
            return None;
        }
        while self.next_task < self.coordinator.tasks().len() {
            if self.coordinator.retire_redundant_unissued(self.next_task) {
                self.next_task += 1;
                continue;
            }
            let task = self.coordinator.tasks().get(self.next_task)?.clone();
            self.next_task += 1;
            return Some(task);
        }
        None
    }

    fn prepare_idle_assist(
        &mut self,
        maximum_children: usize,
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<bool, ExactMinimumCoverPortfolioError> {
        if self.warm_session.is_some() {
            return Ok(false);
        }
        self.coordinator
            .prepare_idle_assist(self.next_task, maximum_children, guard, &mut || false)
            .map_err(parallel_error)
    }
}

struct ParallelMinimumProof {
    oracle: ParallelOracle,
    incumbent: Vec<usize>,
}

/// One exact minimum-cardinality cover expressed in original input-row
/// identities. The row indices are strictly increasing, which also defines the
/// canonical order between portfolios.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactMinimumCoverPortfolio {
    row_indices: Vec<usize>,
}

impl ExactMinimumCoverPortfolio {
    pub fn row_indices(&self) -> &[usize] {
        &self.row_indices
    }

    pub fn into_row_indices(self) -> Vec<usize> {
        self.row_indices
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.row_indices.capacity() as u128).checked_mul(core::mem::size_of::<usize>() as u128)
    }
}

/// Why a lazy enumeration call returned before sealing the semantic result
/// set. `PageFull` is a presentation boundary; the other two reasons are
/// explicit incomplete outcomes that callers must not describe as "all".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactMinimumCoverEnumerationStop {
    PageFull,
    WorkBudgetExhausted,
    Cancelled,
    Sealed,
}

/// Opaque restart checkpoint for the immutable `(required, rows)` input that
/// created it. A persistence layer must bind this value to its own query,
/// candidate-map, build, and integrity identities before accepting it from an
/// untrusted source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactMinimumCoverRestart {
    input: Arc<ExactMinimumCoverPortfolioInput>,
    optimal_cardinality: usize,
    next_combination: Option<Vec<usize>>,
    known_alternative_count: DecimalCounter,
    enumeration_complete: bool,
}

impl ExactMinimumCoverRestart {
    pub fn row_count(&self) -> usize {
        self.input.row_words.len()
    }

    pub const fn optimal_cardinality(&self) -> usize {
        self.optimal_cardinality
    }

    pub fn next_combination(&self) -> Option<&[usize]> {
        self.next_combination.as_deref()
    }

    pub fn known_alternative_count_decimal(&self) -> String {
        self.known_alternative_count.to_decimal_string()
    }

    pub const fn enumeration_complete(&self) -> bool {
        self.enumeration_complete
    }

    /// Exact heap payload retained by this restart owner. This includes the
    /// immutable restart input shared by clones, the current combination, and
    /// the arbitrary-precision decimal counter. Callers that share a cloned
    /// enumerator must charge the immutable input graph only once.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = checked_input_retained_capacity_bytes(&self.input)?;
        if let Some(combination) = &self.next_combination {
            bytes = bytes.checked_add(
                (combination.capacity() as u128)
                    .checked_mul(core::mem::size_of::<usize>() as u128)?,
            )?;
        }
        bytes = bytes.checked_add(self.known_alternative_count.digits.capacity() as u128)?;
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactMinimumCoverPortfolioPage {
    portfolios: Vec<ExactMinimumCoverPortfolio>,
    optimal_cardinality: usize,
    known_alternative_count_decimal: String,
    total_alternative_count_decimal: Option<String>,
    enumeration_complete: bool,
    stop: ExactMinimumCoverEnumerationStop,
    work_steps: u64,
    solver_cursor_work_steps: u64,
    candidate_combinations_tested: u64,
    impossible_prefix_subtrees_pruned: u64,
    restart: Option<ExactMinimumCoverRestart>,
}

impl ExactMinimumCoverPortfolioPage {
    pub fn portfolios(&self) -> &[ExactMinimumCoverPortfolio] {
        &self.portfolios
    }

    pub fn into_portfolios(self) -> Vec<ExactMinimumCoverPortfolio> {
        self.portfolios
    }

    pub const fn optimal_cardinality(&self) -> usize {
        self.optimal_cardinality
    }

    pub fn known_alternative_count_decimal(&self) -> &str {
        &self.known_alternative_count_decimal
    }

    pub fn total_alternative_count_decimal(&self) -> Option<&str> {
        self.total_alternative_count_decimal.as_deref()
    }

    pub const fn enumeration_complete(&self) -> bool {
        self.enumeration_complete
    }

    pub const fn stop(&self) -> ExactMinimumCoverEnumerationStop {
        self.stop
    }

    /// Work units consumed during this call. Every unit is classified exactly
    /// once as solver-cursor work, one emitted candidate, or one pruned suffix.
    /// A cheap replay that completes a semantic frontier therefore consumes
    /// the candidate/prune unit even when it enters no DFS node. The value
    /// never exceeds the caller's `max_work_steps` budget.
    pub const fn work_steps(&self) -> u64 {
        self.work_steps
    }

    /// Preparation, optional-heuristic, and exact DFS cursor work that did
    /// not itself complete a semantic frontier decision. Together with the
    /// candidate and prune counters this partitions every reported work step.
    pub const fn solver_cursor_work_steps(&self) -> u64 {
        self.solver_cursor_work_steps
    }

    /// Completed semantic frontier decisions that emitted one replay-checked
    /// `k*` combination during this call. Internal AtMost queries are not
    /// persistence identities and are deliberately not exposed here.
    pub const fn candidate_combinations_tested(&self) -> u64 {
        self.candidate_combinations_tested
    }

    /// Completed semantic frontier decisions that proved the entire remaining
    /// lex suffix empty. One such proof can skip many combinations and many
    /// internal selector ranges. This diagnostic is deliberately absent from
    /// the persistence contract.
    pub const fn impossible_prefix_subtrees_pruned(&self) -> u64 {
        self.impossible_prefix_subtrees_pruned
    }

    pub fn restart(&self) -> Option<&ExactMinimumCoverRestart> {
        self.restart.as_ref()
    }

    /// Conservative heap payload retained by this page owner. When the page
    /// and its originating enumerator coexist, the restart's immutable input
    /// is shared but deliberately counted again; callers may use this value as
    /// a safe standalone admission upper bound without inspecting private Arc
    /// identity.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.portfolios.capacity() as u128)
            .checked_mul(core::mem::size_of::<ExactMinimumCoverPortfolio>() as u128)?;
        for portfolio in &self.portfolios {
            bytes = bytes.checked_add(portfolio.checked_retained_capacity_bytes()?)?;
        }
        bytes = bytes.checked_add(self.known_alternative_count_decimal.capacity() as u128)?;
        if let Some(total) = &self.total_alternative_count_decimal {
            bytes = bytes.checked_add(total.capacity() as u128)?;
        }
        if let Some(restart) = &self.restart {
            bytes = bytes.checked_add(restart.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExactMinimumCoverPortfolioError {
    MinimumCover(ExactMinimumCoverError),
    InvalidMinimumCoverProof,
    RequiredPatternsNotCoverable {
        covered_pattern_count: u32,
        required_pattern_count: u32,
    },
    PageSizeMustBePositive,
    InvalidRestart,
    AllocationFailed {
        component: &'static str,
    },
}

/// A two-pass exact minimum-cover authority.
///
/// Construction first delegates to the existing branch-and-bound solver to
/// prove the minimum cardinality `k*`. Enumeration then self-reduces exact
/// bounded-cover decisions over the original row identities, not the solver's
/// dominated-row reduction. A synthetic query-local selector batches an
/// interval of possible next numeric IDs; an interval is skipped only after a
/// complete `k*`-bounded infeasibility proof. Consequently output is strict
/// numeric lexicographic order, equal and dominated rows remain observable,
/// and there is no semantic top-K cutoff.
#[derive(Clone)]
pub struct ExactMinimumCoverPortfolioEnumerator {
    input: Arc<ExactMinimumCoverPortfolioInput>,
    optimal_cardinality: usize,
    next_combination: Option<Vec<usize>>,
    known_alternative_count: DecimalCounter,
    enumeration_complete: bool,
    /// Non-authoritative acceleration hint. It is never serialized, never
    /// compared as restart identity, and every use is replay-validated.
    witness_hint: Option<Vec<usize>>,
    /// In-flight original-row lexicographic self-reduction. Unlike the opaque
    /// restart projection, this trusted in-memory continuation retains the
    /// exact AtMost DFS stack, memo table, and incumbent across bounded calls.
    /// It is never serialized; an external restart safely replays from the
    /// inclusive `next_combination` frontier instead.
    pending_search: Option<PendingLexSearch>,
    parallel: Option<ParallelConfiguration>,
}

impl core::fmt::Debug for ExactMinimumCoverPortfolioEnumerator {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ExactMinimumCoverPortfolioEnumerator")
            .field("input", &self.input)
            .field("optimal_cardinality", &self.optimal_cardinality)
            .field("next_combination", &self.next_combination)
            .field("known_alternative_count", &self.known_alternative_count)
            .field("enumeration_complete", &self.enumeration_complete)
            .field("witness_hint", &self.witness_hint)
            .field("has_pending_search", &self.pending_search.is_some())
            .finish()
    }
}

/// One exact proof pass prepared for public canonical selection. Complete
/// inputs carry both the proof evidence and an original-row enumerator;
/// incomplete inputs preserve the exact solver's partial coverage evidence
/// without manufacturing an all-portfolio authority.
#[derive(Clone, Debug)]
pub enum ExactMinimumCoverPortfolioPreparation {
    Coverable {
        proof: ExactMinimumCoverResult,
        enumerator: ExactMinimumCoverPortfolioEnumerator,
    },
    Incomplete {
        proof: ExactMinimumCoverResult,
    },
}

/// One bounded advance of the proof-bound portfolio preparation authority.
#[derive(Debug)]
pub enum ExactMinimumCoverPortfolioPreparationAdvance {
    Pending {
        visited_nodes: u64,
    },
    Coverable {
        proof: ExactMinimumCoverResult,
        enumerator: ExactMinimumCoverPortfolioEnumerator,
        visited_nodes: u64,
    },
    Incomplete {
        proof: ExactMinimumCoverResult,
        visited_nodes: u64,
    },
    Cancelled {
        visited_nodes: u64,
    },
    Finished,
}

/// Owns both the immutable original-row matrix and its resumable minimum-cover
/// proof. Only this owner can turn the terminal proof into an enumerator, which
/// prevents a proof from another matrix from being injected as `k*` authority.
pub struct ExactMinimumCoverPortfolioPreparationSession {
    state: ExactMinimumCoverPortfolioPreparationSessionState,
    parallel: Option<ParallelConfiguration>,
    parallel_proof: Option<ParallelMinimumProof>,
}

enum ExactMinimumCoverPortfolioPreparationSessionState {
    Proving {
        required: PatternBitSet,
        rows: Vec<PatternBitSet>,
        proof_session: ExactMinimumCoverSession,
    },
    Finished,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExactMinimumCoverPortfolioInput {
    pattern_count: usize,
    row_words: Vec<Vec<u64>>,
    target_words: Vec<u64>,
}

fn checked_input_retained_capacity_bytes(input: &ExactMinimumCoverPortfolioInput) -> Option<u128> {
    let mut bytes = (core::mem::size_of::<ExactMinimumCoverPortfolioInput>() as u128)
        .checked_add((2 * core::mem::size_of::<usize>()) as u128)?
        .checked_add(
            (input.row_words.capacity() as u128)
                .checked_mul(core::mem::size_of::<Vec<u64>>() as u128)?,
        )?;
    for row in &input.row_words {
        bytes = bytes.checked_add(
            (row.capacity() as u128).checked_mul(core::mem::size_of::<u64>() as u128)?,
        )?;
    }
    bytes.checked_add(
        (input.target_words.capacity() as u128).checked_mul(core::mem::size_of::<u64>() as u128)?,
    )
}

impl ExactMinimumCoverPortfolioPreparationSession {
    /// Complete heap payload of the retained input, serial proof owner and
    /// optional parallel frontier. Shared immutable matrices are deliberately
    /// counted per owner for conservative external admission.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = match &self.state {
            ExactMinimumCoverPortfolioPreparationSessionState::Proving {
                required,
                rows,
                proof_session,
            } => checked_preparation_input_retained_capacity_bytes(required, rows)
                .ok()?
                .checked_add(proof_session.checked_retained_capacity_bytes()?)?,
            ExactMinimumCoverPortfolioPreparationSessionState::Finished => 0,
        };
        if let Some(proof) = &self.parallel_proof {
            bytes = bytes
                .checked_add(proof.oracle.checked_retained_bytes()?)?
                .checked_add(
                    (proof.incumbent.capacity() as u128)
                        .checked_mul(core::mem::size_of::<usize>() as u128)?,
                )?;
        }
        Some(bytes)
    }
    pub fn enable_parallel(
        &mut self,
        partitions: usize,
        matrix_id: [u8; 32],
    ) -> Result<(), ExactMinimumCoverPortfolioError> {
        enable_configuration(&mut self.parallel, partitions, matrix_id)
    }

    pub fn parallel_query(&self) -> Option<&ExactAtMostQuery> {
        self.parallel_proof.as_ref()?.oracle.published_query()
    }

    pub fn parallel_query_satisfied(&self) -> bool {
        self.parallel_proof.as_ref().is_some_and(|proof| {
            matches!(
                proof.oracle.coordinator.decision(),
                ExactAtMostParallelDecision::Found(_)
            )
        })
    }

    pub fn take_parallel_task(&mut self) -> Option<ExactAtMostTask> {
        self.parallel_proof.as_mut()?.oracle.take_task()
    }

    pub fn prepare_parallel_idle_assist(
        &mut self,
        maximum_children: usize,
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<bool, ExactMinimumCoverPortfolioError> {
        match &mut self.parallel_proof {
            Some(proof) => proof.oracle.prepare_idle_assist(maximum_children, guard),
            None => Ok(false),
        }
    }

    pub fn parallel_task_is_redundant(
        &self,
        identity: ExactAtMostQueryIdentity,
        partition_id: u64,
    ) -> Result<bool, ExactMinimumCoverPortfolioError> {
        self.parallel_proof
            .as_ref()
            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?
            .oracle
            .coordinator
            .task_is_redundant(identity, partition_id)
            .map_err(parallel_error)
    }

    pub fn accept_parallel_receipt(
        &mut self,
        receipt: ExactAtMostReceipt,
    ) -> Result<(), ExactMinimumCoverPortfolioError> {
        self.parallel_proof
            .as_mut()
            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?
            .oracle
            .coordinator
            .accept(receipt)
            .map_err(parallel_error)?;
        Ok(())
    }

    fn advance_parallel(
        &mut self,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<Option<ExactMinimumCoverPortfolioPreparationAdvance>, ExactMinimumCoverPortfolioError>
    {
        if cancelled() {
            self.parallel_proof = None;
            self.state = ExactMinimumCoverPortfolioPreparationSessionState::Finished;
            return Ok(Some(
                ExactMinimumCoverPortfolioPreparationAdvance::Cancelled { visited_nodes: 0 },
            ));
        }
        let ExactMinimumCoverPortfolioPreparationSessionState::Proving {
            required,
            rows,
            proof_session,
        } = &self.state
        else {
            return Ok(None);
        };
        if required.is_empty() {
            return Ok(None);
        }
        let owned_base = checked_preparation_input_retained_capacity_bytes(required, rows)?
            .checked_add(
                proof_session
                    .checked_retained_capacity_bytes()
                    .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?,
            )
            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?;
        let memory_guard = &mut |owned: u128| {
            memory_guard(
                owned_base
                    .checked_add(owned)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            )
        };
        let config = self
            .parallel
            .as_ref()
            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?;
        if self.parallel_proof.is_none() {
            // A deterministic greedy cover is only a replayable upper bound,
            // never minimum authority. Every improvement/negative is exact.
            let base = checked_preparation_input_retained_capacity_bytes(required, rows)?;
            let mut covered = try_vec_with_memory_guard(
                required.word_count(),
                base,
                memory_guard,
                "parallel minimum greedy coverage",
            )?;
            covered.resize(required.word_count(), 0_u64);
            let mut incumbent = try_vec_with_memory_guard(
                rows.len(),
                base,
                memory_guard,
                "parallel minimum greedy incumbent",
            )?;
            loop {
                if cancelled() {
                    return Ok(Some(
                        ExactMinimumCoverPortfolioPreparationAdvance::Cancelled {
                            visited_nodes: 0,
                        },
                    ));
                }
                let mut best = None;
                for (index, row) in rows.iter().enumerate() {
                    let gain: u32 = covered
                        .iter()
                        .enumerate()
                        .map(|(word, covered)| {
                            (row.word_at(word) & required.word_at(word) & !covered).count_ones()
                        })
                        .sum();
                    if gain > best.map_or(0, |(_, gain)| gain) {
                        best = Some((index, gain));
                    }
                }
                let Some((selected, _)) = best else {
                    break;
                };
                incumbent.push(selected);
                for (word, covered) in covered.iter_mut().enumerate() {
                    *covered |= rows[selected].word_at(word);
                }
            }
            if covered
                .iter()
                .enumerate()
                .any(|(word, covered)| covered & required.word_at(word) != required.word_at(word))
            {
                // Preserve the serial authority's incomplete-coverage contract.
                self.parallel = None;
                return Ok(None);
            }
            incumbent.sort_unstable();
            let oracle = make_parallel_oracle(
                config,
                required.clone(),
                rows.clone(),
                incumbent.len() - 1,
                Some(incumbent.clone()),
                memory_guard,
                cancelled,
            )?;
            self.parallel_proof = Some(ParallelMinimumProof { oracle, incumbent });
            return Ok(Some(
                ExactMinimumCoverPortfolioPreparationAdvance::Pending { visited_nodes: 1 },
            ));
        }
        let mut parallel = self
            .parallel_proof
            .take()
            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?;
        if let Some(warm) = parallel
            .oracle
            .advance_warm(1, 0, memory_guard, cancelled)?
        {
            let visited_nodes = match warm {
                ParallelWarmAdvance::Pending(nodes) => nodes,
                ParallelWarmAdvance::Cancelled(visited_nodes) => {
                    self.state = ExactMinimumCoverPortfolioPreparationSessionState::Finished;
                    return Ok(Some(
                        ExactMinimumCoverPortfolioPreparationAdvance::Cancelled { visited_nodes },
                    ));
                }
            };
            self.parallel_proof = Some(parallel);
            return Ok(Some(
                ExactMinimumCoverPortfolioPreparationAdvance::Pending { visited_nodes },
            ));
        }
        if parallel.oracle.waiting_for_issued() {
            self.parallel_proof = Some(parallel);
            return Ok(Some(
                ExactMinimumCoverPortfolioPreparationAdvance::Pending { visited_nodes: 0 },
            ));
        }
        match parallel.oracle.coordinator.decision() {
            ExactAtMostParallelDecision::Pending { .. } => {
                self.parallel_proof = Some(parallel);
                Ok(Some(
                    ExactMinimumCoverPortfolioPreparationAdvance::Pending { visited_nodes: 0 },
                ))
            }
            ExactAtMostParallelDecision::Found(found) => {
                parallel.incumbent = found.clone();
                let previous_owned = parallel
                    .oracle
                    .checked_retained_bytes()
                    .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?;
                parallel.oracle = make_parallel_oracle(
                    config,
                    required.clone(),
                    rows.clone(),
                    parallel.incumbent.len() - 1,
                    Some(parallel.incumbent.clone()),
                    &mut |owned| {
                        memory_guard(
                            previous_owned
                                .checked_add(owned)
                                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                        )
                    },
                    cancelled,
                )?;
                self.parallel_proof = Some(parallel);
                Ok(Some(
                    ExactMinimumCoverPortfolioPreparationAdvance::Pending { visited_nodes: 1 },
                ))
            }
            ExactAtMostParallelDecision::ProvedNone => {
                let proof = ExactMinimumCoverResult::from_parallel_negative(
                    &parallel.oracle.coordinator,
                    parallel.incumbent,
                )
                .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?;
                let enumerator =
                    ExactMinimumCoverPortfolioEnumerator::from_proof_with_memory_guard(
                        required,
                        rows,
                        &proof,
                        memory_guard,
                    )?;
                self.state = ExactMinimumCoverPortfolioPreparationSessionState::Finished;
                Ok(Some(
                    ExactMinimumCoverPortfolioPreparationAdvance::Coverable {
                        proof,
                        enumerator,
                        visited_nodes: 1,
                    },
                ))
            }
            ExactAtMostParallelDecision::Cancelled => {
                self.state = ExactMinimumCoverPortfolioPreparationSessionState::Finished;
                Ok(Some(
                    ExactMinimumCoverPortfolioPreparationAdvance::Cancelled { visited_nodes: 0 },
                ))
            }
        }
    }

    pub fn new(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        Self::new_with_memory_guard(required, rows, &mut |_| Ok(()))
    }

    pub fn new_with_memory_guard(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let required = required.clone();
        let required_live = required.checked_storage_retained_bytes().ok_or(
            ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            ),
        )?;
        memory_guard(required_live).map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        let mut owned_rows = try_vec_with_memory_guard(
            rows.len(),
            required_live,
            memory_guard,
            "exact_minimum_cover_preparation_rows",
        )?;
        for row in rows {
            owned_rows.push(row.clone());
        }
        let input_live = checked_preparation_input_retained_capacity_bytes(&required, &owned_rows)?;
        memory_guard(input_live).map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        let proof_session = ExactMinimumCoverSession::new_with_memory_guard(
            &required,
            &owned_rows,
            &mut |solver_owned| {
                memory_guard(
                    input_live
                        .checked_add(solver_owned)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                )
            },
        )
        .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        Ok(Self {
            parallel: None,
            parallel_proof: None,
            state: ExactMinimumCoverPortfolioPreparationSessionState::Proving {
                required,
                rows: owned_rows,
                proof_session,
            },
        })
    }

    pub fn advance(
        &mut self,
        max_nodes: u64,
    ) -> Result<ExactMinimumCoverPortfolioPreparationAdvance, ExactMinimumCoverPortfolioError> {
        self.advance_with_memory_guard_and_control(max_nodes, &mut |_| Ok(()), &mut || false)
    }

    pub fn advance_with_memory_guard_and_control(
        &mut self,
        max_nodes: u64,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<ExactMinimumCoverPortfolioPreparationAdvance, ExactMinimumCoverPortfolioError> {
        if matches!(
            self.state,
            ExactMinimumCoverPortfolioPreparationSessionState::Finished
        ) {
            return Ok(ExactMinimumCoverPortfolioPreparationAdvance::Finished);
        }
        if max_nodes == 0 {
            return Ok(ExactMinimumCoverPortfolioPreparationAdvance::Pending { visited_nodes: 0 });
        }
        if self.parallel.is_some() {
            if let Some(advance) = self.advance_parallel(memory_guard, cancelled)? {
                return Ok(advance);
            }
        }
        let state = core::mem::replace(
            &mut self.state,
            ExactMinimumCoverPortfolioPreparationSessionState::Finished,
        );
        let ExactMinimumCoverPortfolioPreparationSessionState::Proving {
            required,
            rows,
            mut proof_session,
        } = state
        else {
            return Ok(ExactMinimumCoverPortfolioPreparationAdvance::Finished);
        };
        let input_live = checked_preparation_input_retained_capacity_bytes(&required, &rows)?;
        let advance = proof_session
            .advance_with_memory_guard_and_control(
                max_nodes,
                &mut |solver_owned| {
                    memory_guard(
                        input_live
                            .checked_add(solver_owned)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    )
                },
                cancelled,
            )
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        match advance {
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes } => {
                self.state = ExactMinimumCoverPortfolioPreparationSessionState::Proving {
                    required,
                    rows,
                    proof_session,
                };
                Ok(ExactMinimumCoverPortfolioPreparationAdvance::Pending { visited_nodes })
            }
            ExactMinimumCoverSessionAdvance::Found {
                result: proof,
                visited_nodes,
            } => {
                if !proof.complete() {
                    return Ok(ExactMinimumCoverPortfolioPreparationAdvance::Incomplete {
                        proof,
                        visited_nodes,
                    });
                }
                let enumerator =
                    ExactMinimumCoverPortfolioEnumerator::from_proof_with_memory_guard(
                        &required,
                        &rows,
                        &proof,
                        &mut |construction_live| {
                            memory_guard(
                                input_live
                                    .checked_add(construction_live)
                                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                            )
                        },
                    )?;
                Ok(ExactMinimumCoverPortfolioPreparationAdvance::Coverable {
                    proof,
                    enumerator,
                    visited_nodes,
                })
            }
            ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes } => {
                Ok(ExactMinimumCoverPortfolioPreparationAdvance::Cancelled { visited_nodes })
            }
            ExactMinimumCoverSessionAdvance::ProvedNone { .. }
            | ExactMinimumCoverSessionAdvance::Finished => {
                Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)
            }
        }
    }
}

fn enable_configuration(
    config: &mut Option<ParallelConfiguration>,
    partitions: usize,
    matrix_id: [u8; 32],
) -> Result<(), ExactMinimumCoverPortfolioError> {
    if partitions < 2 {
        return Ok(());
    }
    if let Some(current) = config {
        if current.matrix_id != matrix_id || current.partitions != partitions {
            return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
        }
    } else {
        *config = Some(ParallelConfiguration {
            partitions,
            matrix_id,
            next_query_id: AtomicU64::new(1),
        });
    }
    Ok(())
}

#[cfg(test)]
mod parallel_portfolio_tests {
    use super::super::exact_at_most_parallel::{ExactAtMostShardAdvance, ExactAtMostShardSession};
    use super::*;

    fn run(query: &ExactAtMostQuery, task: ExactAtMostTask) -> ExactAtMostReceipt {
        let mut worker =
            ExactAtMostShardSession::prepare(query.clone(), task, &mut |_| Ok(()), &mut || false)
                .unwrap();
        for _ in 0..10000 {
            if let ExactAtMostShardAdvance::Terminal(receipt) =
                worker.advance(32, &mut |_| Ok(()), &mut || false).unwrap()
            {
                return receipt;
            }
        }
        panic!("tiny parallel oracle exceeded its test-only fixture bound");
    }

    #[test]
    fn parallel_query_counter_is_sync_and_clones_an_independent_snapshot() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ExactMinimumCoverPortfolioEnumerator>();
        assert_send_sync::<ExactMinimumCoverPortfolioPreparationSession>();
        let config = ParallelConfiguration {
            partitions: 2,
            matrix_id: [7; 32],
            next_query_id: AtomicU64::new(9),
        };
        let cloned = config.clone();
        let oracle = make_parallel_oracle(
            &config,
            PatternBitSet::all(1),
            vec![PatternBitSet::all(1)],
            1,
            None,
            &mut |_| Ok(()),
            &mut || false,
        )
        .unwrap();
        assert_eq!(oracle.coordinator.query().identity().query_id, 9);
        assert_eq!(config.next_query_id.load(Ordering::Relaxed), 10);
        assert_eq!(cloned.next_query_id.load(Ordering::Relaxed), 9);
        config.next_query_id.store(u64::MAX, Ordering::Relaxed);
        assert!(matches!(
            make_parallel_oracle(
                &config,
                PatternBitSet::all(1),
                vec![PatternBitSet::all(1)],
                1,
                None,
                &mut |_| Ok(()),
                &mut || false,
            ),
            Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)
        ));
        assert_eq!(config.next_query_id.load(Ordering::Relaxed), u64::MAX);
    }

    #[test]
    fn preparation_retained_heap_accounts_input_proof_and_parallel_frontier() {
        let required = PatternBitSet::all(3);
        let rows: Vec<_> = [3, 5, 6]
            .into_iter()
            .map(|mask| PatternBitSet::from_words(3, vec![mask]).unwrap())
            .collect();
        let mut preparation =
            ExactMinimumCoverPortfolioPreparationSession::new(&required, &rows).unwrap();
        let initial = preparation.checked_retained_capacity_bytes().unwrap();
        let ExactMinimumCoverPortfolioPreparationSessionState::Proving {
            required,
            rows,
            proof_session,
        } = &preparation.state
        else {
            panic!("new preparation must retain proof input")
        };
        assert_eq!(
            initial,
            checked_preparation_input_retained_capacity_bytes(required, rows).unwrap()
                + proof_session.checked_retained_capacity_bytes().unwrap()
        );
        preparation.enable_parallel(4, [8; 32]).unwrap();
        assert!(matches!(
            preparation.advance(1).unwrap(),
            ExactMinimumCoverPortfolioPreparationAdvance::Pending { .. }
        ));
        let parallel = preparation
            .parallel_proof
            .as_ref()
            .expect("parallel proof retained");
        let expected_extra = parallel.oracle.checked_retained_bytes().unwrap()
            + parallel.incumbent.capacity() as u128 * core::mem::size_of::<usize>() as u128;
        assert_eq!(
            preparation.checked_retained_capacity_bytes().unwrap(),
            initial + expected_extra
        );
        assert!(expected_extra > 0);
    }

    #[test]
    fn global_warm_admission_uses_checked_work_not_query_identity() {
        assert!(global_warm_work_is_admitted(256, 64, 24, 1000, 1));
        assert!(global_warm_work_is_admitted(256, 64, 24, 1000, 8));
        assert!(!global_warm_work_is_admitted(1024, 128, 32, 1000, 1));
        assert!(!global_warm_work_is_admitted(
            usize::MAX,
            usize::MAX,
            usize::MAX,
            usize::MAX,
            usize::MAX
        ));
        let query = ExactAtMostQuery::new_with_witness_hint(
            ExactAtMostQueryIdentity {
                matrix_id: [12; 32],
                generation: 1,
                query_id: 1,
            },
            PatternBitSet::from_words(3, vec![7]).unwrap(),
            [3, 5, 6]
                .into_iter()
                .map(|mask| PatternBitSet::from_words(3, vec![mask]).unwrap())
                .collect(),
            2,
            Some(vec![0, 1]),
        )
        .unwrap();
        assert_eq!(
            global_warm_admission(&query),
            GlobalWarmAdmission::KnownCover
        );
    }

    #[test]
    fn parallel_global_warm_pass_only_commits_replayed_positive_answers() {
        for limit in [1, 2] {
            let required = PatternBitSet::from_words(3, vec![7]).unwrap();
            let rows = [3, 5, 6]
                .into_iter()
                .map(|mask| PatternBitSet::from_words(3, vec![mask]).unwrap())
                .collect();
            let config = ParallelConfiguration {
                partitions: 8,
                matrix_id: [9; 32],
                next_query_id: AtomicU64::new(1),
            };
            let mut oracle = make_parallel_oracle(
                &config,
                required,
                rows,
                limit,
                Some(if limit == 2 {
                    vec![0, 1, 2]
                } else {
                    vec![0, 1]
                }),
                &mut |_| Ok(()),
                &mut || false,
            )
            .unwrap();
            assert!(oracle.published_query().is_none());
            assert!(oracle.take_task().is_none());
            assert!(
                oracle.checked_retained_bytes().unwrap()
                    > oracle.coordinator.checked_retained_bytes().unwrap()
            );
            for _ in 0..20000 {
                if oracle.warm_session.is_none() {
                    break;
                }
                assert!(matches!(
                    oracle
                        .advance_warm(8, 0, &mut |_| Ok(()), &mut || false)
                        .unwrap(),
                    Some(ParallelWarmAdvance::Pending(_)),
                ));
            }
            assert!(oracle.warm_session.is_none(), "bounded warm pass completed");
            if limit == 2 {
                assert!(matches!(oracle.coordinator.decision(),
                    ExactAtMostParallelDecision::Found(rows) if rows.len() == 2));
                assert!(oracle.published_query().is_none());
                assert!(oracle.take_task().is_none());
            } else {
                // An unsuccessful heuristic did not settle even one negative
                // partition; every cube is still owed a terminal receipt.
                assert!(matches!(
                    oracle.coordinator.decision(),
                    ExactAtMostParallelDecision::Pending { .. }
                ));
                let query = oracle.published_query().unwrap().clone();
                let task_count = oracle.coordinator.tasks().len();
                let mut issued = 0;
                while let Some(task) = oracle.take_task() {
                    issued += 1;
                    oracle.coordinator.accept(run(&query, task)).unwrap();
                }
                assert_eq!(issued, task_count);
                assert!(matches!(
                    oracle.coordinator.decision(),
                    ExactAtMostParallelDecision::ProvedNone
                ));
            }
        }
    }

    #[test]
    fn parallel_global_warm_pass_preserves_cancellation_and_memory_guard() {
        let required = PatternBitSet::from_words(3, vec![7]).unwrap();
        let rows: Vec<_> = [3, 5, 6]
            .into_iter()
            .map(|mask| PatternBitSet::from_words(3, vec![mask]).unwrap())
            .collect();
        let config = ParallelConfiguration {
            partitions: 8,
            matrix_id: [10; 32],
            next_query_id: AtomicU64::new(1),
        };
        let mut oracle = make_parallel_oracle(
            &config,
            required.clone(),
            rows.clone(),
            1,
            Some(vec![0, 1]),
            &mut |_| Ok(()),
            &mut || false,
        )
        .unwrap();
        assert!(matches!(
            oracle
                .advance_warm(8, 0, &mut |_| Ok(()), &mut || true)
                .unwrap(),
            Some(ParallelWarmAdvance::Cancelled(_)),
        ));
        assert!(matches!(
            oracle.coordinator.decision(),
            ExactAtMostParallelDecision::Pending { .. }
        ));
        let mut oracle = make_parallel_oracle(
            &config,
            required,
            rows,
            1,
            Some(vec![0, 1]),
            &mut |_| Ok(()),
            &mut || false,
        )
        .unwrap();
        // The first step moves the owned raw input into its warm cursor;
        // every later step must still guard the complete retained owner.
        oracle
            .advance_warm(8, 0, &mut |_| Ok(()), &mut || false)
            .unwrap();
        assert!(matches!(
            oracle.advance_warm(
                8,
                0,
                &mut |_| Err(ExactMinimumCoverError::MemoryGuardRejected),
                &mut || false,
            ),
            Err(ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::MemoryGuardRejected
            ))
        ));
    }

    #[test]
    fn parallel_proof_and_canonical_selector_match_original_row_serial_family() {
        // Equal and dominated original rows must remain visible to canonical
        // selection even when each proof shard uses lossless reductions.
        let required = PatternBitSet::from_words(3, vec![7]).unwrap();
        let rows: Vec<_> = [3, 5, 6, 3, 1]
            .into_iter()
            .map(|mask| PatternBitSet::from_words(3, vec![mask]).unwrap())
            .collect();
        let mut preparation =
            ExactMinimumCoverPortfolioPreparationSession::new(&required, &rows).unwrap();
        preparation.enable_parallel(8, [1; 32]).unwrap();
        preparation.enable_parallel(8, [1; 32]).unwrap();
        let mut enumerator = None;
        // A negative warm pass may consume its bounded 1,000 advisory swaps
        // before external exact cubes are issued. This is a test-only bound.
        for _ in 0..20000 {
            match preparation.advance(8).unwrap() {
                ExactMinimumCoverPortfolioPreparationAdvance::Coverable {
                    proof,
                    enumerator: ready,
                    ..
                } => {
                    assert_eq!(proof.row_indices().len(), 2);
                    enumerator = Some(ready);
                    break;
                }
                ExactMinimumCoverPortfolioPreparationAdvance::Pending { .. } => {}
                unexpected => panic!("unexpected preparation: {unexpected:?}"),
            }
            if let Some(query) = preparation.parallel_query().cloned() {
                let mut tasks = vec![];
                while let Some(task) = preparation.take_parallel_task() {
                    tasks.push(task);
                }
                for task in tasks.into_iter().rev() {
                    preparation
                        .accept_parallel_receipt(run(&query, task))
                        .unwrap();
                }
            }
        }
        let mut enumerator = enumerator.expect("parallel proof completed");
        enumerator.enable_parallel(8, [2; 32]).unwrap();
        let mut actual = vec![];
        for _ in 0..1000 {
            let page = enumerator
                .next_page_owned_with_memory_guard_and_control(1, 8, &mut |_| Ok(()), &mut || false)
                .unwrap();
            actual.extend(
                page.portfolios()
                    .iter()
                    .map(|portfolio| portfolio.row_indices().to_vec()),
            );
            if page.enumeration_complete() {
                break;
            }
            if let Some(query) = enumerator.parallel_query().cloned() {
                let mut tasks = vec![];
                while let Some(task) = enumerator.take_parallel_task() {
                    tasks.push(task);
                }
                for task in tasks.into_iter().rev() {
                    enumerator
                        .accept_parallel_receipt(run(&query, task))
                        .unwrap();
                }
            }
        }
        assert!(enumerator.enumeration_complete());
        let mut serial = ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).unwrap();
        let mut expected = vec![];
        for _ in 0..1000 {
            let page = serial
                .next_page_owned_with_memory_guard_and_control(
                    100,
                    u64::MAX,
                    &mut |_| Ok(()),
                    &mut || false,
                )
                .unwrap();
            expected.extend(
                page.portfolios()
                    .iter()
                    .map(|portfolio| portfolio.row_indices().to_vec()),
            );
            if page.enumeration_complete() {
                break;
            }
        }
        assert!(serial.enumeration_complete());
        assert_eq!(actual, expected);
        assert!(actual.iter().any(|rows| rows.contains(&3)));
    }

    #[test]
    fn parallel_first_page_handoff_resumes_serial_without_losing_query_or_frontier() {
        let required = PatternBitSet::from_words(3, vec![7]).unwrap();
        // The first numeric pair must NOT already cover: that legitimate
        // replay fast path never issues an oracle and cannot test handoff.
        let rows: Vec<_> = [1, 2, 5, 6, 3]
            .into_iter()
            .map(|mask| PatternBitSet::from_words(3, vec![mask]).unwrap())
            .collect();
        let mut enumerator = ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).unwrap();
        // Exercise an external oracle rather than the independent positive
        // warm prepass, which may legitimately settle this small example.
        enumerator.witness_hint = None;
        enumerator.enable_parallel(8, [3; 32]).unwrap();
        let mut first = None;
        let mut rejected_active_handoff = false;
        for _ in 0..1000 {
            let page = enumerator
                .next_page_owned_with_memory_guard_and_control(1, 8, &mut |_| Ok(()), &mut || false)
                .unwrap();
            if let Some(portfolio) = page.portfolios().first() {
                first = Some(portfolio.row_indices().to_vec());
                break;
            }
            if let Some(query) = enumerator.parallel_query().cloned() {
                let identity = query.identity();
                assert_eq!(
                    enumerator.disable_parallel_if_quiescent(),
                    Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)
                );
                assert_eq!(enumerator.parallel_query().unwrap().identity(), identity);
                rejected_active_handoff = true;
                let mut tasks = vec![];
                while let Some(task) = enumerator.take_parallel_task() {
                    tasks.push(task);
                }
                for task in tasks {
                    enumerator
                        .accept_parallel_receipt(run(&query, task))
                        .unwrap();
                }
            }
        }
        assert!(rejected_active_handoff);
        assert_eq!(first, Some(vec![0, 3]));
        let frontier = enumerator.next_combination.clone();
        let count = enumerator.known_alternative_count.clone();
        enumerator.disable_parallel_if_quiescent().unwrap();
        assert_eq!(enumerator.next_combination, frontier);
        assert_eq!(enumerator.known_alternative_count, count);
        let mut remainder = vec![];
        for _ in 0..1000 {
            let page = enumerator
                .next_page_owned_with_memory_guard_and_control(1, 8, &mut |_| Ok(()), &mut || false)
                .unwrap();
            assert!(
                enumerator.parallel_query().is_none(),
                "generic lazy page cannot await an absent external scheduler"
            );
            remainder.extend(
                page.portfolios()
                    .iter()
                    .map(|portfolio| portfolio.row_indices().to_vec()),
            );
            if page.enumeration_complete() {
                break;
            }
        }
        assert!(enumerator.enumeration_complete());
        assert_eq!(
            remainder,
            vec![vec![1, 2], vec![2, 3], vec![2, 4], vec![3, 4]]
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn make_parallel_oracle(
    config: &ParallelConfiguration,
    required: PatternBitSet,
    rows: Vec<PatternBitSet>,
    limit: usize,
    witness_hint: Option<Vec<usize>>,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ParallelOracle, ExactMinimumCoverPortfolioError> {
    let query_id = config
        .next_query_id
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .map_err(|_| ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?;
    let query = ExactAtMostQuery::new_with_witness_hint(
        ExactAtMostQueryIdentity {
            matrix_id: config.matrix_id,
            generation: query_id,
            query_id,
        },
        required,
        rows,
        limit,
        witness_hint,
    )
    .map_err(parallel_error)?;
    let mut coordinator =
        ExactAtMostCoordinator::prepare(query, config.partitions, memory_guard, cancelled)
            .map_err(parallel_error)?;
    let admission = global_warm_admission(coordinator.query());
    #[cfg(all(feature = "diagnostic-probes", not(target_arch = "wasm32")))]
    if coordinator.query().witness_hint().is_some() {
        eprintln!("{{\"phase\":\"global_warm_admission\",\"query\":{query_id},\"limit\":{limit},\"rows\":{},\"words\":{},\"admission\":\"{admission:?}\"}}",
            coordinator.query().rows().len(), coordinator.query().required().word_count());
    }
    if admission == GlobalWarmAdmission::KnownCover {
        let hint = try_clone_usize_slice_with_memory_guard(
            coordinator.query().witness_hint().expect("admitted hint"),
            coordinator
                .checked_retained_bytes()
                .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?,
            memory_guard,
            "parallel_global_warm_known_witness",
        )?;
        coordinator
            .accept_warm_witness(hint, memory_guard)
            .map_err(parallel_error)?;
    }
    let warm_session = if admission == GlobalWarmAdmission::Repair {
        let hint = coordinator.query().witness_hint().expect("admitted hint");
        let base = coordinator
            .checked_retained_bytes()
            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?;
        Some(
            ExactCoverSearchSession::prepare_at_most_with_memory_guard_and_control(
                coordinator.query().required(),
                coordinator.query().rows(),
                limit,
                Some(hint),
                &mut |owned| {
                    memory_guard(
                        base.checked_add(owned)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    )
                },
                cancelled,
            )
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?,
        )
    } else {
        None
    };
    Ok(ParallelOracle {
        coordinator,
        next_task: 0,
        warm_session,
        // Total global cursor work, not a fresh budget per supporter.
        warm_remaining_steps: WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET as u64,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GlobalWarmAdmission {
    Skip,
    KnownCover,
    Repair,
}

// A checked word-work estimate, not a wall-clock promise. It charges full
// matrix materialization/bit indexing, worst-case greedy row scans, and each
// selected-row/word scan of the admitted swap budget. Larger matrices retain
// the same exhaustive partition path without a coordinator-side warm pass.
fn global_warm_work_is_admitted(
    rows: usize,
    words: usize,
    selected: usize,
    swaps: usize,
    supporters: usize,
) -> bool {
    const MAX_ESTIMATED_WORD_WORK: u128 = 1_000_000_000;
    let estimate = (rows as u128)
        .checked_mul(words as u128)
        .and_then(|matrix| {
            (selected as u128)
                .checked_add(1)
                .and_then(|selected| {
                    selected.checked_mul((swaps as u128).checked_add(supporters as u128)?)
                })
                .and_then(|repair| {
                    repair
                        .checked_add(rows as u128)?
                        .checked_add(u64::BITS as u128)
                })
                .and_then(|passes| matrix.checked_mul(passes))
        });
    estimate.is_some_and(|work| work <= MAX_ESTIMATED_WORD_WORK)
}

fn global_warm_admission(query: &ExactAtMostQuery) -> GlobalWarmAdmission {
    let Some(hint) = query.witness_hint() else {
        return GlobalWarmAdmission::Skip;
    };
    if !(hint.len() == query.limit() || query.limit().checked_add(1) == Some(hint.len()))
        || !global_warm_work_is_admitted(
            query.rows().len(),
            query.required().word_count(),
            hint.len(),
            WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET,
            1,
        )
    {
        return GlobalWarmAdmission::Skip;
    }
    let mut missing = None;
    for word in 0..query.required().word_count() {
        let covered = hint
            .iter()
            .fold(0, |bits, &row| bits | query.rows()[row].word_at(word));
        let bits = query.required().word_at(word) & !covered;
        if bits == 0 {
            continue;
        }
        if bits.count_ones() != 1 || missing.is_some() {
            return GlobalWarmAdmission::Skip;
        }
        missing = Some(word * u64::BITS as usize + bits.trailing_zeros() as usize);
    }
    let Some(missing) = missing else {
        return if hint.len() <= query.limit() {
            GlobalWarmAdmission::KnownCover
        } else {
            GlobalWarmAdmission::Repair
        };
    };
    if hint.len() != query.limit() {
        return GlobalWarmAdmission::Skip;
    }
    let supporters = query
        .rows()
        .iter()
        .filter(|row| {
            row.word_at(missing / u64::BITS as usize) & (1_u64 << (missing % u64::BITS as usize))
                != 0
        })
        .count();
    if supporters != 0
        && global_warm_work_is_admitted(
            query.rows().len(),
            query.required().word_count(),
            hint.len(),
            WITNESS_ASSISTED_BREAKOUT_SWAP_BUDGET,
            supporters,
        )
    {
        GlobalWarmAdmission::Repair
    } else {
        GlobalWarmAdmission::Skip
    }
}

fn checked_preparation_input_retained_capacity_bytes(
    required: &PatternBitSet,
    rows: &Vec<PatternBitSet>,
) -> Result<u128, ExactMinimumCoverPortfolioError> {
    required
        .checked_storage_retained_bytes()
        .and_then(|bytes| bytes.checked_add(checked_pattern_bitset_vec_retained_bytes(rows).ok()?))
        .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
            ExactMinimumCoverError::ProjectionOverflow,
        ))
}

impl ExactMinimumCoverPortfolioEnumerator {
    pub fn enable_parallel(
        &mut self,
        partitions: usize,
        matrix_id: [u8; 32],
    ) -> Result<(), ExactMinimumCoverPortfolioError> {
        enable_configuration(&mut self.parallel, partitions, matrix_id)
    }

    pub fn prepare_parallel_idle_assist(
        &mut self,
        maximum_children: usize,
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<bool, ExactMinimumCoverPortfolioError> {
        match self.parallel_oracle_mut() {
            Some(oracle) => oracle.prepare_idle_assist(maximum_children, guard),
            None => Ok(false),
        }
    }

    pub fn parallel_task_is_redundant(
        &self,
        identity: ExactAtMostQueryIdentity,
        partition_id: u64,
    ) -> Result<bool, ExactMinimumCoverPortfolioError> {
        self.parallel_oracle()
            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?
            .coordinator
            .task_is_redundant(identity, partition_id)
            .map_err(parallel_error)
    }

    fn parallel_oracle(&self) -> Option<&ParallelOracle> {
        match &self.pending_search.as_ref()?.phase {
            PendingLexPhase::PivotOracle { oracle, .. }
            | PendingLexPhase::CanonicalOracle { oracle, .. } => oracle.parallel.as_ref(),
            _ => None,
        }
    }

    fn parallel_oracle_mut(&mut self) -> Option<&mut ParallelOracle> {
        match &mut self.pending_search.as_mut()?.phase {
            PendingLexPhase::PivotOracle { oracle, .. }
            | PendingLexPhase::CanonicalOracle { oracle, .. } => oracle.parallel.as_mut(),
            _ => None,
        }
    }

    pub fn parallel_query(&self) -> Option<&ExactAtMostQuery> {
        self.parallel_oracle()?.published_query()
    }

    pub fn parallel_query_satisfied(&self) -> bool {
        self.parallel_oracle().is_some_and(|oracle| {
            matches!(
                oracle.coordinator.decision(),
                ExactAtMostParallelDecision::Found(_)
            )
        })
    }

    /// Hand a quiescent continuation to an owner that drives ordinary lazy
    /// pages but has no external shard scheduler. Preserve the exact frontier,
    /// witness and any owned serial cursor; never discard an issued query.
    pub fn disable_parallel_if_quiescent(&mut self) -> Result<(), ExactMinimumCoverPortfolioError> {
        if self.parallel_oracle().is_some() {
            return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
        }
        self.parallel = None;
        Ok(())
    }

    pub fn take_parallel_task(&mut self) -> Option<ExactAtMostTask> {
        self.parallel_oracle_mut()?.take_task()
    }

    pub fn accept_parallel_receipt(
        &mut self,
        receipt: ExactAtMostReceipt,
    ) -> Result<(), ExactMinimumCoverPortfolioError> {
        self.parallel_oracle_mut()
            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?
            .coordinator
            .accept(receipt)
            .map_err(parallel_error)?;
        Ok(())
    }

    pub fn new(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        Self::new_with_memory_guard(required, rows, &mut |_| Ok(()))
    }

    /// Proves `k*` exactly once and constructs the original-row portfolio
    /// authority under one memory guard. Keeping the proof production and its
    /// consumption inside this constructor prevents a valid proof from one
    /// matrix from becoming stale minimum-cardinality authority for another.
    pub fn new_with_memory_guard(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        match Self::prepare_with_memory_guard(required, rows, memory_guard)? {
            ExactMinimumCoverPortfolioPreparation::Coverable { enumerator, .. } => Ok(enumerator),
            ExactMinimumCoverPortfolioPreparation::Incomplete { proof } => Err(
                ExactMinimumCoverPortfolioError::RequiredPatternsNotCoverable {
                    covered_pattern_count: proof.covered_patterns().count_ones(),
                    required_pattern_count: required.count_ones(),
                },
            ),
        }
    }

    /// Runs the exact proof once while preserving its incomplete-result
    /// semantics. On complete input, the returned enumerator is derived
    /// privately from that same proof, so no stale cross-matrix proof can be
    /// supplied by a caller and no second minimum-cover search is performed.
    pub fn prepare_with_memory_guard(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<ExactMinimumCoverPortfolioPreparation, ExactMinimumCoverPortfolioError> {
        let mut session = ExactMinimumCoverPortfolioPreparationSession::new_with_memory_guard(
            required,
            rows,
            memory_guard,
        )?;
        loop {
            match session.advance_with_memory_guard_and_control(
                u64::MAX,
                memory_guard,
                &mut || false,
            )? {
                ExactMinimumCoverPortfolioPreparationAdvance::Pending { .. } => {}
                ExactMinimumCoverPortfolioPreparationAdvance::Coverable {
                    proof,
                    enumerator,
                    ..
                } => {
                    return Ok(ExactMinimumCoverPortfolioPreparation::Coverable {
                        proof,
                        enumerator,
                    });
                }
                ExactMinimumCoverPortfolioPreparationAdvance::Incomplete { proof, .. } => {
                    return Ok(ExactMinimumCoverPortfolioPreparation::Incomplete { proof });
                }
                ExactMinimumCoverPortfolioPreparationAdvance::Cancelled { .. }
                | ExactMinimumCoverPortfolioPreparationAdvance::Finished => {
                    return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
                }
            }
        }
    }

    fn from_proof_with_memory_guard(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        proof: &ExactMinimumCoverResult,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let proof_retained_bytes =
            proof
                .checked_retained_bytes()
                .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ))?;
        memory_guard(proof_retained_bytes)
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        for (row_index, row) in rows.iter().enumerate() {
            if row.pattern_count() != required.pattern_count() {
                return Err(ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::RowPatternCountMismatch {
                        row_index,
                        expected: required.pattern_count(),
                        actual: row.pattern_count(),
                    },
                ));
            }
        }
        if !proof.complete() {
            return Err(
                ExactMinimumCoverPortfolioError::RequiredPatternsNotCoverable {
                    covered_pattern_count: proof.covered_patterns().count_ones(),
                    required_pattern_count: required.count_ones(),
                },
            );
        }
        let selected_rows = proof.row_indices();
        if proof.covered_patterns().pattern_count() != required.pattern_count()
            || selected_rows.len() > rows.len()
            || selected_rows.iter().any(|index| *index >= rows.len())
            || selected_rows.windows(2).any(|pair| pair[0] >= pair[1])
            || (required.is_empty() != selected_rows.is_empty())
            || (0..required.word_count()).any(|word_index| {
                let selected_covered = selected_rows.iter().fold(0_u64, |covered, row_index| {
                    covered | rows[*row_index].word_at(word_index)
                }) & required.word_at(word_index);
                selected_covered != required.word_at(word_index)
                    || selected_covered != proof.covered_patterns().word_at(word_index)
            })
        {
            return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
        }

        let mut live_bytes = proof_retained_bytes;
        let mut target_words = try_vec_with_memory_guard(
            required.word_count(),
            live_bytes,
            memory_guard,
            "exact_minimum_cover_portfolio_target",
        )?;
        live_bytes = checked_add_vec_retained_bytes(live_bytes, &target_words)?;
        for word_index in 0..required.word_count() {
            target_words.push(required.word_at(word_index));
        }

        let mut row_words = try_vec_with_memory_guard(
            rows.len(),
            live_bytes,
            memory_guard,
            "exact_minimum_cover_portfolio_rows",
        )?;
        live_bytes = checked_add_vec_retained_bytes(live_bytes, &row_words)?;
        for row in rows {
            let mut words = try_vec_with_memory_guard(
                required.word_count(),
                live_bytes,
                memory_guard,
                "exact_minimum_cover_portfolio_row_words",
            )?;
            live_bytes = checked_add_vec_retained_bytes(live_bytes, &words)?;
            for word_index in 0..required.word_count() {
                let word = row.word_at(word_index) & required.word_at(word_index);
                words.push(word);
            }
            row_words.push(words);
        }

        let optimal_cardinality = proof.row_indices().len();
        let mut first = try_vec_with_memory_guard(
            optimal_cardinality,
            live_bytes,
            memory_guard,
            "exact_minimum_cover_portfolio_first_combination",
        )?;
        live_bytes = checked_add_vec_retained_bytes(live_bytes, &first)?;
        first.extend(0..optimal_cardinality);
        let next_combination = Some(first);
        let mut witness_hint = try_vec_with_memory_guard(
            optimal_cardinality,
            live_bytes,
            memory_guard,
            "exact_minimum_cover_portfolio_witness_hint",
        )?;
        live_bytes = checked_add_vec_retained_bytes(live_bytes, &witness_hint)?;
        witness_hint.extend(selected_rows.iter().copied());
        let mut counter_digits = try_vec_with_memory_guard(
            1,
            live_bytes,
            memory_guard,
            "exact_minimum_cover_alternative_count",
        )?;
        counter_digits.push(0);
        Ok(Self {
            input: Arc::new(ExactMinimumCoverPortfolioInput {
                pattern_count: required.pattern_count(),
                row_words,
                target_words,
            }),
            optimal_cardinality,
            next_combination,
            known_alternative_count: DecimalCounter {
                digits: counter_digits,
            },
            enumeration_complete: false,
            witness_hint: Some(witness_hint),
            pending_search: None,
            parallel: None,
        })
    }

    /// Resumes a checkpoint against the same immutable semantic input. The
    /// minimum cardinality is reproved, so a checkpoint cannot silently change
    /// the optimization target. External snapshot integrity remains the
    /// responsibility of the layer that serializes this opaque value.
    pub fn resume(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        restart: ExactMinimumCoverRestart,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let mut enumerator = Self::new(required, rows)?;
        if restart.input.as_ref() != enumerator.input.as_ref()
            || restart.optimal_cardinality != enumerator.optimal_cardinality
            || !valid_restart_combination(
                restart.next_combination.as_deref(),
                restart.enumeration_complete,
                rows.len(),
                enumerator.optimal_cardinality,
            )
        {
            return Err(ExactMinimumCoverPortfolioError::InvalidRestart);
        }
        enumerator.next_combination = restart.next_combination;
        enumerator.known_alternative_count = restart.known_alternative_count;
        enumerator.enumeration_complete = restart.enumeration_complete;
        enumerator.pending_search = None;
        if enumerator.enumeration_complete {
            enumerator.witness_hint = None;
        }
        Ok(enumerator)
    }

    /// Reconstructs a restart from a persistence-safe field projection.
    ///
    /// The immutable semantic input and optimum are independently rebuilt and
    /// compared before any frontier state is accepted. Persistence layers must
    /// additionally bind these fields to their query/build/profile/candidate
    /// identities and authenticate the containing snapshot.
    pub fn resume_from_fields(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        optimal_cardinality: usize,
        next_combination: Option<Vec<usize>>,
        known_alternative_count_decimal: &str,
        enumeration_complete: bool,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let mut enumerator = Self::new(required, rows)?;
        if optimal_cardinality != enumerator.optimal_cardinality
            || !valid_restart_combination(
                next_combination.as_deref(),
                enumeration_complete,
                rows.len(),
                optimal_cardinality,
            )
        {
            return Err(ExactMinimumCoverPortfolioError::InvalidRestart);
        }
        let known_alternative_count = DecimalCounter::parse_canonical_bounded(
            known_alternative_count_decimal,
            maximum_subset_count_decimal_digits(rows.len()),
        )
        .ok_or(ExactMinimumCoverPortfolioError::InvalidRestart)?;
        enumerator.next_combination = next_combination;
        enumerator.known_alternative_count = known_alternative_count;
        enumerator.enumeration_complete = enumeration_complete;
        enumerator.pending_search = None;
        if enumerator.enumeration_complete {
            enumerator.witness_hint = None;
        }
        Ok(enumerator)
    }

    /// Applies a persistence frontier to this already proof-bound immutable
    /// input without proving `k*` again. This is only for a trusted in-memory
    /// authority whose outer layer has authenticated the checkpoint's query,
    /// candidate-map, build, and set identities. All cardinality, frontier,
    /// and count fields are still validated fail closed; private pending DFS
    /// state is intentionally discarded and safely replays from the inclusive
    /// frontier.
    pub fn resume_from_proven_fields(
        &self,
        optimal_cardinality: usize,
        next_combination: Option<Vec<usize>>,
        known_alternative_count_decimal: &str,
        enumeration_complete: bool,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        self.resume_from_proven_fields_with_memory_guard(
            optimal_cardinality,
            next_combination,
            known_alternative_count_decimal,
            enumeration_complete,
            &mut |_| Ok(()),
        )
    }

    pub fn resume_from_proven_fields_with_memory_guard(
        &self,
        optimal_cardinality: usize,
        next_combination: Option<Vec<usize>>,
        known_alternative_count_decimal: &str,
        enumeration_complete: bool,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        if optimal_cardinality != self.optimal_cardinality
            || !valid_restart_combination(
                next_combination.as_deref(),
                enumeration_complete,
                self.input.row_words.len(),
                optimal_cardinality,
            )
        {
            return Err(ExactMinimumCoverPortfolioError::InvalidRestart);
        }
        let maximum_digits = maximum_subset_count_decimal_digits(self.input.row_words.len());
        memory_guard(
            checked_input_retained_capacity_bytes(&self.input)
                .and_then(|bytes| bytes.checked_add(known_alternative_count_decimal.len() as u128))
                .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ))?,
        )
        .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        let known_alternative_count = DecimalCounter::parse_canonical_bounded(
            known_alternative_count_decimal,
            maximum_digits,
        )
        .ok_or(ExactMinimumCoverPortfolioError::InvalidRestart)?;
        let resumed = Self {
            input: Arc::clone(&self.input),
            optimal_cardinality,
            next_combination,
            known_alternative_count,
            enumeration_complete,
            witness_hint: None,
            pending_search: None,
            parallel: self.parallel.clone(),
        };
        memory_guard(resumed.checked_retained_capacity_bytes().ok_or(
            ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            ),
        )?)
        .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        Ok(resumed)
    }

    pub const fn optimal_cardinality(&self) -> usize {
        self.optimal_cardinality
    }

    pub fn known_alternative_count_decimal(&self) -> String {
        self.known_alternative_count.to_decimal_string()
    }

    /// Fallibly materializes the decimal count while adding its projected and
    /// actual String capacity to `external_live_bytes`. The caller supplies
    /// every concurrently retained owner, including this enumerator when it
    /// remains live; the callback therefore continues to receive whole-live
    /// bytes rather than a String-only delta.
    pub fn known_alternative_count_decimal_with_memory_guard(
        &self,
        external_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<String, ExactMinimumCoverPortfolioError> {
        self.known_alternative_count
            .to_decimal_string_with_memory_guard(external_live_bytes, memory_guard)
    }

    /// Exact heap payload retained by this enumerator. This includes the
    /// immutable restart input shared by clones, the current combination, and
    /// the arbitrary-precision decimal counter. Callers that share a cloned
    /// enumerator must charge the immutable input graph only once.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = checked_input_retained_capacity_bytes(&self.input)?;
        if let Some(combination) = &self.next_combination {
            bytes = bytes.checked_add(
                (combination.capacity() as u128)
                    .checked_mul(core::mem::size_of::<usize>() as u128)?,
            )?;
        }
        if let Some(witness_hint) = &self.witness_hint {
            bytes = bytes.checked_add(
                (witness_hint.capacity() as u128)
                    .checked_mul(core::mem::size_of::<usize>() as u128)?,
            )?;
        }
        if let Some(pending_search) = &self.pending_search {
            bytes = bytes.checked_add(pending_search.checked_retained_capacity_bytes()?)?;
        }
        bytes = bytes.checked_add(self.known_alternative_count.digits.capacity() as u128)?;
        Some(bytes)
    }

    /// Fallibly stages an independent mutable frontier while sharing the
    /// immutable input Arc. The callback unit is this staged clone's complete
    /// retained heap (the shared input is conservatively charged once) plus
    /// each clone-construction future/actual capacity. It does not include the
    /// original enumerator; an outer owner that keeps both live must add its
    /// own persistent baseline.
    pub fn try_clone_with_memory_guard(
        &self,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let mut live = checked_input_retained_capacity_bytes(&self.input).ok_or(
            ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            ),
        )?;
        memory_guard(live).map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        let next_combination = match &self.next_combination {
            Some(next) => {
                let cloned = try_clone_usize_slice_with_memory_guard(
                    next,
                    live,
                    memory_guard,
                    "exact_minimum_cover_staged_frontier",
                )?;
                live = checked_add_vec_retained_bytes(live, &cloned)?;
                Some(cloned)
            }
            None => None,
        };
        let known_alternative_count = self
            .known_alternative_count
            .try_clone_with_memory_guard(live, memory_guard)?;
        live = checked_add_vec_retained_bytes(live, &known_alternative_count.digits)?;
        let witness_hint = match &self.witness_hint {
            Some(hint) => {
                let cloned = try_clone_usize_slice_with_memory_guard(
                    hint,
                    live,
                    memory_guard,
                    "exact_minimum_cover_staged_witness_hint",
                )?;
                live = checked_add_vec_retained_bytes(live, &cloned)?;
                Some(cloned)
            }
            None => None,
        };
        let pending_search = match &self.pending_search {
            Some(pending) => {
                let cloned = pending.try_clone_with_memory_guard(live, memory_guard)?;
                live = checked_add_bytes(
                    live,
                    cloned.checked_retained_capacity_bytes().ok_or(
                        ExactMinimumCoverPortfolioError::MinimumCover(
                            ExactMinimumCoverError::ProjectionOverflow,
                        ),
                    )?,
                )?;
                Some(cloned)
            }
            None => None,
        };
        let cloned = Self {
            input: Arc::clone(&self.input),
            optimal_cardinality: self.optimal_cardinality,
            next_combination,
            known_alternative_count,
            enumeration_complete: self.enumeration_complete,
            witness_hint,
            pending_search,
            parallel: self.parallel.clone(),
        };
        memory_guard(live).map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        Ok(cloned)
    }

    pub const fn enumeration_complete(&self) -> bool {
        self.enumeration_complete
    }

    pub fn restart_state(&self) -> Option<ExactMinimumCoverRestart> {
        (!self.enumeration_complete).then(|| ExactMinimumCoverRestart {
            input: Arc::clone(&self.input),
            optimal_cardinality: self.optimal_cardinality,
            next_combination: self.next_combination.clone(),
            known_alternative_count: self.known_alternative_count.clone(),
            enumeration_complete: self.enumeration_complete,
        })
    }

    /// Consumes this authority and returns the first original-row portfolio in
    /// numeric lexicographic order. It is the canonical projection for callers
    /// that need one public representative but must not expose the
    /// branch-and-bound proof's internal row choice.
    pub fn into_canonical_portfolio(
        mut self,
    ) -> Result<Option<ExactMinimumCoverPortfolio>, ExactMinimumCoverPortfolioError> {
        // This adapter consumes the entire authority, so a failed slice has
        // no caller-owned frontier to roll back. Keep the exact cursor owned
        // here just as product construction does, instead of cloning its
        // growing memo through the ordinary transactional paging adapter.
        loop {
            let mut page = self.next_page_owned_with_memory_guard_and_control(
                1,
                u64::MAX,
                &mut |_| Ok(()),
                &mut || false,
            )?;
            if let Some(portfolio) = page.portfolios.pop() {
                return Ok(Some(portfolio));
            }
            if page.enumeration_complete() {
                return Ok(None);
            }
        }
    }

    /// Returns the next canonical portfolio without imposing a work cap. This
    /// is suitable for selecting the canonical first result after `k*` has
    /// already been proven. Interactive and distributed callers should use
    /// [`Self::next_page_with_control`] instead.
    pub fn next_portfolio(
        &mut self,
    ) -> Result<Option<ExactMinimumCoverPortfolio>, ExactMinimumCoverPortfolioError> {
        loop {
            let mut page = self.next_page(1, u64::MAX)?;
            if let Some(portfolio) = page.portfolios.pop() {
                return Ok(Some(portfolio));
            }
            if page.enumeration_complete() {
                return Ok(None);
            }
        }
    }

    pub fn next_page(
        &mut self,
        page_size: usize,
        max_work_steps: u64,
    ) -> Result<ExactMinimumCoverPortfolioPage, ExactMinimumCoverPortfolioError> {
        if page_size == 0 {
            return Err(ExactMinimumCoverPortfolioError::PageSizeMustBePositive);
        }

        // This is the legacy ungoverned adapter. The interactive APIs below
        // deliberately return after one wall-bounded inner oracle slice, even
        // when their caller supplies a very large logical work budget. Drive
        // those slices here only, while preserving the caller's total budget.
        let mut portfolios = Vec::new();
        portfolios.try_reserve_exact(page_size).map_err(|_| {
            ExactMinimumCoverPortfolioError::AllocationFailed {
                component: "exact_minimum_cover_blocking_page",
            }
        })?;
        let mut remaining = max_work_steps;
        let mut total_work_steps = 0_u64;
        let mut total_solver_cursor_work_steps = 0_u64;
        let mut total_candidates = 0_u64;
        let mut total_pruned = 0_u64;

        loop {
            let mut page =
                self.next_page_with_control(page_size - portfolios.len(), remaining, &mut || {
                    false
                })?;
            let consumed = page.work_steps;
            if consumed > remaining {
                return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
            }
            remaining -= consumed;
            total_work_steps = total_work_steps.checked_add(consumed).ok_or(
                ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ),
            )?;
            total_solver_cursor_work_steps = total_solver_cursor_work_steps
                .checked_add(page.solver_cursor_work_steps)
                .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ))?;
            total_candidates = total_candidates
                .checked_add(page.candidate_combinations_tested)
                .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ))?;
            total_pruned = total_pruned
                .checked_add(page.impossible_prefix_subtrees_pruned)
                .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ))?;
            portfolios.append(&mut page.portfolios);

            let terminal = portfolios.len() == page_size
                || page.enumeration_complete
                || remaining == 0
                || page.stop == ExactMinimumCoverEnumerationStop::Cancelled;
            if terminal {
                page.portfolios = portfolios;
                page.work_steps = total_work_steps;
                page.solver_cursor_work_steps = total_solver_cursor_work_steps;
                page.candidate_combinations_tested = total_candidates;
                page.impossible_prefix_subtrees_pruned = total_pruned;
                if page.portfolios.len() == page_size && !page.enumeration_complete {
                    page.stop = ExactMinimumCoverEnumerationStop::PageFull;
                } else if remaining == 0 && !page.enumeration_complete {
                    page.stop = ExactMinimumCoverEnumerationStop::WorkBudgetExhausted;
                }
                return Ok(page);
            }
            if consumed == 0 {
                return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
            }
        }
    }

    /// Legacy ungoverned wrapper around
    /// [`Self::next_page_with_memory_guard_and_control`].
    pub fn next_page_with_control(
        &mut self,
        page_size: usize,
        max_work_steps: u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<ExactMinimumCoverPortfolioPage, ExactMinimumCoverPortfolioError> {
        self.next_page_with_memory_guard_and_control(
            page_size,
            max_work_steps,
            &mut |_| Ok(()),
            cancelled,
        )
    }

    /// Advances the canonical self-reduction by at most `max_work_steps`
    /// bounded preparation/branch-and-bound work units across every internal
    /// AtMost query in this call.
    /// An unfinished oracle and the surrounding lexicographic phase are kept
    /// in memory, so a later call resumes the same DFS rather than replaying a
    /// long negative proof. A zero budget never starts or mutates that state.
    ///
    /// `memory_guard` always receives one unit: the complete retained heap
    /// conservatively attributed to this active enumerator (including its
    /// immutable Arc input once), plus every simultaneously live page,
    /// prefix/witness, augmented-query, heuristic, and inner exact-solver
    /// current-or-future allocation. It never receives a transient delta.
    /// Projected capacities are checked before reserve/conversion and actual
    /// allocator capacities immediately afterwards. A rejected guard is an
    /// error, never an empty or sealed result.
    ///
    /// The call is transactional on cancellation. If `cancelled` fires inside
    /// any AtMost oracle, neither newly visited solver nodes nor frontier,
    /// count, page-member, or diagnostic changes are committed; retrying
    /// resumes the exact trusted state that existed before this call. A
    /// persistence restart deliberately omits private DFS state and safely
    /// replays from the same inclusive numeric frontier.
    pub fn next_page_with_memory_guard_and_control(
        &mut self,
        page_size: usize,
        max_work_steps: u64,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<ExactMinimumCoverPortfolioPage, ExactMinimumCoverPortfolioError> {
        self.next_page_with_memory_guard_and_control_mode(
            page_size,
            max_work_steps,
            memory_guard,
            cancelled,
            true,
        )
    }

    /// Advances an exclusively owned enumerator without deep-cloning its
    /// in-flight exact-search cursor. This is reserved for product
    /// construction, where cancellation or failure discards the whole owner;
    /// ordinary public paging keeps the transactional method above.
    #[doc(hidden)]
    pub fn next_page_owned_with_memory_guard_and_control(
        &mut self,
        page_size: usize,
        max_work_steps: u64,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<ExactMinimumCoverPortfolioPage, ExactMinimumCoverPortfolioError> {
        self.next_page_with_memory_guard_and_control_mode(
            page_size,
            max_work_steps,
            memory_guard,
            cancelled,
            false,
        )
    }

    fn next_page_with_memory_guard_and_control_mode(
        &mut self,
        page_size: usize,
        max_work_steps: u64,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
        transactional: bool,
    ) -> Result<ExactMinimumCoverPortfolioPage, ExactMinimumCoverPortfolioError> {
        if page_size == 0 {
            return Err(ExactMinimumCoverPortfolioError::PageSizeMustBePositive);
        }
        let enumerator_live_before_transaction = self.checked_retained_capacity_bytes().ok_or(
            ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            ),
        )?;
        memory_guard(enumerator_live_before_transaction)
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;

        if cancelled() {
            return self.unchanged_page_with_memory_guard(
                ExactMinimumCoverEnumerationStop::Cancelled,
                memory_guard,
            );
        }
        if self.enumeration_complete {
            return self.unchanged_page_with_memory_guard(
                ExactMinimumCoverEnumerationStop::Sealed,
                memory_guard,
            );
        }
        if max_work_steps == 0 {
            return self.unchanged_page_with_memory_guard(
                ExactMinimumCoverEnumerationStop::WorkBudgetExhausted,
                memory_guard,
            );
        }

        let (mut working, enumerator_live) = if transactional {
            (
                PendingEnumerationState::try_clone_from_enumerator(
                    self,
                    enumerator_live_before_transaction,
                    memory_guard,
                )?,
                enumerator_live_before_transaction,
            )
        } else {
            let working = PendingEnumerationState::take_from_enumerator(self);
            let enumerator_live_after_take = self.checked_retained_capacity_bytes().ok_or(
                ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ),
            )?;
            (working, enumerator_live_after_take)
        };
        let mut portfolios = try_vec_with_memory_guard(
            page_size,
            checked_add_pending_state_bytes(enumerator_live, &working)?,
            memory_guard,
            "exact_minimum_cover_portfolio_page",
        )?;
        let mut work_steps = 0_u64;
        let mut solver_cursor_work_steps = 0_u64;
        let mut candidate_combinations_tested = 0_u64;
        let mut impossible_prefix_subtrees_pruned = 0_u64;
        let stop = loop {
            if working.enumeration_complete {
                break ExactMinimumCoverEnumerationStop::Sealed;
            }
            if cancelled() {
                if !transactional {
                    working.commit_into(self);
                }
                drop(portfolios);
                return self.unchanged_page_with_memory_guard(
                    ExactMinimumCoverEnumerationStop::Cancelled,
                    memory_guard,
                );
            }
            if work_steps >= max_work_steps {
                break ExactMinimumCoverEnumerationStop::WorkBudgetExhausted;
            }
            let Some(frontier) = working.next_combination.as_deref() else {
                working.enumeration_complete = true;
                break ExactMinimumCoverEnumerationStop::Sealed;
            };
            if working.pending_search.is_none() {
                let base_live =
                    checked_page_transaction_live_bytes(enumerator_live, &working, &portfolios)?;
                let allow_initial_heuristic =
                    working.known_alternative_count.is_zero() && is_first_combination(frontier);
                working.pending_search = Some(PendingLexSearch::try_new(
                    frontier,
                    working.witness_hint.as_deref(),
                    allow_initial_heuristic,
                    base_live,
                    memory_guard,
                )?);
            }
            let pending_live =
                checked_page_transaction_live_bytes(enumerator_live, &working, &portfolios)?;
            let decision = working
                .pending_search
                .as_mut()
                .expect("pending canonical search was initialized")
                .advance(
                    self,
                    max_work_steps - work_steps,
                    pending_live,
                    memory_guard,
                    cancelled,
                )?;
            match decision {
                LexSearchAdvance::Cancelled { visited_nodes } => {
                    let _discarded_transactional_work = visited_nodes;
                    drop(portfolios);
                    if !transactional {
                        working.commit_into(self);
                    }
                    return self.unchanged_page_with_memory_guard(
                        ExactMinimumCoverEnumerationStop::Cancelled,
                        memory_guard,
                    );
                }
                LexSearchAdvance::Pending { visited_nodes } => {
                    work_steps = work_steps.saturating_add(visited_nodes);
                    solver_cursor_work_steps =
                        solver_cursor_work_steps.saturating_add(visited_nodes);
                    break ExactMinimumCoverEnumerationStop::WorkBudgetExhausted;
                }
                LexSearchAdvance::ProvedNone { visited_nodes } => {
                    let classified = visited_nodes.max(1);
                    work_steps = work_steps.saturating_add(classified);
                    solver_cursor_work_steps =
                        solver_cursor_work_steps.saturating_add(classified - 1);
                    impossible_prefix_subtrees_pruned += 1;
                    working.pending_search = None;
                    working.next_combination = None;
                    working.witness_hint = None;
                    working.enumeration_complete = true;
                    break ExactMinimumCoverEnumerationStop::Sealed;
                }
                LexSearchAdvance::Found {
                    combination,
                    visited_nodes,
                } => {
                    let classified = visited_nodes.max(1);
                    work_steps = work_steps.saturating_add(classified);
                    solver_cursor_work_steps =
                        solver_cursor_work_steps.saturating_add(classified - 1);
                    candidate_combinations_tested += 1;
                    if combination.len() != self.optimal_cardinality
                        || combination.as_slice() < frontier
                        || combination.windows(2).any(|pair| pair[0] >= pair[1])
                        || !self.combination_covers(&combination)
                    {
                        return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
                    }
                    let live_with_combination = checked_page_transaction_live_bytes(
                        enumerator_live,
                        &working,
                        &portfolios,
                    )?
                    .checked_add(checked_vec_retained_bytes(&combination)?)
                    .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                        ExactMinimumCoverError::ProjectionOverflow,
                    ))?;
                    let successor = numeric_successor_with_memory_guard(
                        &combination,
                        self.input.row_words.len(),
                        live_with_combination,
                        memory_guard,
                    )?;
                    let count_live = live_with_combination
                        .checked_add(
                            successor
                                .as_ref()
                                .map(checked_vec_retained_bytes)
                                .transpose()?
                                .unwrap_or(0),
                        )
                        .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                            ExactMinimumCoverError::ProjectionOverflow,
                        ))?;
                    working
                        .known_alternative_count
                        .increment_with_memory_guard(count_live, memory_guard)?;
                    portfolios.push(ExactMinimumCoverPortfolio {
                        row_indices: combination,
                    });
                    working.next_combination = successor;
                    working.witness_hint = None;
                    working.pending_search = None;
                    working.enumeration_complete = working.next_combination.is_none();
                    if portfolios.len() == page_size {
                        break if working.enumeration_complete {
                            ExactMinimumCoverEnumerationStop::Sealed
                        } else {
                            ExactMinimumCoverEnumerationStop::PageFull
                        };
                    }
                }
            }
        };
        let page = build_page_with_memory_guard(
            self,
            &working,
            portfolios,
            stop,
            work_steps,
            solver_cursor_work_steps,
            candidate_combinations_tested,
            impossible_prefix_subtrees_pruned,
            memory_guard,
        )?;
        working.commit_into(self);
        Ok(page)
    }

    fn unchanged_page_with_memory_guard(
        &self,
        stop: ExactMinimumCoverEnumerationStop,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<ExactMinimumCoverPortfolioPage, ExactMinimumCoverPortfolioError> {
        let enumerator_live = self.checked_retained_capacity_bytes().ok_or(
            ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            ),
        )?;
        let working = PendingEnumerationState::try_clone_from_enumerator(
            self,
            enumerator_live,
            memory_guard,
        )?;
        let portfolios = try_vec_with_memory_guard(
            0,
            checked_add_pending_state_bytes(enumerator_live, &working)?,
            memory_guard,
            "exact_minimum_cover_portfolio_empty_page",
        )?;
        build_page_with_memory_guard(self, &working, portfolios, stop, 0, 0, 0, 0, memory_guard)
    }

    fn valid_witness(&self, witness: &[usize]) -> bool {
        witness.len() == self.optimal_cardinality
            && witness.iter().all(|row| *row < self.input.row_words.len())
            && witness.windows(2).all(|pair| pair[0] < pair[1])
            && self.combination_covers(witness)
    }

    fn combination_covers(&self, combination: &[usize]) -> bool {
        (0..self.input.target_words.len()).all(|word_index| {
            let covered = combination.iter().fold(0_u64, |covered, row_index| {
                covered | self.input.row_words[*row_index][word_index]
            });
            covered & self.input.target_words[word_index] == self.input.target_words[word_index]
        })
    }

    fn witness_from_query_proof(
        &self,
        prefix: &[usize],
        start: usize,
        selector_end: Option<usize>,
        proof: Vec<usize>,
        base_live: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Vec<usize>, ExactMinimumCoverPortfolioError> {
        let proof_live = checked_vec_retained_bytes(&proof)?;
        let mut witness = try_vec_with_memory_guard(
            self.optimal_cardinality,
            base_live.checked_add(proof_live).ok_or(
                ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ),
            )?,
            memory_guard,
            "exact_minimum_cover_query_witness",
        )?;
        witness.extend(prefix.iter().copied());
        witness.extend(proof.iter().map(|row| row + start));
        if !self.valid_witness(&witness)
            || witness.get(..prefix.len()) != Some(prefix)
            || selector_end
                .is_some_and(|end| witness.get(prefix.len()).is_none_or(|row| *row >= end))
        {
            return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
        }
        memory_guard(checked_add_bytes(
            base_live.checked_add(proof_live).ok_or(
                ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ),
            )?,
            checked_vec_retained_bytes(&witness)?,
        )?)
        .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        Ok(witness)
    }
}

#[derive(Clone)]
struct PendingLexSearch {
    frontier: Vec<usize>,
    witness_hint: Option<Vec<usize>>,
    allow_initial_assisted_query: bool,
    phase: PendingLexPhase,
}

#[derive(Clone)]
enum PendingLexPhase {
    Initial,
    Pivot {
        next_pivot_exclusive: usize,
    },
    PivotOracle {
        pivot: usize,
        prefix: Vec<usize>,
        start: usize,
        oracle: PendingAtMostOracle,
    },
    Canonicalize {
        prefix: Vec<usize>,
        start_floor: usize,
        witness: Vec<usize>,
        assisted_query_available: bool,
    },
    CanonicalOracle {
        prefix: Vec<usize>,
        start: usize,
        witness: Vec<usize>,
        witness_next: usize,
        oracle: PendingAtMostOracle,
    },
}

#[derive(Clone)]
struct PendingAtMostOracle {
    session: Option<ExactCoverSearchSession>,
    parallel: Option<ParallelOracle>,
}

enum LexSearchAdvance {
    Pending {
        visited_nodes: u64,
    },
    Found {
        combination: Vec<usize>,
        visited_nodes: u64,
    },
    ProvedNone {
        visited_nodes: u64,
    },
    Cancelled {
        visited_nodes: u64,
    },
}

enum PendingOracleStart {
    Ready(PendingAtMostOracle),
    ProvedNone,
    Cancelled,
}

enum PendingOracleAdvance {
    Pending {
        visited_nodes: u64,
    },
    Found {
        proof: Vec<usize>,
        visited_nodes: u64,
    },
    ProvedNone {
        visited_nodes: u64,
    },
    Cancelled {
        visited_nodes: u64,
    },
}

fn checked_add_visited_nodes(
    visited_nodes: u64,
    consumed: u64,
    max_nodes: u64,
) -> Result<u64, ExactMinimumCoverPortfolioError> {
    visited_nodes
        .checked_add(consumed)
        .filter(|total| *total <= max_nodes)
        .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)
}

impl PendingLexSearch {
    fn try_new(
        frontier: &[usize],
        witness_hint: Option<&[usize]>,
        allow_initial_assisted_query: bool,
        base_live: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let frontier = try_clone_usize_slice_with_memory_guard(
            frontier,
            base_live,
            memory_guard,
            "exact_minimum_cover_pending_lex_frontier",
        )?;
        let live = checked_add_vec_retained_bytes(base_live, &frontier)?;
        let witness_hint = match witness_hint {
            Some(hint) => Some(try_clone_usize_slice_with_memory_guard(
                hint,
                live,
                memory_guard,
                "exact_minimum_cover_pending_lex_hint",
            )?),
            None => None,
        };
        let pending = Self {
            frontier,
            witness_hint,
            allow_initial_assisted_query,
            phase: PendingLexPhase::Initial,
        };
        memory_guard(checked_add_bytes(
            base_live,
            pending.checked_retained_capacity_bytes().ok_or(
                ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ),
            )?,
        )?)
        .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        Ok(pending)
    }

    fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = checked_vec_retained_bytes(&self.frontier).ok()?;
        if let Some(hint) = &self.witness_hint {
            bytes = bytes.checked_add(checked_vec_retained_bytes(hint).ok()?)?;
        }
        bytes.checked_add(self.phase.checked_retained_capacity_bytes()?)
    }

    fn try_clone_with_memory_guard(
        &self,
        base_live: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let projected = self.checked_retained_capacity_bytes().ok_or(
            ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            ),
        )?;
        memory_guard(checked_add_bytes(base_live, projected)?)
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        let frontier = try_clone_usize_slice_with_memory_guard(
            &self.frontier,
            base_live,
            memory_guard,
            "exact_minimum_cover_cloned_lex_frontier",
        )?;
        let mut live = checked_add_vec_retained_bytes(base_live, &frontier)?;
        let witness_hint = match &self.witness_hint {
            Some(hint) => {
                let cloned = try_clone_usize_slice_with_memory_guard(
                    hint,
                    live,
                    memory_guard,
                    "exact_minimum_cover_cloned_lex_hint",
                )?;
                live = checked_add_vec_retained_bytes(live, &cloned)?;
                Some(cloned)
            }
            None => None,
        };
        let phase = self.phase.try_clone_with_memory_guard(live, memory_guard)?;
        live = checked_add_bytes(
            live,
            phase.checked_retained_capacity_bytes().ok_or(
                ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ),
            )?,
        )?;
        let cloned = Self {
            frontier,
            witness_hint,
            allow_initial_assisted_query: self.allow_initial_assisted_query,
            phase,
        };
        memory_guard(live).map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        Ok(cloned)
    }

    #[allow(clippy::too_many_arguments)]
    fn advance(
        &mut self,
        enumerator: &ExactMinimumCoverPortfolioEnumerator,
        max_nodes: u64,
        active_live: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<LexSearchAdvance, ExactMinimumCoverPortfolioError> {
        debug_assert!(max_nodes > 0);
        let mut visited_nodes = 0_u64;
        loop {
            if cancelled() {
                return Ok(LexSearchAdvance::Cancelled { visited_nodes });
            }
            let phase = core::mem::replace(&mut self.phase, PendingLexPhase::Initial);
            match phase {
                PendingLexPhase::Initial => {
                    if enumerator.combination_covers(&self.frontier) {
                        let combination = try_clone_usize_slice_with_memory_guard(
                            &self.frontier,
                            active_live,
                            memory_guard,
                            "exact_minimum_cover_frontier_witness",
                        )?;
                        return Ok(LexSearchAdvance::Found {
                            combination,
                            visited_nodes,
                        });
                    }
                    if is_first_combination(&self.frontier) {
                        if let Some(hint) = self
                            .witness_hint
                            .as_deref()
                            .filter(|hint| enumerator.valid_witness(hint))
                        {
                            let witness = try_clone_usize_slice_with_memory_guard(
                                hint,
                                active_live,
                                memory_guard,
                                "exact_minimum_cover_initial_witness",
                            )?;
                            self.phase = PendingLexPhase::Canonicalize {
                                prefix: Vec::new(),
                                start_floor: 0,
                                witness,
                                assisted_query_available: self.allow_initial_assisted_query,
                            };
                            continue;
                        }
                    }
                    self.phase = PendingLexPhase::Pivot {
                        next_pivot_exclusive: self.frontier.len(),
                    };
                }
                PendingLexPhase::Pivot {
                    next_pivot_exclusive,
                } => {
                    let Some(pivot) = next_pivot_exclusive.checked_sub(1) else {
                        return Ok(LexSearchAdvance::ProvedNone { visited_nodes });
                    };
                    let prefix = try_clone_usize_slice_with_memory_guard(
                        &self.frontier[..pivot],
                        active_live,
                        memory_guard,
                        "exact_minimum_cover_successor_prefix",
                    )?;
                    let start = self.frontier[pivot].checked_add(1).ok_or(
                        ExactMinimumCoverPortfolioError::MinimumCover(
                            ExactMinimumCoverError::ProjectionOverflow,
                        ),
                    )?;
                    let hinted = self.witness_hint.as_deref().filter(|hint| {
                        enumerator.valid_witness(hint)
                            && hint.get(..pivot) == Some(prefix.as_slice())
                            && hint.get(pivot).is_some_and(|row| *row >= start)
                    });
                    if let Some(hint) = hinted {
                        let witness = try_clone_usize_slice_with_memory_guard(
                            hint,
                            active_live,
                            memory_guard,
                            "exact_minimum_cover_successor_hint",
                        )?;
                        self.phase = PendingLexPhase::Canonicalize {
                            prefix,
                            start_floor: start,
                            witness,
                            assisted_query_available: false,
                        };
                        continue;
                    }
                    let query_base = checked_add_vec_retained_bytes(active_live, &prefix)?;
                    match PendingAtMostOracle::try_new(
                        enumerator,
                        &prefix,
                        start,
                        None,
                        None,
                        false,
                        query_base,
                        memory_guard,
                        cancelled,
                    )? {
                        PendingOracleStart::Ready(oracle) => {
                            self.phase = PendingLexPhase::PivotOracle {
                                pivot,
                                prefix,
                                start,
                                oracle,
                            };
                        }
                        PendingOracleStart::ProvedNone => {
                            self.phase = PendingLexPhase::Pivot {
                                next_pivot_exclusive: pivot,
                            };
                        }
                        PendingOracleStart::Cancelled => {
                            return Ok(LexSearchAdvance::Cancelled { visited_nodes });
                        }
                    }
                }
                PendingLexPhase::PivotOracle {
                    pivot,
                    prefix,
                    start,
                    mut oracle,
                } => {
                    let remaining = max_nodes - visited_nodes;
                    match oracle.advance(active_live, remaining, memory_guard, cancelled)? {
                        PendingOracleAdvance::Pending {
                            visited_nodes: consumed,
                        } => {
                            visited_nodes =
                                checked_add_visited_nodes(visited_nodes, consumed, max_nodes)?;
                            self.phase = PendingLexPhase::PivotOracle {
                                pivot,
                                prefix,
                                start,
                                oracle,
                            };
                            return Ok(LexSearchAdvance::Pending { visited_nodes });
                        }
                        PendingOracleAdvance::Found {
                            proof,
                            visited_nodes: consumed,
                        } => {
                            visited_nodes =
                                checked_add_visited_nodes(visited_nodes, consumed, max_nodes)?;
                            let witness_base = checked_add_bytes(
                                checked_add_vec_retained_bytes(active_live, &prefix)?,
                                oracle.checked_retained_capacity_bytes().ok_or(
                                    ExactMinimumCoverPortfolioError::MinimumCover(
                                        ExactMinimumCoverError::ProjectionOverflow,
                                    ),
                                )?,
                            )?;
                            let witness = enumerator.witness_from_query_proof(
                                &prefix,
                                start,
                                None,
                                proof,
                                witness_base,
                                memory_guard,
                            )?;
                            self.phase = PendingLexPhase::Canonicalize {
                                prefix,
                                start_floor: start,
                                witness,
                                assisted_query_available: false,
                            };
                        }
                        PendingOracleAdvance::ProvedNone {
                            visited_nodes: consumed,
                        } => {
                            visited_nodes =
                                checked_add_visited_nodes(visited_nodes, consumed, max_nodes)?;
                            self.phase = PendingLexPhase::Pivot {
                                next_pivot_exclusive: pivot,
                            };
                        }
                        PendingOracleAdvance::Cancelled {
                            visited_nodes: consumed,
                        } => {
                            visited_nodes =
                                checked_add_visited_nodes(visited_nodes, consumed, max_nodes)?;
                            return Ok(LexSearchAdvance::Cancelled { visited_nodes });
                        }
                    }
                    if visited_nodes >= max_nodes {
                        return Ok(LexSearchAdvance::Pending { visited_nodes });
                    }
                }
                PendingLexPhase::Canonicalize {
                    mut prefix,
                    mut start_floor,
                    witness,
                    mut assisted_query_available,
                } => {
                    if !enumerator.valid_witness(&witness)
                        || witness.get(..prefix.len()) != Some(prefix.as_slice())
                    {
                        return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
                    }
                    if prefix.len() == enumerator.optimal_cardinality {
                        if prefix != witness {
                            return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
                        }
                        return Ok(LexSearchAdvance::Found {
                            combination: witness,
                            visited_nodes,
                        });
                    }
                    let start = prefix
                        .last()
                        .map_or(start_floor, |row| row.saturating_add(1).max(start_floor));
                    let witness_next = witness[prefix.len()];
                    if witness_next < start {
                        return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
                    }
                    if start == witness_next {
                        prefix.push(witness_next);
                        start_floor = 0;
                        self.phase = PendingLexPhase::Canonicalize {
                            prefix,
                            start_floor,
                            witness,
                            assisted_query_available,
                        };
                        continue;
                    }
                    let use_assisted_query = assisted_query_available;
                    assisted_query_available = false;
                    let query_base = checked_add_vec_retained_bytes(
                        checked_add_vec_retained_bytes(active_live, &prefix)?,
                        &witness,
                    )?;
                    match PendingAtMostOracle::try_new(
                        enumerator,
                        &prefix,
                        start,
                        Some(witness_next),
                        Some(&witness),
                        use_assisted_query,
                        query_base,
                        memory_guard,
                        cancelled,
                    )? {
                        PendingOracleStart::Ready(oracle) => {
                            self.phase = PendingLexPhase::CanonicalOracle {
                                prefix,
                                start,
                                witness,
                                witness_next,
                                oracle,
                            };
                        }
                        PendingOracleStart::ProvedNone => {
                            prefix.push(witness_next);
                            self.phase = PendingLexPhase::Canonicalize {
                                prefix,
                                start_floor: 0,
                                witness,
                                assisted_query_available,
                            };
                        }
                        PendingOracleStart::Cancelled => {
                            return Ok(LexSearchAdvance::Cancelled { visited_nodes });
                        }
                    }
                }
                PendingLexPhase::CanonicalOracle {
                    mut prefix,
                    start,
                    witness,
                    witness_next,
                    mut oracle,
                } => {
                    let remaining = max_nodes - visited_nodes;
                    match oracle.advance(active_live, remaining, memory_guard, cancelled)? {
                        PendingOracleAdvance::Pending {
                            visited_nodes: consumed,
                        } => {
                            visited_nodes =
                                checked_add_visited_nodes(visited_nodes, consumed, max_nodes)?;
                            self.phase = PendingLexPhase::CanonicalOracle {
                                prefix,
                                start,
                                witness,
                                witness_next,
                                oracle,
                            };
                            return Ok(LexSearchAdvance::Pending { visited_nodes });
                        }
                        PendingOracleAdvance::Found {
                            proof,
                            visited_nodes: consumed,
                        } => {
                            visited_nodes =
                                checked_add_visited_nodes(visited_nodes, consumed, max_nodes)?;
                            let witness_base = checked_add_bytes(
                                checked_add_vec_retained_bytes(
                                    checked_add_vec_retained_bytes(active_live, &prefix)?,
                                    &witness,
                                )?,
                                oracle.checked_retained_capacity_bytes().ok_or(
                                    ExactMinimumCoverPortfolioError::MinimumCover(
                                        ExactMinimumCoverError::ProjectionOverflow,
                                    ),
                                )?,
                            )?;
                            let smaller = enumerator.witness_from_query_proof(
                                &prefix,
                                start,
                                Some(witness_next),
                                proof,
                                witness_base,
                                memory_guard,
                            )?;
                            if smaller.get(..prefix.len()) != Some(prefix.as_slice())
                                || smaller[prefix.len()] >= witness_next
                            {
                                return Err(
                                    ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof,
                                );
                            }
                            self.phase = PendingLexPhase::Canonicalize {
                                prefix,
                                start_floor: start,
                                witness: smaller,
                                assisted_query_available: false,
                            };
                        }
                        PendingOracleAdvance::ProvedNone {
                            visited_nodes: consumed,
                        } => {
                            visited_nodes =
                                checked_add_visited_nodes(visited_nodes, consumed, max_nodes)?;
                            prefix.push(witness_next);
                            self.phase = PendingLexPhase::Canonicalize {
                                prefix,
                                start_floor: 0,
                                witness,
                                assisted_query_available: false,
                            };
                        }
                        PendingOracleAdvance::Cancelled {
                            visited_nodes: consumed,
                        } => {
                            visited_nodes =
                                checked_add_visited_nodes(visited_nodes, consumed, max_nodes)?;
                            return Ok(LexSearchAdvance::Cancelled { visited_nodes });
                        }
                    }
                    if visited_nodes >= max_nodes {
                        return Ok(LexSearchAdvance::Pending { visited_nodes });
                    }
                }
            }
        }
    }
}

impl PendingLexPhase {
    fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = 0_u128;
        match self {
            Self::Initial | Self::Pivot { .. } => {}
            Self::PivotOracle { prefix, oracle, .. } => {
                bytes = bytes.checked_add(checked_vec_retained_bytes(prefix).ok()?)?;
                bytes = bytes.checked_add(oracle.checked_retained_capacity_bytes()?)?;
            }
            Self::Canonicalize {
                prefix, witness, ..
            } => {
                bytes = bytes.checked_add(checked_vec_retained_bytes(prefix).ok()?)?;
                bytes = bytes.checked_add(checked_vec_retained_bytes(witness).ok()?)?;
            }
            Self::CanonicalOracle {
                prefix,
                witness,
                oracle,
                ..
            } => {
                bytes = bytes.checked_add(checked_vec_retained_bytes(prefix).ok()?)?;
                bytes = bytes.checked_add(checked_vec_retained_bytes(witness).ok()?)?;
                bytes = bytes.checked_add(oracle.checked_retained_capacity_bytes()?)?;
            }
        }
        Some(bytes)
    }

    fn try_clone_with_memory_guard(
        &self,
        base_live: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let clone_vec = |source: &[usize],
                         live: u128,
                         memory_guard: &mut _|
         -> Result<Vec<usize>, ExactMinimumCoverPortfolioError> {
            try_clone_usize_slice_with_memory_guard(
                source,
                live,
                memory_guard,
                "exact_minimum_cover_cloned_lex_phase",
            )
        };
        match self {
            Self::Initial => Ok(Self::Initial),
            Self::Pivot {
                next_pivot_exclusive,
            } => Ok(Self::Pivot {
                next_pivot_exclusive: *next_pivot_exclusive,
            }),
            Self::PivotOracle {
                pivot,
                prefix,
                start,
                oracle,
            } => {
                let prefix = clone_vec(prefix, base_live, memory_guard)?;
                let live = checked_add_vec_retained_bytes(base_live, &prefix)?;
                let oracle = oracle.try_clone_with_memory_guard(live, memory_guard)?;
                Ok(Self::PivotOracle {
                    pivot: *pivot,
                    prefix,
                    start: *start,
                    oracle,
                })
            }
            Self::Canonicalize {
                prefix,
                start_floor,
                witness,
                assisted_query_available,
            } => {
                let prefix = clone_vec(prefix, base_live, memory_guard)?;
                let live = checked_add_vec_retained_bytes(base_live, &prefix)?;
                let witness = clone_vec(witness, live, memory_guard)?;
                Ok(Self::Canonicalize {
                    prefix,
                    start_floor: *start_floor,
                    witness,
                    assisted_query_available: *assisted_query_available,
                })
            }
            Self::CanonicalOracle {
                prefix,
                start,
                witness,
                witness_next,
                oracle,
            } => {
                let prefix = clone_vec(prefix, base_live, memory_guard)?;
                let mut live = checked_add_vec_retained_bytes(base_live, &prefix)?;
                let witness = clone_vec(witness, live, memory_guard)?;
                live = checked_add_vec_retained_bytes(live, &witness)?;
                let oracle = oracle.try_clone_with_memory_guard(live, memory_guard)?;
                Ok(Self::CanonicalOracle {
                    prefix,
                    start: *start,
                    witness,
                    witness_next: *witness_next,
                    oracle,
                })
            }
        }
    }
}

impl PendingAtMostOracle {
    #[allow(clippy::too_many_arguments)]
    fn try_new(
        enumerator: &ExactMinimumCoverPortfolioEnumerator,
        prefix: &[usize],
        start: usize,
        selector_end: Option<usize>,
        witness_hint: Option<&[usize]>,
        assisted_query: bool,
        base_live: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PendingOracleStart, ExactMinimumCoverPortfolioError> {
        let Some(slots) = enumerator.optimal_cardinality.checked_sub(prefix.len()) else {
            return Ok(PendingOracleStart::ProvedNone);
        };
        let Some(available) = enumerator.input.row_words.len().checked_sub(start) else {
            return Ok(PendingOracleStart::ProvedNone);
        };
        if available < slots
            || selector_end
                .is_some_and(|end| end <= start || end > enumerator.input.row_words.len())
        {
            return Ok(PendingOracleStart::ProvedNone);
        }
        if cancelled() {
            return Ok(PendingOracleStart::Cancelled);
        }
        let query = ExactLexQuery::try_new(
            enumerator,
            prefix,
            start,
            selector_end,
            base_live,
            memory_guard,
        )?;
        let query_live = checked_add_bytes(base_live, query.checked_retained_capacity_bytes()?)?;
        let mut local_witness_hint = Vec::new();
        if assisted_query {
            let hint = witness_hint.filter(|hint| {
                enumerator.valid_witness(hint) && hint.get(..prefix.len()) == Some(prefix)
            });
            let Some(hint) = hint else {
                return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
            };
            local_witness_hint = try_vec_with_memory_guard(
                slots,
                query_live,
                memory_guard,
                "exact_minimum_cover_query_local_witness_hint",
            )?;
            for row in hint.iter().copied().skip(prefix.len()) {
                if row < start {
                    return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
                }
                local_witness_hint.push(row - start);
            }
            if local_witness_hint.len() != slots
                || !local_witness_hint.windows(2).all(|pair| pair[0] < pair[1])
            {
                return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
            }
        }
        let oracle_live =
            checked_add_bytes(query_live, checked_vec_retained_bytes(&local_witness_hint)?)?;
        if let Some(config) = &enumerator.parallel {
            let parallel = make_parallel_oracle(
                config,
                query.required,
                query.rows,
                slots,
                assisted_query.then_some(local_witness_hint),
                &mut |owned| {
                    memory_guard(
                        base_live
                            .checked_add(owned)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    )
                },
                cancelled,
            )?;
            return Ok(PendingOracleStart::Ready(Self {
                session: None,
                parallel: Some(parallel),
            }));
        }
        let session = ExactCoverSearchSession::prepare_at_most_with_memory_guard_and_control(
            &query.required,
            &query.rows,
            slots,
            assisted_query.then_some(local_witness_hint.as_slice()),
            &mut |solver_owned| {
                memory_guard(
                    oracle_live
                        .checked_add(solver_owned)
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                )
            },
            cancelled,
        )
        .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        drop(local_witness_hint);
        drop(query);
        let oracle = Self {
            session: Some(session),
            parallel: None,
        };
        memory_guard(checked_add_bytes(
            base_live,
            oracle.checked_retained_capacity_bytes().ok_or(
                ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ),
            )?,
        )?)
        .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        Ok(PendingOracleStart::Ready(oracle))
    }

    fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        if let Some(parallel) = &self.parallel {
            return parallel.checked_retained_bytes();
        }
        self.session.as_ref()?.checked_retained_capacity_bytes()
    }

    fn try_clone_with_memory_guard(
        &self,
        base_live: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        if let Some(parallel) = &self.parallel {
            memory_guard(
                base_live
                    .checked_add(
                        parallel
                            .checked_retained_bytes()
                            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?,
                    )
                    .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?,
            )
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
            let cloned = Self {
                session: None,
                parallel: Some(parallel.clone()),
            };
            memory_guard(
                base_live
                    .checked_add(
                        cloned
                            .checked_retained_capacity_bytes()
                            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?,
                    )
                    .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?,
            )
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
            return Ok(cloned);
        }
        let session = self
            .session
            .as_ref()
            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?
            .try_clone_with_memory_guard(base_live, memory_guard)
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        Ok(Self {
            session: Some(session),
            parallel: None,
        })
    }

    fn advance(
        &mut self,
        active_live: u128,
        max_nodes: u64,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<PendingOracleAdvance, ExactMinimumCoverPortfolioError> {
        if let Some(parallel) = &mut self.parallel {
            if cancelled() {
                return Ok(PendingOracleAdvance::Cancelled { visited_nodes: 0 });
            }
            if let Some(warm) =
                parallel.advance_warm(max_nodes, active_live, memory_guard, cancelled)?
            {
                return Ok(match warm {
                    ParallelWarmAdvance::Pending(visited_nodes) => {
                        PendingOracleAdvance::Pending { visited_nodes }
                    }
                    ParallelWarmAdvance::Cancelled(visited_nodes) => {
                        PendingOracleAdvance::Cancelled { visited_nodes }
                    }
                });
            }
            if parallel.waiting_for_issued() {
                return Ok(PendingOracleAdvance::Pending { visited_nodes: 0 });
            }
            return Ok(match parallel.coordinator.decision() {
                ExactAtMostParallelDecision::Pending { .. } => {
                    PendingOracleAdvance::Pending { visited_nodes: 0 }
                }
                ExactAtMostParallelDecision::Found(rows) => PendingOracleAdvance::Found {
                    proof: rows.clone(),
                    visited_nodes: 1,
                },
                ExactAtMostParallelDecision::ProvedNone => {
                    PendingOracleAdvance::ProvedNone { visited_nodes: 1 }
                }
                ExactAtMostParallelDecision::Cancelled => {
                    PendingOracleAdvance::Cancelled { visited_nodes: 0 }
                }
            });
        }
        let advance = self
            .session
            .as_mut()
            .ok_or(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)?
            .advance(
                max_nodes,
                &mut |solver_owned| {
                    memory_guard(
                        active_live
                            .checked_add(solver_owned)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    )
                },
                cancelled,
            )
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        Ok(match advance {
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes } => {
                PendingOracleAdvance::Pending { visited_nodes }
            }
            ExactMinimumCoverSessionAdvance::Found {
                result,
                visited_nodes,
            } => PendingOracleAdvance::Found {
                proof: result.into_parts().0,
                visited_nodes,
            },
            ExactMinimumCoverSessionAdvance::ProvedNone { visited_nodes } => {
                PendingOracleAdvance::ProvedNone { visited_nodes }
            }
            ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes } => {
                PendingOracleAdvance::Cancelled { visited_nodes }
            }
            ExactMinimumCoverSessionAdvance::Finished => {
                return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
            }
        })
    }
}

struct ExactLexQuery {
    required: PatternBitSet,
    rows: Vec<PatternBitSet>,
}

impl ExactLexQuery {
    #[allow(clippy::too_many_arguments)]
    fn try_new(
        enumerator: &ExactMinimumCoverPortfolioEnumerator,
        prefix: &[usize],
        start: usize,
        selector_end: Option<usize>,
        base_live: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let input = &enumerator.input;
        let mut covered = try_vec_with_memory_guard(
            input.target_words.len(),
            base_live,
            memory_guard,
            "exact_minimum_cover_query_covered",
        )?;
        covered.resize(input.target_words.len(), 0_u64);
        for row in prefix.iter().copied() {
            for (covered_word, row_word) in
                covered.iter_mut().zip(input.row_words[row].iter().copied())
            {
                *covered_word |= row_word;
            }
        }
        let covered_bytes = checked_vec_retained_bytes(&covered)?;
        let pattern_count = input
            .pattern_count
            .checked_add(usize::from(selector_end.is_some()))
            .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            ))?;
        let word_count = pattern_count.div_ceil(u64::BITS as usize);
        let mut required_words = try_vec_with_memory_guard(
            word_count,
            base_live.checked_add(covered_bytes).ok_or(
                ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ),
            )?,
            memory_guard,
            "exact_minimum_cover_query_required_words",
        )?;
        required_words.resize(word_count, 0_u64);
        for word in 0..input.target_words.len() {
            required_words[word] = input.target_words[word] & !covered[word];
        }
        if selector_end.is_some() {
            required_words[input.pattern_count / u64::BITS as usize] |=
                1_u64 << (input.pattern_count % u64::BITS as usize);
        }
        let required_word_bytes = checked_vec_retained_bytes(&required_words)?;
        let required = dense_bitset_from_words_with_memory_guard(
            pattern_count,
            required_words,
            base_live
                .checked_add(covered_bytes)
                .and_then(|bytes| bytes.checked_add(required_word_bytes))
                .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ))?,
            memory_guard,
        )?;
        let required_storage = required.checked_storage_retained_bytes().ok_or(
            ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            ),
        )?;
        let live_before_rows = base_live
            .checked_add(covered_bytes)
            .and_then(|bytes| bytes.checked_add(required_storage))
            .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            ))?;
        memory_guard(live_before_rows).map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        let row_count = input.row_words.len() - start;
        let mut rows = try_vec_with_memory_guard(
            row_count,
            live_before_rows,
            memory_guard,
            "exact_minimum_cover_query_rows",
        )?;
        // The outer vector has its final capacity. Account each immutable
        // row once; rescanning the entire growing matrix before and after
        // every insertion makes each canonical suffix query quadratic.
        let mut rows_retained = checked_vec_retained_bytes(&rows)?;
        for row_index in start..input.row_words.len() {
            let row_base = live_before_rows.checked_add(rows_retained).ok_or(
                ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ),
            )?;
            let mut words = try_vec_with_memory_guard(
                word_count,
                row_base,
                memory_guard,
                "exact_minimum_cover_query_row_words",
            )?;
            words.resize(word_count, 0_u64);
            words[..input.target_words.len()].copy_from_slice(&input.row_words[row_index]);
            if selector_end.is_some_and(|end| row_index < end) {
                words[input.pattern_count / u64::BITS as usize] |=
                    1_u64 << (input.pattern_count % u64::BITS as usize);
            }
            let word_bytes = checked_vec_retained_bytes(&words)?;
            let row = dense_bitset_from_words_with_memory_guard(
                pattern_count,
                words,
                row_base.checked_add(word_bytes).ok_or(
                    ExactMinimumCoverPortfolioError::MinimumCover(
                        ExactMinimumCoverError::ProjectionOverflow,
                    ),
                )?,
                memory_guard,
            )?;
            rows_retained = checked_add_bytes(
                rows_retained,
                row.checked_storage_retained_bytes().ok_or(
                    ExactMinimumCoverPortfolioError::MinimumCover(
                        ExactMinimumCoverError::ProjectionOverflow,
                    ),
                )?,
            )?;
            rows.push(row);
            memory_guard(checked_add_bytes(live_before_rows, rows_retained)?)
                .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        }
        drop(covered);
        let query = Self { required, rows };
        memory_guard(checked_add_bytes(
            base_live,
            query.checked_retained_capacity_bytes()?,
        )?)
        .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        Ok(query)
    }

    fn checked_retained_capacity_bytes(&self) -> Result<u128, ExactMinimumCoverPortfolioError> {
        self.required
            .checked_storage_retained_bytes()
            .and_then(|bytes| {
                bytes.checked_add(checked_pattern_bitset_vec_retained_bytes(&self.rows).ok()?)
            })
            .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            ))
    }
}

#[derive(Clone)]
struct PendingEnumerationState {
    next_combination: Option<Vec<usize>>,
    known_alternative_count: DecimalCounter,
    enumeration_complete: bool,
    witness_hint: Option<Vec<usize>>,
    pending_search: Option<PendingLexSearch>,
}

impl PendingEnumerationState {
    fn take_from_enumerator(enumerator: &mut ExactMinimumCoverPortfolioEnumerator) -> Self {
        Self {
            next_combination: enumerator.next_combination.take(),
            known_alternative_count: core::mem::replace(
                &mut enumerator.known_alternative_count,
                DecimalCounter { digits: Vec::new() },
            ),
            enumeration_complete: enumerator.enumeration_complete,
            witness_hint: enumerator.witness_hint.take(),
            pending_search: enumerator.pending_search.take(),
        }
    }

    fn commit_into(self, enumerator: &mut ExactMinimumCoverPortfolioEnumerator) {
        enumerator.next_combination = self.next_combination;
        enumerator.known_alternative_count = self.known_alternative_count;
        enumerator.enumeration_complete = self.enumeration_complete;
        enumerator.witness_hint = self.witness_hint;
        enumerator.pending_search = self.pending_search;
    }

    fn try_clone_from_enumerator(
        enumerator: &ExactMinimumCoverPortfolioEnumerator,
        base_live: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let mut live = base_live;
        let next_combination = match &enumerator.next_combination {
            Some(next) => {
                let cloned = try_clone_usize_slice_with_memory_guard(
                    next,
                    live,
                    memory_guard,
                    "exact_minimum_cover_pending_frontier",
                )?;
                live = checked_add_vec_retained_bytes(live, &cloned)?;
                Some(cloned)
            }
            None => None,
        };
        let known_alternative_count = enumerator
            .known_alternative_count
            .try_clone_with_memory_guard(live, memory_guard)?;
        live = checked_add_vec_retained_bytes(live, &known_alternative_count.digits)?;
        let witness_hint = match &enumerator.witness_hint {
            Some(hint) => {
                let cloned = try_clone_usize_slice_with_memory_guard(
                    hint,
                    live,
                    memory_guard,
                    "exact_minimum_cover_pending_witness_hint",
                )?;
                live = checked_add_vec_retained_bytes(live, &cloned)?;
                Some(cloned)
            }
            None => None,
        };
        let pending_search = match &enumerator.pending_search {
            Some(pending) => {
                let cloned = pending.try_clone_with_memory_guard(live, memory_guard)?;
                live = live
                    .checked_add(cloned.checked_retained_capacity_bytes().ok_or(
                        ExactMinimumCoverPortfolioError::MinimumCover(
                            ExactMinimumCoverError::ProjectionOverflow,
                        ),
                    )?)
                    .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                        ExactMinimumCoverError::ProjectionOverflow,
                    ))?;
                Some(cloned)
            }
            None => None,
        };
        memory_guard(live).map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        Ok(Self {
            next_combination,
            known_alternative_count,
            enumeration_complete: enumerator.enumeration_complete,
            witness_hint,
            pending_search,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn build_page_with_memory_guard(
    enumerator: &ExactMinimumCoverPortfolioEnumerator,
    working: &PendingEnumerationState,
    portfolios: Vec<ExactMinimumCoverPortfolio>,
    stop: ExactMinimumCoverEnumerationStop,
    work_steps: u64,
    solver_cursor_work_steps: u64,
    candidate_combinations_tested: u64,
    impossible_prefix_subtrees_pruned: u64,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<ExactMinimumCoverPortfolioPage, ExactMinimumCoverPortfolioError> {
    if solver_cursor_work_steps
        .checked_add(candidate_combinations_tested)
        .and_then(|steps| steps.checked_add(impossible_prefix_subtrees_pruned))
        != Some(work_steps)
    {
        return Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof);
    }
    let enumerator_live = enumerator.checked_retained_capacity_bytes().ok_or(
        ExactMinimumCoverPortfolioError::MinimumCover(ExactMinimumCoverError::ProjectionOverflow),
    )?;
    let mut live = checked_page_transaction_live_bytes(enumerator_live, working, &portfolios)?;
    let known = working
        .known_alternative_count
        .to_decimal_string_with_memory_guard(live, memory_guard)?;
    live = live.checked_add(known.capacity() as u128).ok_or(
        ExactMinimumCoverPortfolioError::MinimumCover(ExactMinimumCoverError::ProjectionOverflow),
    )?;
    let total = if working.enumeration_complete {
        let total = try_clone_string_with_memory_guard(
            &known,
            live,
            memory_guard,
            "exact_minimum_cover_total_count",
        )?;
        live = live.checked_add(total.capacity() as u128).ok_or(
            ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            ),
        )?;
        Some(total)
    } else {
        None
    };
    let restart = if working.enumeration_complete {
        None
    } else {
        let next_combination = match &working.next_combination {
            Some(next) => {
                let cloned = try_clone_usize_slice_with_memory_guard(
                    next,
                    live,
                    memory_guard,
                    "exact_minimum_cover_page_restart_frontier",
                )?;
                live = checked_add_vec_retained_bytes(live, &cloned)?;
                Some(cloned)
            }
            None => None,
        };
        let known_alternative_count = working
            .known_alternative_count
            .try_clone_with_memory_guard(live, memory_guard)?;
        live = checked_add_vec_retained_bytes(live, &known_alternative_count.digits)?;
        Some(ExactMinimumCoverRestart {
            input: Arc::clone(&enumerator.input),
            optimal_cardinality: enumerator.optimal_cardinality,
            next_combination,
            known_alternative_count,
            enumeration_complete: false,
        })
    };
    let page = ExactMinimumCoverPortfolioPage {
        portfolios,
        optimal_cardinality: enumerator.optimal_cardinality,
        known_alternative_count_decimal: known,
        total_alternative_count_decimal: total,
        enumeration_complete: working.enumeration_complete,
        stop,
        work_steps,
        solver_cursor_work_steps,
        candidate_combinations_tested,
        impossible_prefix_subtrees_pruned,
        restart,
    };
    // The standalone page method deliberately double-counts the shared input,
    // so use the already tracked construction live here and let the caller's
    // final App-layer admission apply its own conservative owner accounting.
    memory_guard(live).map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
    Ok(page)
}

fn checked_add_pending_state_bytes(
    mut bytes: u128,
    working: &PendingEnumerationState,
) -> Result<u128, ExactMinimumCoverPortfolioError> {
    if let Some(next) = &working.next_combination {
        bytes = checked_add_vec_retained_bytes(bytes, next)?;
    }
    bytes = checked_add_vec_retained_bytes(bytes, &working.known_alternative_count.digits)?;
    if let Some(hint) = &working.witness_hint {
        bytes = checked_add_vec_retained_bytes(bytes, hint)?;
    }
    if let Some(pending) = &working.pending_search {
        bytes = checked_add_bytes(
            bytes,
            pending.checked_retained_capacity_bytes().ok_or(
                ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ),
            )?,
        )?;
    }
    Ok(bytes)
}

fn checked_page_transaction_live_bytes(
    base_live: u128,
    working: &PendingEnumerationState,
    portfolios: &Vec<ExactMinimumCoverPortfolio>,
) -> Result<u128, ExactMinimumCoverPortfolioError> {
    let mut bytes = checked_add_pending_state_bytes(base_live, working)?;
    bytes = checked_add_vec_retained_bytes(bytes, portfolios)?;
    for portfolio in portfolios {
        bytes = checked_add_vec_retained_bytes(bytes, &portfolio.row_indices)?;
    }
    Ok(bytes)
}

fn checked_pattern_bitset_vec_retained_bytes(
    rows: &Vec<PatternBitSet>,
) -> Result<u128, ExactMinimumCoverPortfolioError> {
    let mut bytes = checked_vec_retained_bytes(rows)?;
    for row in rows {
        bytes = bytes
            .checked_add(row.checked_storage_retained_bytes().ok_or(
                ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ),
            )?)
            .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            ))?;
    }
    Ok(bytes)
}

fn dense_bitset_from_words_with_memory_guard(
    pattern_count: usize,
    words: Vec<u64>,
    live_with_words: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<PatternBitSet, ExactMinimumCoverPortfolioError> {
    let dense_storage_bytes = (words.len() as u128)
        .checked_mul(core::mem::size_of::<u64>() as u128)
        .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
            ExactMinimumCoverError::ProjectionOverflow,
        ))?;
    memory_guard(checked_add_bytes(live_with_words, dense_storage_bytes)?)
        .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
    let words: Arc<[u64]> = words.into();
    PatternBitSet::from_shared_words(pattern_count, words)
        .map_err(|_| ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)
}

fn try_clone_usize_slice_with_memory_guard(
    source: &[usize],
    live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    component: &'static str,
) -> Result<Vec<usize>, ExactMinimumCoverPortfolioError> {
    let mut cloned = try_vec_with_memory_guard(source.len(), live_bytes, memory_guard, component)?;
    cloned.extend_from_slice(source);
    Ok(cloned)
}

fn try_clone_string_with_memory_guard(
    source: &str,
    live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    component: &'static str,
) -> Result<String, ExactMinimumCoverPortfolioError> {
    memory_guard(checked_add_bytes(live_bytes, source.len() as u128)?)
        .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
    let mut cloned = String::new();
    cloned
        .try_reserve_exact(source.len())
        .map_err(|_| ExactMinimumCoverPortfolioError::AllocationFailed { component })?;
    memory_guard(checked_add_bytes(live_bytes, cloned.capacity() as u128)?)
        .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
    cloned.push_str(source);
    Ok(cloned)
}

fn checked_vec_retained_bytes<T>(values: &Vec<T>) -> Result<u128, ExactMinimumCoverPortfolioError> {
    (values.capacity() as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
            ExactMinimumCoverError::ProjectionOverflow,
        ))
}

fn checked_add_bytes(left: u128, right: u128) -> Result<u128, ExactMinimumCoverPortfolioError> {
    left.checked_add(right)
        .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
            ExactMinimumCoverError::ProjectionOverflow,
        ))
}

fn is_first_combination(combination: &[usize]) -> bool {
    combination
        .iter()
        .copied()
        .enumerate()
        .all(|(index, row)| index == row)
}

fn numeric_successor_with_memory_guard(
    combination: &[usize],
    row_count: usize,
    live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<Option<Vec<usize>>, ExactMinimumCoverPortfolioError> {
    let mut next = try_clone_usize_slice_with_memory_guard(
        combination,
        live_bytes,
        memory_guard,
        "exact_minimum_cover_numeric_successor",
    )?;
    if advance_combination(&mut next, row_count) {
        Ok(Some(next))
    } else {
        Ok(None)
    }
}

fn try_vec_with_memory_guard<T>(
    capacity: usize,
    live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    component: &'static str,
) -> Result<Vec<T>, ExactMinimumCoverPortfolioError> {
    let requested_bytes = (capacity as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
            ExactMinimumCoverError::ProjectionOverflow,
        ))?;
    memory_guard(live_bytes.checked_add(requested_bytes).ok_or(
        ExactMinimumCoverPortfolioError::MinimumCover(ExactMinimumCoverError::ProjectionOverflow),
    )?)
    .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| ExactMinimumCoverPortfolioError::AllocationFailed { component })?;
    memory_guard(checked_add_vec_retained_bytes(live_bytes, &values)?)
        .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
    Ok(values)
}

fn checked_add_vec_retained_bytes<T>(
    live_bytes: u128,
    values: &Vec<T>,
) -> Result<u128, ExactMinimumCoverPortfolioError> {
    live_bytes
        .checked_add(
            (values.capacity() as u128)
                .checked_mul(core::mem::size_of::<T>() as u128)
                .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                    ExactMinimumCoverError::ProjectionOverflow,
                ))?,
        )
        .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
            ExactMinimumCoverError::ProjectionOverflow,
        ))
}

#[cfg(test)]
fn first_combination(row_count: usize, cardinality: usize) -> Option<Vec<usize>> {
    (cardinality <= row_count).then(|| (0..cardinality).collect())
}

fn advance_combination(combination: &mut [usize], row_count: usize) -> bool {
    advance_combination_after_prefix(combination, row_count, combination.len())
}

fn advance_combination_after_prefix(
    combination: &mut [usize],
    row_count: usize,
    prefix_len: usize,
) -> bool {
    let cardinality = combination.len();
    debug_assert!(prefix_len <= cardinality);
    for index in (0..prefix_len).rev() {
        let maximum = row_count - cardinality + index;
        if combination[index] < maximum {
            combination[index] += 1;
            for suffix in index + 1..cardinality {
                combination[suffix] = combination[suffix - 1] + 1;
            }
            return true;
        }
    }
    false
}

fn valid_restart_combination(
    combination: Option<&[usize]>,
    complete: bool,
    row_count: usize,
    cardinality: usize,
) -> bool {
    if complete {
        return combination.is_none();
    }
    let Some(combination) = combination else {
        return false;
    };
    combination.len() == cardinality
        && combination.iter().all(|index| *index < row_count)
        && combination.windows(2).all(|pair| pair[0] < pair[1])
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DecimalCounter {
    // Little-endian base-10 digits make increment exact without a fixed-width
    // integer ceiling. The public representation is canonical decimal text.
    digits: Vec<u8>,
}

impl DecimalCounter {
    fn parse_canonical_bounded(value: &str, maximum_digits: usize) -> Option<Self> {
        if value.is_empty()
            || value.len() > maximum_digits.max(1)
            || (value.len() > 1 && value.starts_with('0'))
            || !value.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let mut digits = Vec::new();
        digits.try_reserve_exact(value.len()).ok()?;
        digits.extend(value.bytes().rev().map(|byte| byte - b'0'));
        Some(Self { digits })
    }

    fn increment(&mut self) -> Result<(), ExactMinimumCoverPortfolioError> {
        for digit in &mut self.digits {
            if *digit < 9 {
                *digit += 1;
                return Ok(());
            }
            *digit = 0;
        }
        self.digits.try_reserve_exact(1).map_err(|_| {
            ExactMinimumCoverPortfolioError::AllocationFailed {
                component: "exact_minimum_cover_alternative_count",
            }
        })?;
        self.digits.push(1);
        Ok(())
    }

    fn increment_with_memory_guard(
        &mut self,
        whole_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<(), ExactMinimumCoverPortfolioError> {
        if self.digits.iter().any(|digit| *digit < 9) {
            return self.increment();
        }
        memory_guard(checked_add_bytes(whole_live_bytes, 1)?)
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        let previous_capacity = self.digits.capacity() as u128;
        self.digits.try_reserve_exact(1).map_err(|_| {
            ExactMinimumCoverPortfolioError::AllocationFailed {
                component: "exact_minimum_cover_alternative_count",
            }
        })?;
        let live_after_reserve = whole_live_bytes
            .checked_sub(previous_capacity)
            .and_then(|bytes| bytes.checked_add(self.digits.capacity() as u128))
            .ok_or(ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            ))?;
        memory_guard(live_after_reserve).map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        self.increment()
    }

    fn try_clone_with_memory_guard(
        &self,
        live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverPortfolioError> {
        let mut digits = try_vec_with_memory_guard(
            self.digits.len(),
            live_bytes,
            memory_guard,
            "exact_minimum_cover_count_clone",
        )?;
        digits.extend_from_slice(&self.digits);
        Ok(Self { digits })
    }

    fn is_zero(&self) -> bool {
        self.digits.iter().all(|digit| *digit == 0)
    }

    fn to_decimal_string(&self) -> String {
        self.digits
            .iter()
            .rev()
            .map(|digit| char::from(b'0' + *digit))
            .collect()
    }

    fn to_decimal_string_with_memory_guard(
        &self,
        live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<String, ExactMinimumCoverPortfolioError> {
        memory_guard(checked_add_bytes(live_bytes, self.digits.len() as u128)?)
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        let mut output = String::new();
        output.try_reserve_exact(self.digits.len()).map_err(|_| {
            ExactMinimumCoverPortfolioError::AllocationFailed {
                component: "exact_minimum_cover_count_string",
            }
        })?;
        memory_guard(checked_add_bytes(live_bytes, output.capacity() as u128)?)
            .map_err(ExactMinimumCoverPortfolioError::MinimumCover)?;
        output.extend(
            self.digits
                .iter()
                .rev()
                .map(|digit| char::from(b'0' + *digit)),
        );
        Ok(output)
    }
}

fn maximum_subset_count_decimal_digits(row_count: usize) -> usize {
    // Every portfolio is one subset, so the exact count cannot exceed 2^n.
    // ceil(n * log10(2)) + one guard digit, using an integer upper bound for
    // log10(2). This also bounds hostile persisted decimal allocation.
    row_count
        .saturating_mul(30_104)
        .div_ceil(100_000)
        .saturating_add(1)
}

#[cfg(test)]
mod tests {
    use crate::cover::exact_minimum_cover::exact_minimum_cover;
    use crate::pattern::pattern_id::PatternId;

    use super::*;

    fn bitset(pattern_count: usize, patterns: &[usize]) -> PatternBitSet {
        PatternBitSet::from_patterns(pattern_count, patterns.iter().copied().map(PatternId::new))
            .expect("valid bitset")
    }

    fn row_vectors(page: &ExactMinimumCoverPortfolioPage) -> Vec<Vec<usize>> {
        page.portfolios()
            .iter()
            .map(|portfolio| portfolio.row_indices().to_vec())
            .collect()
    }

    #[test]
    fn equal_rows_remain_distinct_exact_alternatives() {
        let required = bitset(1, &[0]);
        let rows = vec![bitset(1, &[0]), bitset(1, &[0])];
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");

        let first = enumerator.next_page(1, 1).expect("first page");
        assert_eq!(row_vectors(&first), vec![vec![0]]);
        assert_eq!(first.known_alternative_count_decimal(), "1");
        assert_eq!(first.total_alternative_count_decimal(), None);
        assert_eq!(first.stop(), ExactMinimumCoverEnumerationStop::PageFull);
        assert!(!first.enumeration_complete());

        let second = enumerator.next_page(1, 1).expect("second page");
        assert_eq!(row_vectors(&second), vec![vec![1]]);
        assert_eq!(second.known_alternative_count_decimal(), "2");
        assert_eq!(second.total_alternative_count_decimal(), Some("2"));
        assert_eq!(second.stop(), ExactMinimumCoverEnumerationStop::Sealed);
        assert!(second.enumeration_complete());
    }

    #[test]
    fn dominated_original_row_identity_can_participate_in_an_optimal_cover() {
        let required = bitset(3, &[0, 1, 2]);
        let rows = vec![bitset(3, &[0, 1]), bitset(3, &[0]), bitset(3, &[1, 2])];
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");

        let page = enumerator
            .next_page(10, u64::MAX)
            .expect("all alternatives through the legacy blocking adapter");

        assert_eq!(page.optimal_cardinality(), 2);
        assert_eq!(row_vectors(&page), vec![vec![0, 2], vec![1, 2]]);
        assert_eq!(page.total_alternative_count_decimal(), Some("2"));
    }

    #[test]
    fn cooperative_page_hard_yields_even_with_unbounded_logical_budget() {
        let required = bitset(3, &[0, 1, 2]);
        let rows = vec![bitset(3, &[0, 1]), bitset(3, &[0]), bitset(3, &[1, 2])];
        let mut cooperative =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");
        let first_slice = cooperative
            .next_page_with_control(10, u64::MAX, &mut || false)
            .expect("one cooperative ABI slice");
        assert!(first_slice.portfolios().is_empty());
        assert_eq!(
            first_slice.stop(),
            ExactMinimumCoverEnumerationStop::WorkBudgetExhausted
        );
        assert_eq!(
            first_slice.work_steps(),
            1,
            "an unbounded logical budget must not bypass the inner wall cap"
        );

        let mut blocking =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");
        let complete = blocking
            .next_page(10, u64::MAX)
            .expect("legacy blocking adapter");
        assert_eq!(row_vectors(&complete), vec![vec![0, 2], vec![1, 2]]);
        assert!(complete.enumeration_complete());
    }

    #[test]
    fn exclusively_owned_page_matches_transactional_cooperative_progress() {
        let required = bitset(3, &[0, 1, 2]);
        let rows = vec![bitset(3, &[0, 1]), bitset(3, &[0]), bitset(3, &[1, 2])];
        let mut transactional =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");
        let mut owned =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");

        for _ in 0..16 {
            let expected = transactional
                .next_page_with_memory_guard_and_control(1, 1, &mut |_| Ok(()), &mut || false)
                .expect("transactional page");
            let actual = owned
                .next_page_owned_with_memory_guard_and_control(1, 1, &mut |_| Ok(()), &mut || false)
                .expect("owned page");
            assert_eq!(row_vectors(&actual), row_vectors(&expected));
            assert_eq!(actual.stop(), expected.stop());
            assert_eq!(actual.work_steps(), expected.work_steps());
            assert_eq!(
                actual.enumeration_complete(),
                expected.enumeration_complete()
            );
            assert_eq!(owned.restart_state(), transactional.restart_state());
            if actual.enumeration_complete() || !actual.portfolios().is_empty() {
                return;
            }
        }
        panic!("owned canonical page did not match a terminal transactional slice");
    }

    #[test]
    fn portfolios_are_numeric_lexicographic_and_not_search_ordered() {
        let required = bitset(2, &[0, 1]);
        let rows = vec![
            bitset(2, &[0]),
            bitset(2, &[1]),
            bitset(2, &[0]),
            bitset(2, &[1]),
        ];
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");

        let page = enumerator.next_page(10, 20).expect("all alternatives");

        assert_eq!(
            row_vectors(&page),
            vec![vec![0, 1], vec![0, 3], vec![1, 2], vec![2, 3]]
        );
        assert!(page.enumeration_complete());
    }

    #[test]
    fn proven_minimum_is_projected_to_the_original_row_lexicographic_identity() {
        let required = bitset(3, &[0, 1, 2]);
        let rows = vec![
            bitset(3, &[1, 2]),
            bitset(3, &[0]),
            bitset(3, &[0, 1]),
            bitset(3, &[]),
        ];
        let proof = exact_minimum_cover(&required, &rows).expect("minimum proof");
        assert_eq!(proof.row_indices(), &[0, 2]);

        let enumerator = ExactMinimumCoverPortfolioEnumerator::new_with_memory_guard(
            &required,
            &rows,
            &mut |_| Ok(()),
        )
        .expect("guarded enumerator");
        let canonical = enumerator
            .into_canonical_portfolio()
            .expect("canonical portfolio")
            .expect("one portfolio");

        assert_eq!(canonical.row_indices(), &[0, 1]);
    }

    #[test]
    fn proven_minimum_construction_reports_exact_guard_peak_and_rejects_peak_minus_one() {
        let required = bitset(3, &[0, 1, 2]);
        let rows = vec![bitset(3, &[0, 1]), bitset(3, &[0]), bitset(3, &[1, 2])];
        let proof = exact_minimum_cover(&required, &rows).expect("minimum proof");
        let mut peak = 0_u128;
        let enumerator = ExactMinimumCoverPortfolioEnumerator::new_with_memory_guard(
            &required,
            &rows,
            &mut |live_and_future_bytes| {
                peak = peak.max(live_and_future_bytes);
                Ok(())
            },
        )
        .expect("dry-run construction");
        assert!(peak > proof.checked_retained_bytes().expect("proof bytes"));
        assert!(enumerator.checked_retained_capacity_bytes().is_some());

        let limit = peak - 1;
        let error = ExactMinimumCoverPortfolioEnumerator::new_with_memory_guard(
            &required,
            &rows,
            &mut |required_memory_bytes| {
                if required_memory_bytes > limit {
                    return Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                        required_memory_bytes,
                        max_memory_bytes: limit,
                    });
                }
                Ok(())
            },
        )
        .expect_err("peak minus one must reject");
        assert!(matches!(
            error,
            ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::MemoryCapacityExceeded {
                    required_memory_bytes,
                    max_memory_bytes,
                }
            ) if required_memory_bytes > max_memory_bytes
        ));
    }

    #[test]
    fn staged_clone_is_fallible_guarded_and_shares_only_immutable_input() {
        let required = bitset(3, &[0, 1, 2]);
        let rows = vec![bitset(3, &[0, 1]), bitset(3, &[0]), bitset(3, &[1, 2])];
        let enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");
        let mut peak = 0_u128;
        let cloned = enumerator
            .try_clone_with_memory_guard(&mut |whole_clone_live| {
                peak = peak.max(whole_clone_live);
                Ok(())
            })
            .expect("guarded staged clone");
        assert!(Arc::ptr_eq(&enumerator.input, &cloned.input));
        assert_eq!(enumerator.next_combination, cloned.next_combination);
        assert_eq!(
            peak,
            cloned
                .checked_retained_capacity_bytes()
                .expect("clone retained bytes")
        );

        let limit = peak - 1;
        let error = enumerator
            .try_clone_with_memory_guard(&mut |whole_clone_live| {
                if whole_clone_live > limit {
                    return Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                        required_memory_bytes: whole_clone_live,
                        max_memory_bytes: limit,
                    });
                }
                Ok(())
            })
            .expect_err("clone peak minus one must reject");
        assert!(matches!(
            error,
            ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::MemoryCapacityExceeded {
                    required_memory_bytes,
                    max_memory_bytes,
                }
            ) if required_memory_bytes > max_memory_bytes
        ));
    }

    #[test]
    fn guarded_known_count_materialization_accounts_for_external_whole_live() {
        let required = bitset(3, &[0, 1, 2]);
        let rows = vec![bitset(3, &[0, 1]), bitset(3, &[0]), bitset(3, &[1, 2])];
        let enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");
        let external_live_bytes = 137_u128;
        let mut peak = 0_u128;
        let decimal = enumerator
            .known_alternative_count_decimal_with_memory_guard(
                external_live_bytes,
                &mut |whole_live| {
                    peak = peak.max(whole_live);
                    Ok(())
                },
            )
            .expect("guarded count materialization");
        assert_eq!(decimal, "0");
        assert!(peak >= external_live_bytes + decimal.capacity() as u128);

        let limit = peak - 1;
        let error = enumerator
            .known_alternative_count_decimal_with_memory_guard(
                external_live_bytes,
                &mut |whole_live| {
                    if whole_live > limit {
                        return Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                            required_memory_bytes: whole_live,
                            max_memory_bytes: limit,
                        });
                    }
                    Ok(())
                },
            )
            .expect_err("count peak minus one must reject");
        assert!(matches!(
            error,
            ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::MemoryCapacityExceeded {
                    required_memory_bytes,
                    max_memory_bytes,
                }
            ) if required_memory_bytes > max_memory_bytes
        ));
    }

    #[test]
    fn proven_minimum_rejects_stale_row_identity_before_using_cardinality() {
        let required = bitset(2, &[0, 1]);
        let proof_rows = vec![bitset(2, &[0]), bitset(2, &[1])];
        let proof = exact_minimum_cover(&required, &proof_rows).expect("minimum proof");
        let changed_rows = vec![bitset(2, &[1]), bitset(2, &[1])];

        assert!(matches!(
            ExactMinimumCoverPortfolioEnumerator::from_proof_with_memory_guard(
                &required,
                &changed_rows,
                &proof,
                &mut |_| Ok(()),
            ),
            Err(ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof)
        ));
    }

    #[test]
    fn batched_self_reduction_preserves_every_original_identity_and_lexicographic_order() {
        let required = bitset(3, &[0, 1, 2]);
        let rows = vec![
            bitset(3, &[0]),
            bitset(3, &[0]),
            bitset(3, &[1]),
            bitset(3, &[1]),
            bitset(3, &[2]),
            bitset(3, &[2]),
            bitset(3, &[2]),
        ];
        let mut expected = Vec::new();
        for first in 0..rows.len() {
            for second in first + 1..rows.len() {
                for third in second + 1..rows.len() {
                    let combination = [first, second, third];
                    if combination_covers_for_test(&required, &rows, &combination) {
                        expected.push(combination.to_vec());
                    }
                }
            }
        }
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");

        let page = enumerator.next_page(100, 100).expect("all alternatives");

        assert_eq!(page.optimal_cardinality(), 3);
        assert_eq!(row_vectors(&page), expected);
        assert_eq!(row_vectors(&page).len(), 12);
        assert!(page.impossible_prefix_subtrees_pruned() > 0);
        assert!(page.candidate_combinations_tested() < 35);
        assert!(page.work_steps() <= 100);
        assert!(page.enumeration_complete());
    }

    #[test]
    fn exact_suffix_decision_rejects_a_union_that_needs_too_many_rows() {
        let required = bitset(3, &[0, 1, 2]);
        let rows = vec![
            bitset(3, &[0, 1]),
            bitset(3, &[2]),
            bitset(3, &[2]),
            bitset(3, &[0]),
            bitset(3, &[1]),
        ];
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");

        let page = enumerator.next_page(100, 100).expect("all alternatives");

        assert_eq!(page.optimal_cardinality(), 2);
        assert_eq!(row_vectors(&page), vec![vec![0, 1], vec![0, 2]]);
        assert!(page.impossible_prefix_subtrees_pruned() > 0);
        assert!(page.candidate_combinations_tested() < 10);
    }

    #[test]
    fn batched_self_reduction_matches_bruteforce_for_small_original_row_matrices() {
        const PATTERN_COUNT: usize = 3;
        const ROW_COUNT: usize = 4;
        const ROW_VARIANTS: usize = 1 << PATTERN_COUNT;
        let matrix_count = ROW_VARIANTS.pow(ROW_COUNT as u32);

        for required_mask in 1..ROW_VARIANTS {
            let required = bitset(PATTERN_COUNT, &set_bits(required_mask, PATTERN_COUNT));
            for encoded_rows in 0..matrix_count {
                let mut value = encoded_rows;
                let mut rows = Vec::with_capacity(ROW_COUNT);
                for _ in 0..ROW_COUNT {
                    let row_mask = value % ROW_VARIANTS;
                    value /= ROW_VARIANTS;
                    rows.push(bitset(PATTERN_COUNT, &set_bits(row_mask, PATTERN_COUNT)));
                }

                let expected = brute_force_minimum_portfolios(&required, &rows);
                let actual = ExactMinimumCoverPortfolioEnumerator::new(&required, &rows);
                match expected {
                    None => assert!(matches!(
                        actual,
                        Err(ExactMinimumCoverPortfolioError::RequiredPatternsNotCoverable { .. })
                    )),
                    Some(expected) => {
                        let mut actual = actual.expect("complete matrix enumerator");
                        let page = actual
                            .next_page(64, u64::MAX)
                            .expect("complete matrix page");
                        assert_eq!(row_vectors(&page), expected);
                        assert!(page.enumeration_complete());
                    }
                }
            }
        }
    }

    #[test]
    fn exhaustive_family_order_total_and_every_checkpoint_match_bruteforce() {
        const PATTERN_COUNT: usize = 3;
        const ROW_COUNT: usize = 4;
        const ROW_VARIANTS: usize = 1 << PATTERN_COUNT;
        let required = bitset(PATTERN_COUNT, &[0, 1, 2]);
        let matrix_count = ROW_VARIANTS.pow(ROW_COUNT as u32);
        let mut coverable_cases = 0_usize;
        let mut portfolio_count = 0_usize;
        let mut restart_count = 0_usize;

        for encoded_rows in 0..matrix_count {
            let mut value = encoded_rows;
            let mut rows = Vec::with_capacity(ROW_COUNT);
            for _ in 0..ROW_COUNT {
                let row_mask = value % ROW_VARIANTS;
                value /= ROW_VARIANTS;
                rows.push(bitset(PATTERN_COUNT, &set_bits(row_mask, PATTERN_COUNT)));
            }
            let Some(expected) = brute_force_minimum_portfolios(&required, &rows) else {
                assert!(matches!(
                    ExactMinimumCoverPortfolioEnumerator::new(&required, &rows),
                    Err(ExactMinimumCoverPortfolioError::RequiredPatternsNotCoverable { .. })
                ));
                continue;
            };
            coverable_cases += 1;
            portfolio_count += expected.len();
            let mut enumerator =
                ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");
            let mut actual = Vec::new();
            while !enumerator.enumeration_complete() {
                let page = enumerator
                    .next_page(1, u64::MAX)
                    .expect("single completed portfolio");
                let emitted = row_vectors(&page);
                actual.extend(emitted.iter().cloned());
                assert_eq!(
                    page.known_alternative_count_decimal(),
                    actual.len().to_string()
                );
                if emitted.is_empty() {
                    assert_eq!(page.stop(), ExactMinimumCoverEnumerationStop::Sealed);
                    continue;
                }
                restart_count += 1;
                let (next, count, complete) = page.restart().map_or_else(
                    || {
                        (
                            None,
                            page.known_alternative_count_decimal().to_owned(),
                            page.enumeration_complete(),
                        )
                    },
                    |restart| {
                        (
                            restart.next_combination().map(ToOwned::to_owned),
                            restart.known_alternative_count_decimal(),
                            restart.enumeration_complete(),
                        )
                    },
                );
                let mut resumed = ExactMinimumCoverPortfolioEnumerator::resume_from_fields(
                    &required,
                    &rows,
                    page.optimal_cardinality(),
                    next,
                    &count,
                    complete,
                )
                .expect("every emitted checkpoint resumes");
                let remaining = resumed
                    .next_page(expected.len() + 1, u64::MAX)
                    .expect("checkpoint remainder");
                assert_eq!(
                    row_vectors(&remaining),
                    expected[actual.len()..],
                    "checkpoint suffix mismatch for matrix {encoded_rows}"
                );
                assert!(remaining.enumeration_complete());
            }
            assert_eq!(
                actual, expected,
                "family mismatch for matrix {encoded_rows}"
            );
            assert!(enumerator.enumeration_complete());
            assert!(enumerator.next_combination.is_none());
            assert!(enumerator.restart_state().is_none());
            assert_eq!(
                enumerator.known_alternative_count_decimal(),
                expected.len().to_string()
            );
        }

        assert_eq!(coverable_cases, 3_375);
        assert_eq!(portfolio_count, 5_672);
        assert_eq!(restart_count, portfolio_count);
    }

    #[test]
    fn cancellation_inside_at_most_is_zero_commit_and_retry_exact() {
        let required = bitset(3, &[0, 1, 2]);
        let rows = vec![
            bitset(3, &[0]),
            bitset(3, &[1, 2]),
            bitset(3, &[0]),
            bitset(3, &[]),
        ];
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");
        let first = enumerator.next_page(1, 1).expect("first portfolio");
        assert_eq!(row_vectors(&first), vec![vec![0, 1]]);
        let restart_before = enumerator.restart_state().expect("open frontier");
        let hint_before = enumerator.witness_hint.clone();

        let mut baseline = enumerator.clone();
        let expected = baseline.next_page(1, u64::MAX).expect("unsliced second");
        assert_eq!(row_vectors(&expected), vec![vec![1, 2]]);

        let mut polls = 0_usize;
        let cancelled = enumerator
            .next_page_with_memory_guard_and_control(1, u64::MAX, &mut |_| Ok(()), &mut || {
                polls += 1;
                polls == 7
            })
            .expect("cooperative cancellation");
        assert_eq!(
            cancelled.stop(),
            ExactMinimumCoverEnumerationStop::Cancelled
        );
        assert!(cancelled.portfolios().is_empty());
        assert_eq!(cancelled.work_steps(), 0);
        assert_eq!(cancelled.candidate_combinations_tested(), 0);
        assert_eq!(cancelled.impossible_prefix_subtrees_pruned(), 0);
        assert_eq!(enumerator.restart_state(), Some(restart_before));
        assert_eq!(enumerator.witness_hint, hint_before);

        let retried = enumerator
            .next_page(1, u64::MAX)
            .expect("same oracle retry");
        assert_eq!(row_vectors(&retried), row_vectors(&expected));
        assert_eq!(
            retried.known_alternative_count_decimal(),
            expected.known_alternative_count_decimal()
        );
        assert_eq!(
            retried.enumeration_complete(),
            expected.enumeration_complete()
        );
    }

    #[test]
    fn guarded_page_reports_whole_live_and_rejects_before_state_commit() {
        let required = bitset(3, &[0, 1, 2]);
        let rows = vec![
            bitset(3, &[0]),
            bitset(3, &[0]),
            bitset(3, &[1]),
            bitset(3, &[2]),
        ];
        let enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");
        let enumerator_live = enumerator
            .checked_retained_capacity_bytes()
            .expect("enumerator bytes");
        let mut dry = enumerator.clone();
        let mut observed = Vec::new();
        let page = loop {
            let page = dry
                .next_page_with_memory_guard_and_control(
                    1,
                    u64::MAX,
                    &mut |whole_live| {
                        observed.push(whole_live);
                        Ok(())
                    },
                    &mut || false,
                )
                .expect("guarded cooperative dry slice");
            if !page.portfolios().is_empty() || page.enumeration_complete() {
                break page;
            }
            assert_eq!(
                page.stop(),
                ExactMinimumCoverEnumerationStop::WorkBudgetExhausted
            );
            assert!(page.work_steps() > 0);
        };
        assert_eq!(row_vectors(&page), vec![vec![0, 2, 3]]);
        assert!(
            observed
                .iter()
                .all(|whole_live| *whole_live >= enumerator_live),
            "callback must never report a transient delta"
        );
        let first_transient = observed
            .iter()
            .copied()
            .find(|whole_live| *whole_live > enumerator_live)
            .expect("page/query transient peak");
        assert!(page
            .checked_retained_capacity_bytes()
            .is_some_and(|bytes| bytes != 0));

        let mut rejected = enumerator.clone();
        let restart_before = rejected.restart_state();
        let hint_before = rejected.witness_hint.clone();
        let limit = first_transient - 1;
        let error = rejected
            .next_page_with_memory_guard_and_control(
                1,
                u64::MAX,
                &mut |whole_live| {
                    if whole_live > limit {
                        return Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                            required_memory_bytes: whole_live,
                            max_memory_bytes: limit,
                        });
                    }
                    Ok(())
                },
                &mut || false,
            )
            .expect_err("capacity minus one must fail closed");
        assert!(matches!(
            error,
            ExactMinimumCoverPortfolioError::MinimumCover(
                ExactMinimumCoverError::MemoryCapacityExceeded {
                    required_memory_bytes,
                    max_memory_bytes,
                }
            ) if required_memory_bytes > max_memory_bytes
        ));
        assert_eq!(rejected.restart_state(), restart_before);
        assert_eq!(rejected.witness_hint, hint_before);
    }

    #[test]
    fn restart_fields_remain_sufficient_across_batched_suffix_proofs() {
        let required = bitset(3, &[0, 1, 2]);
        let rows = vec![
            bitset(3, &[0]),
            bitset(3, &[0]),
            bitset(3, &[1]),
            bitset(3, &[1]),
            bitset(3, &[2]),
            bitset(3, &[2]),
            bitset(3, &[2]),
        ];
        let expected = brute_force_minimum_portfolios(&required, &rows).expect("complete cover");
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");
        let mut actual = Vec::new();
        let mut saw_prune = false;

        while !enumerator.enumeration_complete() {
            let page = enumerator
                .next_page(1, u64::MAX)
                .expect("one completed semantic frontier decision");
            actual.extend(row_vectors(&page));
            saw_prune |= page.impossible_prefix_subtrees_pruned() != 0;
            let Some(restart) = page.restart() else {
                break;
            };
            enumerator = ExactMinimumCoverPortfolioEnumerator::resume_from_fields(
                &required,
                &rows,
                restart.optimal_cardinality(),
                restart.next_combination().map(ToOwned::to_owned),
                &restart.known_alternative_count_decimal(),
                restart.enumeration_complete(),
            )
            .expect("fieldwise resume after bounded step");
        }

        assert!(saw_prune);
        assert_eq!(actual, expected);
    }

    #[test]
    fn one_node_slices_resume_the_same_oracle_and_match_blocking_enumeration() {
        let required = bitset(4, &[0, 1, 2, 3]);
        let rows = vec![
            bitset(4, &[0, 1]),
            bitset(4, &[0]),
            bitset(4, &[1, 2]),
            bitset(4, &[2]),
            bitset(4, &[2, 3]),
            bitset(4, &[3]),
            bitset(4, &[0, 3]),
        ];
        let mut blocking =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("blocking");
        let expected = row_vectors(
            &blocking
                .next_page(100, u64::MAX)
                .expect("blocking enumeration"),
        );

        let mut sliced =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("sliced");
        let mut actual = Vec::new();
        let mut saw_pending = false;
        while !sliced.enumeration_complete() {
            let page = sliced.next_page(100, 1).expect("one-node slice");
            assert!(page.work_steps() <= 1);
            saw_pending |= page.stop() == ExactMinimumCoverEnumerationStop::WorkBudgetExhausted
                && page.portfolios().is_empty();
            actual.extend(row_vectors(&page));
        }

        assert!(saw_pending, "the fixture must exercise an in-flight oracle");
        assert_eq!(actual, expected);
    }

    #[test]
    fn preparation_session_zero_budget_is_no_progress_and_slices_match_blocking() {
        let required = bitset(4, &[0, 1, 2, 3]);
        let rows = vec![
            bitset(4, &[0, 1]),
            bitset(4, &[1, 2]),
            bitset(4, &[2, 3]),
            bitset(4, &[0, 3]),
            bitset(4, &[0]),
            bitset(4, &[3]),
        ];
        let blocking = ExactMinimumCoverPortfolioEnumerator::new(&required, &rows)
            .expect("blocking preparation");
        let mut session = ExactMinimumCoverPortfolioPreparationSession::new(&required, &rows)
            .expect("owned preparation");
        assert!(matches!(
            session.advance(0).expect("zero work"),
            ExactMinimumCoverPortfolioPreparationAdvance::Pending { visited_nodes: 0 }
        ));

        let sliced = loop {
            match session.advance(1).expect("one-node proof slice") {
                ExactMinimumCoverPortfolioPreparationAdvance::Pending { visited_nodes } => {
                    assert!(visited_nodes <= 1);
                }
                ExactMinimumCoverPortfolioPreparationAdvance::Coverable {
                    enumerator,
                    visited_nodes,
                    ..
                } => {
                    assert!(visited_nodes <= 1);
                    break enumerator;
                }
                other => panic!("unexpected preparation terminal: {other:?}"),
            }
        };
        assert_eq!(sliced.optimal_cardinality(), blocking.optimal_cardinality());
    }

    #[test]
    fn work_budget_and_cancellation_return_exact_restart_state() {
        let required = bitset(2, &[0, 1]);
        let rows = vec![bitset(2, &[0]), bitset(2, &[0]), bitset(2, &[1])];
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");

        let budgeted = enumerator.next_page(2, 0).expect("budgeted page");
        assert!(budgeted.portfolios().is_empty());
        assert_eq!(
            budgeted.stop(),
            ExactMinimumCoverEnumerationStop::WorkBudgetExhausted
        );
        let restart = budgeted.restart().expect("restart").clone();

        let mut resumed = ExactMinimumCoverPortfolioEnumerator::resume(&required, &rows, restart)
            .expect("valid restart");
        let cancelled = resumed
            .next_page_with_control(2, 10, &mut || true)
            .expect("cancelled page");
        assert_eq!(
            cancelled.stop(),
            ExactMinimumCoverEnumerationStop::Cancelled
        );
        assert!(cancelled.portfolios().is_empty());

        let completed = resumed
            .next_page(10, u64::MAX)
            .expect("resumed blocking page");
        assert_eq!(row_vectors(&completed), vec![vec![0, 2], vec![1, 2]]);
        assert_eq!(completed.total_alternative_count_decimal(), Some("2"));
    }

    #[test]
    fn empty_required_set_has_one_empty_exact_portfolio() {
        let required = bitset(2, &[]);
        let rows = vec![bitset(2, &[0]), bitset(2, &[1])];
        let mut enumerator =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");

        let page = enumerator.next_page(1, 1).expect("empty cover page");

        assert_eq!(row_vectors(&page), vec![Vec::<usize>::new()]);
        assert_eq!(page.total_alternative_count_decimal(), Some("1"));
    }

    #[test]
    fn incomplete_cover_is_rejected_instead_of_claiming_all_alternatives() {
        let required = bitset(2, &[0, 1]);
        let rows = vec![bitset(2, &[0])];

        assert!(matches!(
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows),
            Err(
                ExactMinimumCoverPortfolioError::RequiredPatternsNotCoverable {
                    covered_pattern_count: 1,
                    required_pattern_count: 2,
                }
            )
        ));
    }

    #[test]
    fn restart_is_fieldwise_bound_to_the_original_required_and_rows() {
        let required = bitset(2, &[0, 1]);
        let rows = vec![bitset(2, &[0]), bitset(2, &[1]), bitset(2, &[0, 1])];
        let mut original =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");
        let checkpoint = original
            .next_page(1, 0)
            .expect("checkpoint page")
            .restart()
            .expect("restart")
            .clone();
        let changed_rows = vec![bitset(2, &[1]), bitset(2, &[0]), bitset(2, &[0, 1])];

        assert!(matches!(
            ExactMinimumCoverPortfolioEnumerator::resume(&required, &changed_rows, checkpoint),
            Err(ExactMinimumCoverPortfolioError::InvalidRestart)
        ));
    }

    #[test]
    fn persistence_fields_resume_without_serializing_private_input_owners() {
        let required = bitset(1, &[0]);
        let rows = vec![bitset(1, &[0]), bitset(1, &[0])];
        let mut original =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("enumerator");
        let first = original.next_page(1, 1).expect("first page");
        let restart = first.restart().expect("restart");

        let mut resumed = ExactMinimumCoverPortfolioEnumerator::resume_from_fields(
            &required,
            &rows,
            restart.optimal_cardinality(),
            restart.next_combination().map(ToOwned::to_owned),
            &restart.known_alternative_count_decimal(),
            restart.enumeration_complete(),
        )
        .expect("fieldwise resume");
        let second = resumed.next_page(1, 1).expect("second page");

        assert_eq!(row_vectors(&second), vec![vec![1]]);
        assert_eq!(second.total_alternative_count_decimal(), Some("2"));
    }

    #[test]
    fn proven_input_resume_reuses_authority_and_matches_independent_reproof() {
        let required = bitset(2, &[0, 1]);
        let rows = vec![
            bitset(2, &[0]),
            bitset(2, &[0]),
            bitset(2, &[1]),
            bitset(2, &[1]),
        ];
        let mut authority =
            ExactMinimumCoverPortfolioEnumerator::new(&required, &rows).expect("authority");
        let first = authority.next_page(1, u64::MAX).expect("canonical page");
        let restart = first.restart().expect("open restart");
        let next = restart.next_combination().map(ToOwned::to_owned);
        let count = restart.known_alternative_count_decimal();

        let mut proven = authority
            .resume_from_proven_fields(
                restart.optimal_cardinality(),
                next.clone(),
                &count,
                restart.enumeration_complete(),
            )
            .expect("proof-bound resume");
        let mut reproved = ExactMinimumCoverPortfolioEnumerator::resume_from_fields(
            &required,
            &rows,
            restart.optimal_cardinality(),
            next,
            &count,
            restart.enumeration_complete(),
        )
        .expect("independent reproof resume");

        let proven_page = proven.next_page(10, u64::MAX).expect("proven suffix");
        let reproved_page = reproved.next_page(10, u64::MAX).expect("reproved suffix");
        assert_eq!(row_vectors(&proven_page), row_vectors(&reproved_page));
        assert_eq!(
            proven_page.total_alternative_count_decimal(),
            reproved_page.total_alternative_count_decimal()
        );
        assert!(matches!(
            authority.resume_from_proven_fields(
                restart.optimal_cardinality() + 1,
                restart.next_combination().map(ToOwned::to_owned),
                &count,
                false,
            ),
            Err(ExactMinimumCoverPortfolioError::InvalidRestart)
        ));
    }

    #[test]
    fn persistence_fields_reject_noncanonical_or_unbound_state() {
        let required = bitset(1, &[0]);
        let rows = vec![bitset(1, &[0]), bitset(1, &[0])];

        for result in [
            ExactMinimumCoverPortfolioEnumerator::resume_from_fields(
                &required,
                &rows,
                2,
                Some(vec![1]),
                "1",
                false,
            ),
            ExactMinimumCoverPortfolioEnumerator::resume_from_fields(
                &required,
                &rows,
                1,
                Some(vec![1]),
                "01",
                false,
            ),
            ExactMinimumCoverPortfolioEnumerator::resume_from_fields(
                &required,
                &rows,
                1,
                Some(vec![1]),
                "999",
                false,
            ),
        ] {
            assert!(matches!(
                result,
                Err(ExactMinimumCoverPortfolioError::InvalidRestart)
            ));
        }
    }

    #[test]
    fn progressive_count_has_no_fixed_width_integer_ceiling() {
        let mut count = DecimalCounter {
            digits: vec![9; 40],
        };

        count.increment().expect("grow decimal counter");

        assert_eq!(count.to_decimal_string(), format!("1{}", "0".repeat(40)));
    }

    fn combination_covers_for_test(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
        combination: &[usize],
    ) -> bool {
        (0..required.word_count()).all(|word_index| {
            let covered = combination.iter().fold(0_u64, |covered, row_index| {
                covered | rows[*row_index].word_at(word_index)
            });
            covered & required.word_at(word_index) == required.word_at(word_index)
        })
    }

    fn set_bits(mask: usize, pattern_count: usize) -> Vec<usize> {
        (0..pattern_count)
            .filter(|pattern| mask & (1 << pattern) != 0)
            .collect()
    }

    fn brute_force_minimum_portfolios(
        required: &PatternBitSet,
        rows: &[PatternBitSet],
    ) -> Option<Vec<Vec<usize>>> {
        for cardinality in 0..=rows.len() {
            let Some(mut combination) = first_combination(rows.len(), cardinality) else {
                continue;
            };
            let mut portfolios = Vec::new();
            loop {
                if combination_covers_for_test(required, rows, &combination) {
                    portfolios.push(combination.clone());
                }
                if !advance_combination(&mut combination, rows.len()) {
                    break;
                }
            }
            if !portfolios.is_empty() {
                return Some(portfolios);
            }
        }
        None
    }
}
