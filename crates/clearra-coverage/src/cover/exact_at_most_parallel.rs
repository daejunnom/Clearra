// SRP rationale: one change reason is the portable exact-AtMost partition proof
// protocol. Query/task identities, exhaustive partition obligations, shard
// row remapping, receipt validation and their memory admission form one
// correctness boundary: only a replayed witness or closed negative obligations
// may decide the query. The scalar solver owns search, while host schedulers
// and transports relay tasks and receipts without acquiring proof authority.
//! Portable, exhaustive partitions of an exact AtMost query. Scheduling and
//! transport do not own proof authority: a missing, stale or cancelled shard is
//! never evidence of infeasibility. Original row identities survive reductions
//! inside each shard, including canonical queries with synthetic constraints.
use std::sync::Arc;

use crate::pattern::pattern_bitset::PatternBitSet;

use super::exact_minimum_cover::{
    ExactCoverSearchSession, ExactMinimumCoverError, ExactMinimumCoverSessionAdvance,
};

#[path = "exact_at_most_assistance.rs"]
mod assistance;

/// `matrix_id` is the caller's existing canonical SHA-256 matrix binding. The
/// transport must validate that binding when loading a worker's shared matrix.
/// Neither this identity nor an AtMost answer is a minimum-cardinality proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactAtMostQueryIdentity {
    pub matrix_id: [u8; 32],
    pub generation: u64,
    pub query_id: u64,
}

#[derive(Clone, Debug)]
pub struct ExactAtMostQuery(Arc<QueryData>);

#[derive(Debug)]
struct QueryData {
    identity: ExactAtMostQueryIdentity,
    required: PatternBitSet,
    rows: Vec<PatternBitSet>,
    limit: usize,
    // Search-order advice only. It may deliberately miss the query-local
    // selector; only independently replayed shard witnesses grant authority.
    witness_hint: Option<Vec<usize>>,
    retained_bytes: u128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExactAtMostParallelError {
    Exact(ExactMinimumCoverError),
    InvalidTask,
    StaleQuery,
    UnknownPartition,
    DuplicateReceipt,
    InvalidWitness,
    ContradictoryReceipt,
    Cancelled,
    Finished,
}

impl From<ExactMinimumCoverError> for ExactAtMostParallelError {
    fn from(value: ExactMinimumCoverError) -> Self {
        Self::Exact(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactAtMostTask {
    identity: ExactAtMostQueryIdentity,
    partition_id: u64,
    forced_rows: Vec<usize>,
    excluded_rows: Vec<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExactAtMostShardOutcome {
    Found(Vec<usize>),
    ProvedNone,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactAtMostReceipt {
    task: ExactAtMostTask,
    outcome: ExactAtMostShardOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExactAtMostParallelDecision {
    Pending { remaining: usize },
    Found(Vec<usize>),
    ProvedNone,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExactAtMostShardAdvance {
    Pending { work_steps: u64 },
    Terminal(ExactAtMostReceipt),
}

#[derive(Clone, Debug)]
pub struct ExactAtMostCoordinator {
    query: ExactAtMostQuery,
    tasks: Vec<ExactAtMostTask>,
    received: Vec<bool>,
    decision: ExactAtMostParallelDecision,
    assistance: Option<assistance::AssistanceState>,
}

#[derive(Debug)]
pub struct ExactAtMostShardSession {
    query: ExactAtMostQuery,
    task: ExactAtMostTask,
    row_map: Vec<usize>,
    search: Option<ExactCoverSearchSession>,
    ready: Option<ExactAtMostShardOutcome>,
    finished: bool,
}

fn overflow() -> ExactMinimumCoverError {
    ExactMinimumCoverError::ProjectionOverflow
}

fn bytes<T>(items: usize) -> Result<u128, ExactMinimumCoverError> {
    (items as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or_else(overflow)
}

fn reserve<T>(items: usize) -> Result<Vec<T>, ExactMinimumCoverError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(items)
        .map_err(|_| ExactMinimumCoverError::AllocationFailed {
            component: "parallel exact AtMost",
        })?;
    Ok(values)
}

fn sorted_unique(values: &[usize]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

impl ExactAtMostQuery {
    /// Inline query and Arc control owner allocated when already-admitted
    /// matrix vectors move into their immutable shared query. Excludes all
    /// caller-owned vector/bitset payloads, which the decoder counts separately.
    pub fn checked_constructor_owner_bytes() -> Option<u128> {
        (core::mem::size_of::<QueryData>() as u128)
            .checked_add((core::mem::size_of::<usize>() as u128).checked_mul(2)?)
    }

    pub fn new(
        identity: ExactAtMostQueryIdentity,
        required: PatternBitSet,
        rows: Vec<PatternBitSet>,
        limit: usize,
    ) -> Result<Self, ExactAtMostParallelError> {
        Self::new_with_witness_hint(identity, required, rows, limit, None)
    }

    pub fn new_with_witness_hint(
        identity: ExactAtMostQueryIdentity,
        required: PatternBitSet,
        rows: Vec<PatternBitSet>,
        limit: usize,
        witness_hint: Option<Vec<usize>>,
    ) -> Result<Self, ExactAtMostParallelError> {
        if witness_hint
            .as_ref()
            .is_some_and(|hint| !sorted_unique(hint) || hint.iter().any(|&row| row >= rows.len()))
        {
            return Err(ExactAtMostParallelError::InvalidWitness);
        }
        for (row_index, row) in rows.iter().enumerate() {
            if row.pattern_count() != required.pattern_count() {
                return Err(ExactMinimumCoverError::RowPatternCountMismatch {
                    row_index,
                    expected: required.pattern_count(),
                    actual: row.pattern_count(),
                }
                .into());
            }
        }
        let mut retained_bytes = Self::checked_constructor_owner_bytes()
            .and_then(|retained| {
                retained.checked_add(bytes::<PatternBitSet>(rows.capacity()).ok()?)
            })
            .ok_or_else(overflow)?
            .checked_add(
                required
                    .checked_storage_retained_bytes()
                    .ok_or_else(overflow)?,
            )
            .ok_or_else(overflow)?;
        for row in &rows {
            retained_bytes = retained_bytes
                .checked_add(row.checked_storage_retained_bytes().ok_or_else(overflow)?)
                .ok_or_else(overflow)?;
        }
        retained_bytes = retained_bytes
            .checked_add(bytes::<usize>(
                witness_hint.as_ref().map_or(0, Vec::capacity),
            )?)
            .ok_or_else(overflow)?;
        Ok(Self(Arc::new(QueryData {
            identity,
            required,
            rows,
            limit,
            witness_hint,
            retained_bytes,
        })))
    }

    pub fn identity(&self) -> ExactAtMostQueryIdentity {
        self.0.identity
    }
    pub fn required(&self) -> &PatternBitSet {
        &self.0.required
    }
    pub fn rows(&self) -> &[PatternBitSet] {
        &self.0.rows
    }
    pub fn limit(&self) -> usize {
        self.0.limit
    }

    pub fn witness_hint(&self) -> Option<&[usize]> {
        self.0.witness_hint.as_deref()
    }

    pub fn checked_retained_bytes(&self) -> Option<u128> {
        // Query inputs are immutable. Do not re-scan every original row at
        // each tiny cooperative DFS slice merely to account the same graph.
        Some(self.0.retained_bytes)
    }

    fn validate_task(&self, task: &ExactAtMostTask) -> Result<(), ExactAtMostParallelError> {
        if self.identity() != task.identity {
            return Err(ExactAtMostParallelError::StaleQuery);
        }
        if task.forced_rows.len() > self.limit()
            || task
                .forced_rows
                .iter()
                .chain(&task.excluded_rows)
                .any(|&row| row >= self.rows().len())
        {
            return Err(ExactAtMostParallelError::InvalidTask);
        }
        Ok(())
    }

    fn validate_witness(
        &self,
        task: &ExactAtMostTask,
        rows: &[usize],
    ) -> Result<(), ExactAtMostParallelError> {
        if rows.len() > self.limit()
            || !sorted_unique(rows)
            || rows.iter().any(|&row| {
                row >= self.rows().len() || task.excluded_rows.binary_search(&row).is_ok()
            })
            || task
                .forced_rows
                .iter()
                .any(|row| rows.binary_search(row).is_err())
        {
            return Err(ExactAtMostParallelError::InvalidWitness);
        }
        for word in 0..self.required().word_count() {
            let covered = rows
                .iter()
                .fold(0, |covered, &row| covered | self.rows()[row].word_at(word));
            if covered & self.required().word_at(word) != self.required().word_at(word) {
                return Err(ExactAtMostParallelError::InvalidWitness);
            }
        }
        Ok(())
    }
}

impl ExactAtMostTask {
    /// Transport shape validation. The authoritative coordinator additionally
    /// checks the complete descriptor against its privately issued frontier.
    pub fn from_parts(
        identity: ExactAtMostQueryIdentity,
        partition_id: u64,
        forced_rows: Vec<usize>,
        excluded_rows: Vec<usize>,
    ) -> Result<Self, ExactAtMostParallelError> {
        if !sorted_unique(&forced_rows)
            || !sorted_unique(&excluded_rows)
            || forced_rows
                .iter()
                .any(|row| excluded_rows.binary_search(row).is_ok())
        {
            return Err(ExactAtMostParallelError::InvalidTask);
        }
        Ok(Self {
            identity,
            partition_id,
            forced_rows,
            excluded_rows,
        })
    }
    pub fn identity(&self) -> ExactAtMostQueryIdentity {
        self.identity
    }
    pub fn partition_id(&self) -> u64 {
        self.partition_id
    }
    pub fn forced_rows(&self) -> &[usize] {
        &self.forced_rows
    }
    pub fn excluded_rows(&self) -> &[usize] {
        &self.excluded_rows
    }
    pub fn checked_retained_bytes(&self) -> Option<u128> {
        bytes::<usize>(self.forced_rows.capacity())
            .ok()?
            .checked_add(bytes::<usize>(self.excluded_rows.capacity()).ok()?)
    }
}

impl ExactAtMostReceipt {
    /// Decode only from the trusted exact worker channel. A negative answer is
    /// an exact-worker claim, not a certificate supplied by a GUI/user input.
    /// Accepted positives are independently replayed by the coordinator.
    pub fn from_parts(
        task: ExactAtMostTask,
        outcome: ExactAtMostShardOutcome,
    ) -> Result<Self, ExactAtMostParallelError> {
        if matches!(&outcome, ExactAtMostShardOutcome::Found(rows) if !sorted_unique(rows)) {
            return Err(ExactAtMostParallelError::InvalidWitness);
        }
        Ok(Self { task, outcome })
    }
    pub fn task(&self) -> &ExactAtMostTask {
        &self.task
    }
    pub fn outcome(&self) -> &ExactAtMostShardOutcome {
        &self.outcome
    }
}

/// Rarest uncovered pivot, using the ORIGINAL matrix. A cover belongs to exactly
/// one child: its first selected supporter of this pivot. Dominance reduction
/// therefore cannot remove a canonical portfolio before partitioning it.
fn supporters(
    query: &ExactAtMostQuery,
    task: &ExactAtMostTask,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Option<Vec<usize>>, ExactAtMostParallelError> {
    let mut best: Option<Vec<usize>> = None;
    for pattern in query
        .required()
        .covered_patterns_before(query.required().pattern_count())
    {
        if cancelled() {
            return Err(ExactAtMostParallelError::Cancelled);
        }
        if task
            .forced_rows
            .iter()
            .any(|&row| query.rows()[row].contains(pattern))
        {
            continue;
        }
        let count = query
            .rows()
            .iter()
            .enumerate()
            .filter(|(row, bits)| {
                bits.contains(pattern) && task.excluded_rows.binary_search(row).is_err()
            })
            .count();
        if best.as_ref().is_some_and(|best| best.len() <= count) {
            continue;
        }
        let mut next = reserve(count)?;
        next.extend(query.rows().iter().enumerate().filter_map(|(row, bits)| {
            (bits.contains(pattern) && task.excluded_rows.binary_search(&row).is_err())
                .then_some(row)
        }));
        best = Some(next);
        if count <= 1 {
            break;
        }
    }
    Ok(best)
}

impl ExactAtMostCoordinator {
    /// Positive-only global repair is not an additional negative-proof
    /// partition. Replay it through its unique cube owner before accepting.
    pub(super) fn accept_warm_witness(
        &mut self,
        mut rows: Vec<usize>,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<(), ExactAtMostParallelError> {
        if !matches!(self.decision, ExactAtMostParallelDecision::Pending { .. }) {
            return Err(ExactAtMostParallelError::Finished);
        }
        rows.sort_unstable();
        let index = self
            .tasks
            .iter()
            .position(|task| {
                task.forced_rows
                    .iter()
                    .all(|row| rows.binary_search(row).is_ok())
                    && task
                        .excluded_rows
                        .iter()
                        .all(|row| rows.binary_search(row).is_err())
            })
            .ok_or(ExactAtMostParallelError::InvalidWitness)?;
        if self.received[index] {
            return Err(ExactAtMostParallelError::DuplicateReceipt);
        }
        self.query.validate_witness(&self.tasks[index], &rows)?;
        memory_guard(
            self.checked_retained_bytes()
                .and_then(|retained| retained.checked_add(bytes::<usize>(rows.capacity()).ok()?))
                .ok_or_else(overflow)?,
        )?;
        // Move the already materialized witness into its cube owner. Do not
        // clone the descriptor or the returned decision just to discard them.
        self.decision = ExactAtMostParallelDecision::Found(rows);
        self.received[index] = true;
        Ok(())
    }

    pub fn prepare(
        query: ExactAtMostQuery,
        target_partitions: usize,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<Self, ExactAtMostParallelError> {
        let query_bytes = query.checked_retained_bytes().ok_or_else(overflow)?;
        memory_guard(query_bytes)?;
        let mut tasks = reserve(1)?;
        tasks.push(ExactAtMostTask::from_parts(
            query.identity(),
            0,
            vec![],
            vec![],
        )?);
        let mut cursor = 0;
        // The target controls queue depth, never the number of admissible
        // alternatives. Splitting a pivot always emits ALL its children.
        while tasks.len() < target_partitions.max(1) && cursor < tasks.len() {
            if cancelled() {
                return Err(ExactAtMostParallelError::Cancelled);
            }
            if tasks[cursor].forced_rows.len() >= query.limit() {
                cursor += 1;
                continue;
            }
            memory_guard(
                query_bytes
                    .checked_add(
                        bytes::<usize>(query.rows().len())?
                            .checked_mul(2)
                            .ok_or_else(overflow)?,
                    )
                    .and_then(|n| n.checked_add(frontier_bytes(&tasks).ok()?))
                    .ok_or_else(overflow)?,
            )?;
            let Some(rows) = supporters(&query, &tasks[cursor], cancelled)? else {
                cursor += 1;
                continue;
            };
            if rows.is_empty() {
                cursor += 1;
                continue;
            }
            let parent = &tasks[cursor];
            let projected_indices = rows
                .len()
                .checked_mul(parent.forced_rows.len() + 1 + parent.excluded_rows.len() + rows.len())
                .ok_or_else(overflow)?;
            memory_guard(
                query_bytes
                    .checked_add(frontier_bytes(&tasks)?)
                    .and_then(|n| n.checked_add(bytes::<usize>(projected_indices).ok()?))
                    .and_then(|n| {
                        n.checked_add(bytes::<ExactAtMostTask>(rows.len()).ok()?.checked_mul(2)?)
                    })
                    .and_then(|n| n.checked_add(bytes::<usize>(rows.capacity()).ok()?))
                    .ok_or_else(overflow)?,
            )?;
            let mut children = reserve(rows.len())?;
            for (index, &selected) in rows.iter().enumerate() {
                let mut forced = reserve(parent.forced_rows.len() + 1)?;
                forced.extend_from_slice(&parent.forced_rows);
                forced.push(selected);
                forced.sort_unstable();
                let mut excluded = reserve(parent.excluded_rows.len() + index)?;
                excluded.extend_from_slice(&parent.excluded_rows);
                excluded.extend_from_slice(&rows[..index]);
                excluded.sort_unstable();
                children.push(ExactAtMostTask::from_parts(
                    query.identity(),
                    0,
                    forced,
                    excluded,
                )?);
            }
            tasks.try_reserve_exact(children.len()).map_err(|_| {
                ExactMinimumCoverError::AllocationFailed {
                    component: "parallel AtMost frontier",
                }
            })?;
            memory_guard(
                query_bytes
                    .checked_add(frontier_bytes(&tasks)?)
                    .and_then(|n| n.checked_add(frontier_bytes(&children).ok()?))
                    .and_then(|n| n.checked_add(bytes::<usize>(rows.capacity()).ok()?))
                    .ok_or_else(overflow)?,
            )?;
            tasks.remove(cursor);
            tasks.extend(children);
        }
        for (index, task) in tasks.iter_mut().enumerate() {
            task.partition_id = u64::try_from(index).map_err(|_| overflow())?;
        }
        memory_guard(
            query_bytes
                .checked_add(frontier_bytes(&tasks)?)
                .and_then(|n| n.checked_add(bytes::<bool>(tasks.len()).ok()?))
                .ok_or_else(overflow)?,
        )?;
        let mut received = reserve(tasks.len())?;
        received.resize(tasks.len(), false);
        let remaining = tasks.len();
        Ok(Self {
            query,
            tasks,
            received,
            decision: ExactAtMostParallelDecision::Pending { remaining },
            assistance: None,
        })
    }

    pub fn query(&self) -> &ExactAtMostQuery {
        &self.query
    }
    pub fn tasks(&self) -> &[ExactAtMostTask] {
        &self.tasks
    }
    pub fn decision(&self) -> &ExactAtMostParallelDecision {
        &self.decision
    }

    pub fn issued_prefix_complete(&self, issued: usize) -> bool {
        self.received
            .get(..issued)
            .is_some_and(|received| received.iter().all(|seen| *seen))
    }

    pub fn checked_retained_bytes(&self) -> Option<u128> {
        self.query
            .checked_retained_bytes()?
            .checked_add(frontier_bytes(&self.tasks).ok()?)?
            .checked_add(bytes::<bool>(self.received.capacity()).ok()?)?
            .checked_add(match &self.decision {
                ExactAtMostParallelDecision::Found(rows) => bytes::<usize>(rows.capacity()).ok()?,
                _ => 0,
            })?
            .checked_add(
                self.assistance
                    .as_ref()
                    .map_or(Some(0), assistance::AssistanceState::checked_retained_bytes)?,
            )
    }

    pub fn accept(
        &mut self,
        receipt: ExactAtMostReceipt,
    ) -> Result<ExactAtMostParallelDecision, ExactAtMostParallelError> {
        if receipt.task.identity != self.query.identity() {
            return Err(ExactAtMostParallelError::StaleQuery);
        }
        let index = usize::try_from(receipt.task.partition_id)
            .map_err(|_| ExactAtMostParallelError::UnknownPartition)?;
        if self.tasks.get(index) != Some(&receipt.task) {
            return Err(ExactAtMostParallelError::UnknownPartition);
        }
        if self
            .assistance
            .as_ref()
            .is_some_and(|state| state.retired[index])
        {
            return Err(ExactAtMostParallelError::UnknownPartition);
        }
        if self.received[index] {
            return Err(ExactAtMostParallelError::DuplicateReceipt);
        }
        if let Some(state) = &self.assistance {
            if state.roots[state.task_roots[index]].closed {
                if let ExactAtMostShardOutcome::Found(rows) = &receipt.outcome {
                    self.query.validate_witness(&receipt.task, rows)?;
                    return Err(ExactAtMostParallelError::ContradictoryReceipt);
                }
                // The root was already proved negative by its original worker
                // or every child. A first late receipt only drains transport.
                self.received[index] = true;
                return Ok(self.decision.clone());
            }
        }
        if matches!(self.decision, ExactAtMostParallelDecision::Found(_)) {
            if let ExactAtMostShardOutcome::Found(rows) = &receipt.outcome {
                self.query.validate_witness(&receipt.task, rows)?;
            }
            self.received[index] = true;
            return Ok(self.decision.clone());
        }
        if !matches!(self.decision, ExactAtMostParallelDecision::Pending { .. }) {
            return Err(ExactAtMostParallelError::Finished);
        }
        match receipt.outcome {
            ExactAtMostShardOutcome::Found(rows) => {
                self.query.validate_witness(&receipt.task, &rows)?;
                self.decision = ExactAtMostParallelDecision::Found(rows);
            }
            ExactAtMostShardOutcome::ProvedNone => {
                self.received[index] = true;
                let remaining = if let Some(state) = &mut self.assistance {
                    let root_index = state.task_roots[index];
                    let root = &mut state.roots[root_index];
                    if index == root_index
                        || root.children.as_ref().is_some_and(|children| {
                            children.clone().all(|child| self.received[child])
                        })
                    {
                        root.closed = true;
                    }
                    state.roots.iter().filter(|root| !root.closed).count()
                } else {
                    self.received.iter().filter(|&&seen| !seen).count()
                };
                self.decision = if remaining == 0 {
                    ExactAtMostParallelDecision::ProvedNone
                } else {
                    ExactAtMostParallelDecision::Pending { remaining }
                };
            }
            ExactAtMostShardOutcome::Cancelled => {
                self.decision = ExactAtMostParallelDecision::Cancelled
            }
        }
        self.received[index] = true;
        Ok(self.decision.clone())
    }
}

fn frontier_bytes(tasks: &Vec<ExactAtMostTask>) -> Result<u128, ExactMinimumCoverError> {
    let mut total = bytes::<ExactAtMostTask>(tasks.capacity())?;
    for task in tasks {
        total = total
            .checked_add(task.checked_retained_bytes().ok_or_else(overflow)?)
            .ok_or_else(overflow)?;
    }
    Ok(total)
}

impl ExactAtMostShardSession {
    pub fn prepare(
        query: ExactAtMostQuery,
        task: ExactAtMostTask,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<Self, ExactAtMostParallelError> {
        query.validate_task(&task)?;
        let base = query
            .checked_retained_bytes()
            .ok_or_else(overflow)?
            .checked_add(task.checked_retained_bytes().ok_or_else(overflow)?)
            .ok_or_else(overflow)?;
        let mut this = Self {
            query,
            task,
            row_map: vec![],
            search: None,
            ready: None,
            finished: false,
        };
        if cancelled() {
            this.ready = Some(ExactAtMostShardOutcome::Cancelled);
            return Ok(this);
        }
        memory_guard(
            base.checked_add(
                bytes::<u64>(this.query.required().word_count())?
                    .checked_mul(2)
                    .ok_or_else(overflow)?,
            )
            .ok_or_else(overflow)?,
        )?;
        let mut words = reserve(this.query.required().word_count())?;
        for word in 0..this.query.required().word_count() {
            let covered = this.task.forced_rows.iter().fold(0, |covered, &row| {
                covered | this.query.rows()[row].word_at(word)
            });
            words.push(this.query.required().word_at(word) & !covered);
        }
        let required = PatternBitSet::from_words(this.query.required().pattern_count(), words)
            .map_err(|_| ExactAtMostParallelError::InvalidTask)?;
        if required.is_empty() {
            // A fully forced cube still allocates its receipt witness. Its
            // size is unrelated to residual word count, and the residual
            // bitset remains live while that independent Vec is constructed.
            let live = base
                .checked_add(
                    required
                        .checked_storage_retained_bytes()
                        .ok_or_else(overflow)?,
                )
                .ok_or_else(overflow)?;
            memory_guard(
                live.checked_add(bytes::<usize>(this.task.forced_rows.len())?)
                    .ok_or_else(overflow)?,
            )?;
            let mut witness = reserve(this.task.forced_rows.len())?;
            memory_guard(
                live.checked_add(bytes::<usize>(witness.capacity())?)
                    .ok_or_else(overflow)?,
            )?;
            witness.extend_from_slice(&this.task.forced_rows);
            this.ready = Some(ExactAtMostShardOutcome::Found(witness));
            return Ok(this);
        }
        let remaining = this.query.limit() - this.task.forced_rows.len();
        if remaining == 0 {
            this.ready = Some(ExactAtMostShardOutcome::ProvedNone);
            return Ok(this);
        }
        memory_guard(
            base.checked_add(bytes::<PatternBitSet>(this.query.rows().len())?)
                .and_then(|n| n.checked_add(bytes::<usize>(this.query.rows().len()).ok()?))
                .ok_or_else(overflow)?,
        )?;
        let mut rows = reserve(this.query.rows().len())?;
        this.row_map = reserve(this.query.rows().len())?;
        for (index, row) in this.query.rows().iter().enumerate() {
            if this.task.forced_rows.binary_search(&index).is_err()
                && this.task.excluded_rows.binary_search(&index).is_err()
            {
                rows.push(row.clone());
                this.row_map.push(index);
            }
        }
        let retained = base
            .checked_add(bytes::<usize>(this.row_map.capacity())?)
            .ok_or_else(overflow)?;
        let mut local_hint = None;
        if let Some(hint) = this.query.witness_hint() {
            memory_guard(
                retained
                    .checked_add(bytes::<usize>(hint.len())?)
                    .ok_or_else(overflow)?,
            )?;
            let mut mapped = reserve(hint.len())?;
            for row in hint {
                if let Ok(local) = this.row_map.binary_search(row) {
                    mapped.push(local);
                }
            }
            // A cube may force a row absent from the warm portfolio. The
            // remaining hint is then one row too large, useful as a replayed
            // breakout seed. More distant hints do not justify heuristic work.
            if mapped.len() == remaining || mapped.len() == remaining.saturating_add(1) {
                local_hint = Some(mapped);
            }
        }
        let preparing = retained
            .checked_add(bytes::<usize>(
                local_hint.as_ref().map_or(0, Vec::capacity),
            )?)
            .and_then(|owned| owned.checked_add(bytes::<PatternBitSet>(rows.capacity()).ok()?))
            .and_then(|owned| owned.checked_add(required.checked_storage_retained_bytes()?))
            .ok_or_else(overflow)?;
        this.search = Some(
            ExactCoverSearchSession::prepare_at_most_with_memory_guard_and_control(
                &required,
                &rows,
                remaining,
                local_hint.as_deref(),
                &mut |search| memory_guard(preparing.checked_add(search).ok_or_else(overflow)?),
                cancelled,
            )?,
        );
        Ok(this)
    }

    pub fn checked_retained_bytes(&self) -> Option<u128> {
        self.query
            .checked_retained_bytes()?
            .checked_add(self.task.checked_retained_bytes()?)?
            .checked_add(bytes::<usize>(self.row_map.capacity()).ok()?)?
            .checked_add(self.search.as_ref().map_or(
                Some(0),
                ExactCoverSearchSession::checked_retained_capacity_bytes,
            )?)?
            .checked_add(match &self.ready {
                Some(ExactAtMostShardOutcome::Found(rows)) => {
                    bytes::<usize>(rows.capacity()).ok()?
                }
                _ => 0,
            })
    }

    /// Probe-only access to existing live-search counters. A terminal shard
    /// no longer owns the search, so callers retain their last live sample.
    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_residual_progress(
        &self,
    ) -> Option<super::exact_minimum_cover::ExactMinimumCoverResidualDiagnostics> {
        self.search.as_ref()?.diagnostic_residual_progress()
    }

    /// Last live cost sample, with the same terminal-loss boundary as residual
    /// diagnostics. Callers retain their last sample, never infer proof from it.
    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_hot_cost(
        &self,
    ) -> Option<super::exact_minimum_cover::ExactMinimumCoverHotCostDiagnostics> {
        self.search.as_ref()?.diagnostic_hot_cost()
    }

    /// Live probe sample only. None means unavailable; a terminal advance may
    /// release the owner before the caller can observe its final counters.
    #[doc(hidden)]
    #[cfg(feature = "diagnostic-probes")]
    pub fn diagnostic_cached_pivot_exhaustion(
        &self,
    ) -> Option<super::exact_minimum_cover::ExactMinimumCoverPivotExhaustionDiagnostics> {
        self.search.as_ref()?.diagnostic_cached_pivot_exhaustion()
    }

    pub fn advance(
        &mut self,
        max_work: u64,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<ExactAtMostShardAdvance, ExactAtMostParallelError> {
        if self.finished {
            return Err(ExactAtMostParallelError::Finished);
        }
        if cancelled() {
            self.ready = Some(ExactAtMostShardOutcome::Cancelled);
            self.search = None;
        }
        if max_work == 0 && self.ready != Some(ExactAtMostShardOutcome::Cancelled) {
            return Ok(ExactAtMostShardAdvance::Pending { work_steps: 0 });
        }
        let outcome = if let Some(outcome) = self.ready.take() {
            outcome
        } else {
            let base = self
                .query
                .checked_retained_bytes()
                .ok_or_else(overflow)?
                .checked_add(self.task.checked_retained_bytes().ok_or_else(overflow)?)
                .and_then(|n| n.checked_add(bytes::<usize>(self.row_map.capacity()).ok()?))
                .ok_or_else(overflow)?;
            let advance = self
                .search
                .as_mut()
                .ok_or(ExactAtMostParallelError::Finished)?
                .advance(
                    max_work,
                    &mut |search| memory_guard(base.checked_add(search).ok_or_else(overflow)?),
                    cancelled,
                )?;
            match advance {
                ExactMinimumCoverSessionAdvance::Pending { visited_nodes } => {
                    return Ok(ExactAtMostShardAdvance::Pending {
                        work_steps: visited_nodes,
                    });
                }
                ExactMinimumCoverSessionAdvance::Found { result, .. } => {
                    // The reduced result stays live while its original-row
                    // witness is staged. Charge both before allocation; the
                    // owning coordinator adds its retained retry descriptor.
                    let retained = self
                        .checked_retained_bytes()
                        .ok_or_else(overflow)?
                        .checked_add(result.checked_retained_bytes().ok_or_else(overflow)?)
                        .ok_or_else(overflow)?;
                    let row_count = self
                        .task
                        .forced_rows
                        .len()
                        .checked_add(result.row_indices().len())
                        .ok_or_else(overflow)?;
                    memory_guard(
                        retained
                            .checked_add(bytes::<usize>(row_count)?)
                            .ok_or_else(overflow)?,
                    )?;
                    let mut rows = reserve(row_count)?;
                    memory_guard(
                        retained
                            .checked_add(bytes::<usize>(rows.capacity())?)
                            .ok_or_else(overflow)?,
                    )?;
                    rows.extend_from_slice(&self.task.forced_rows);
                    for &row in result.row_indices() {
                        rows.push(
                            *self
                                .row_map
                                .get(row)
                                .ok_or(ExactAtMostParallelError::InvalidWitness)?,
                        );
                    }
                    rows.sort_unstable();
                    self.query.validate_witness(&self.task, &rows)?;
                    ExactAtMostShardOutcome::Found(rows)
                }
                ExactMinimumCoverSessionAdvance::ProvedNone { .. } => {
                    ExactAtMostShardOutcome::ProvedNone
                }
                ExactMinimumCoverSessionAdvance::Cancelled { .. } => {
                    ExactAtMostShardOutcome::Cancelled
                }
                ExactMinimumCoverSessionAdvance::Finished => {
                    return Err(ExactAtMostParallelError::Finished);
                }
            }
        };
        let outcome_bytes = match &outcome {
            ExactAtMostShardOutcome::Found(rows) => bytes::<usize>(rows.capacity())?,
            _ => 0,
        };
        memory_guard(
            self.checked_retained_bytes()
                .ok_or_else(overflow)?
                .checked_add(outcome_bytes)
                .ok_or_else(overflow)?,
        )?;
        self.finished = true;
        self.search = None;
        // A finished shard never needs its descriptor again. Moving both
        // vectors avoids a hidden infallible allocation at receipt publication.
        let task = ExactAtMostTask {
            identity: self.task.identity,
            partition_id: self.task.partition_id,
            forced_rows: core::mem::take(&mut self.task.forced_rows),
            excluded_rows: core::mem::take(&mut self.task.excluded_rows),
        };
        Ok(ExactAtMostShardAdvance::Terminal(
            ExactAtMostReceipt::from_parts(task, outcome)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(masks: &[u64], limit: usize) -> ExactAtMostQuery {
        ExactAtMostQuery::new(
            ExactAtMostQueryIdentity {
                matrix_id: [7; 32],
                generation: 1,
                query_id: 1,
            },
            PatternBitSet::from_words(3, vec![7]).unwrap(),
            masks
                .iter()
                .map(|&mask| PatternBitSet::from_words(3, vec![mask]).unwrap())
                .collect(),
            limit,
        )
        .unwrap()
    }
    fn terminal(query: &ExactAtMostQuery, task: &ExactAtMostTask) -> ExactAtMostReceipt {
        let mut shard = ExactAtMostShardSession::prepare(
            query.clone(),
            task.clone(),
            &mut |_| Ok(()),
            &mut || false,
        )
        .unwrap();
        for _ in 0..10000 {
            if let ExactAtMostShardAdvance::Terminal(receipt) =
                shard.advance(32, &mut |_| Ok(()), &mut || false).unwrap()
            {
                return receipt;
            }
        }
        panic!("small exact shard did not terminate")
    }

    #[test]
    fn fully_forced_shard_preflights_witness_before_allocation() {
        let query = query(&[7; 64], 64);
        let task =
            ExactAtMostTask::from_parts(query.identity(), 0, (0..64).collect(), vec![]).unwrap();
        let base = query.checked_retained_bytes().unwrap() + task.checked_retained_bytes().unwrap();
        // Permit the old two-word scratch allowance, but not a 64-row
        // witness. The old clone branch never consulted this larger peak.
        let cap = base + 2 * core::mem::size_of::<u64>() as u128;
        let mut largest = 0;
        let declined = ExactAtMostShardSession::prepare(
            query.clone(),
            task.clone(),
            &mut |required_memory_bytes| {
                largest = largest.max(required_memory_bytes);
                if required_memory_bytes > cap {
                    Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                        required_memory_bytes,
                        max_memory_bytes: cap,
                    })
                } else {
                    Ok(())
                }
            },
            &mut || false,
        );
        assert!(matches!(
            declined,
            Err(ExactAtMostParallelError::Exact(
                ExactMinimumCoverError::MemoryCapacityExceeded { .. }
            ))
        ));
        assert!(largest >= base + 64 * core::mem::size_of::<usize>() as u128);
        let mut admitted =
            ExactAtMostShardSession::prepare(query, task, &mut |_| Ok(()), &mut || false).unwrap();
        let ExactAtMostShardAdvance::Terminal(receipt) =
            admitted.advance(1, &mut |_| Ok(()), &mut || false).unwrap()
        else {
            panic!("fully forced query must produce its positive witness")
        };
        assert!(
            matches!(receipt.outcome, ExactAtMostShardOutcome::Found(ref rows)
            if rows == &(0..64).collect::<Vec<_>>())
        );
    }

    #[test]
    fn terminal_receipt_moves_descriptor_and_rejects_unadmitted_ready_witness() {
        let query = query(&[7, 0], 1);
        let task = ExactAtMostTask::from_parts(query.identity(), 0, vec![0], vec![1]).unwrap();
        let make_shard = || {
            ExactAtMostShardSession::prepare(
                query.clone(),
                task.clone(),
                &mut |_| Ok(()),
                &mut || false,
            )
            .unwrap()
        };
        let mut shard = make_shard();
        let forced_ptr = shard.task.forced_rows.as_ptr();
        let excluded_ptr = shard.task.excluded_rows.as_ptr();
        let mut peak = 0;
        let ExactAtMostShardAdvance::Terminal(receipt) = shard
            .advance(
                1,
                &mut |bytes| {
                    peak = peak.max(bytes);
                    Ok(())
                },
                &mut || false,
            )
            .unwrap()
        else {
            panic!("fully forced witness is immediately ready")
        };
        assert_eq!(receipt.task.forced_rows.as_ptr(), forced_ptr);
        assert_eq!(receipt.task.excluded_rows.as_ptr(), excluded_ptr);
        assert_eq!(receipt.task, task);
        assert!(
            matches!(receipt.outcome, ExactAtMostShardOutcome::Found(ref rows) if rows == &[0])
        );
        assert!(shard.task.forced_rows.is_empty() && shard.task.excluded_rows.is_empty());
        assert!(peak > 0);
        let mut declined = make_shard();
        assert!(matches!(
            declined.advance(
                1,
                &mut |required_memory_bytes| {
                    if required_memory_bytes >= peak {
                        Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                            required_memory_bytes,
                            max_memory_bytes: peak - 1,
                        })
                    } else {
                        Ok(())
                    }
                },
                &mut || false
            ),
            Err(ExactAtMostParallelError::Exact(
                ExactMinimumCoverError::MemoryCapacityExceeded { .. }
            ))
        ));
        assert!(!declined.finished, "decline publishes no terminal receipt");
        assert_eq!(
            declined.task, task,
            "caller can retry the original issued task"
        );
    }

    #[test]
    fn advisory_warm_hints_are_validated_and_never_negative_authority() {
        let original = query(&[3, 5, 6, 0], 2);
        for hint in [vec![1, 1], vec![2, 1], vec![4]] {
            assert!(matches!(
                ExactAtMostQuery::new_with_witness_hint(
                    original.identity(),
                    original.required().clone(),
                    original.rows().to_vec(),
                    original.limit(),
                    Some(hint),
                ),
                Err(ExactAtMostParallelError::InvalidWitness)
            ));
        }
        // Covering, incomplete, empty and one-too-large hints must all produce
        // the same exact decision, in positive and negative AtMost queries.
        for limit in 0..=3 {
            for hint in [vec![], vec![3], vec![0, 1], vec![0, 1, 2]] {
                let query = ExactAtMostQuery::new_with_witness_hint(
                    original.identity(),
                    original.required().clone(),
                    original.rows().to_vec(),
                    limit,
                    Some(hint.clone()),
                )
                .unwrap();
                assert_eq!(query.witness_hint(), Some(hint.as_slice()));
                assert!(
                    query.checked_retained_bytes().unwrap()
                        >= original.checked_retained_bytes().unwrap()
                );
                let mut coordinator =
                    ExactAtMostCoordinator::prepare(query.clone(), 8, &mut |_| Ok(()), &mut || {
                        false
                    })
                    .unwrap();
                for task in coordinator.tasks().to_vec() {
                    if !matches!(
                        coordinator.decision(),
                        ExactAtMostParallelDecision::Pending { .. }
                    ) {
                        break;
                    }
                    coordinator.accept(terminal(&query, &task)).unwrap();
                }
                assert_eq!(
                    matches!(
                        coordinator.decision(),
                        ExactAtMostParallelDecision::Found(_)
                    ),
                    limit >= 2,
                    "limit={limit}, hint={hint:?}"
                );
            }
        }
    }

    #[test]
    fn replayed_one_extra_row_hint_falls_back_to_exact_negative() {
        let base = query(&[3, 5, 6], 1);
        let query = ExactAtMostQuery::new_with_witness_hint(
            base.identity(),
            base.required().clone(),
            base.rows().to_vec(),
            1,
            Some(vec![0, 1]),
        )
        .unwrap();
        let coordinator =
            ExactAtMostCoordinator::prepare(query.clone(), 1, &mut |_| Ok(()), &mut || false)
                .unwrap();
        assert_eq!(
            terminal(&query, &coordinator.tasks()[0]).outcome(),
            &ExactAtMostShardOutcome::ProvedNone,
        );
    }

    #[test]
    fn global_warm_witness_moves_capacity_under_guard_and_rejects_forgery() {
        let mut coordinator =
            ExactAtMostCoordinator::prepare(query(&[3, 5, 6], 2), 8, &mut |_| Ok(()), &mut || {
                false
            })
            .unwrap();
        let before = coordinator.checked_retained_bytes().unwrap();
        assert_eq!(
            coordinator.accept_warm_witness(vec![0], &mut |_| Ok(())),
            Err(ExactAtMostParallelError::InvalidWitness),
        );
        assert_eq!(
            coordinator.accept_warm_witness(vec![0, 1], &mut |_| {
                Err(ExactMinimumCoverError::MemoryGuardRejected)
            }),
            Err(ExactAtMostParallelError::Exact(
                ExactMinimumCoverError::MemoryGuardRejected
            )),
        );
        assert!(matches!(
            coordinator.decision(),
            ExactAtMostParallelDecision::Pending { .. }
        ));
        let mut rows = Vec::with_capacity(16);
        rows.extend([0, 1]);
        let expected = before + bytes::<usize>(rows.capacity()).unwrap();
        let mut observed = 0;
        coordinator
            .accept_warm_witness(rows, &mut |owned| {
                observed = owned;
                Ok(())
            })
            .unwrap();
        assert_eq!(observed, expected);
        assert_eq!(coordinator.checked_retained_bytes(), Some(expected));
        assert!(
            matches!(coordinator.decision(), ExactAtMostParallelDecision::Found(rows)
            if rows == &[0, 1])
        );
    }

    #[test]
    fn partitions_are_disjoint_exhaustive_and_match_brute_force() {
        // Every four-row matrix over a three-bit universe, every cardinality.
        for encoded in 0..4096_u64 {
            let masks: Vec<_> = (0..4).map(|index| (encoded >> (3 * index)) & 7).collect();
            for limit in 0..=4 {
                let query = query(&masks, limit);
                let mut coordinator =
                    ExactAtMostCoordinator::prepare(query.clone(), 5, &mut |_| Ok(()), &mut || {
                        false
                    })
                    .unwrap();
                let mut exists = false;
                for set in 0..16_u64 {
                    if set.count_ones() as usize > limit {
                        continue;
                    }
                    let covered = masks
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| set & (1 << i) != 0)
                        .fold(0, |a, (_, b)| a | b);
                    if covered != 7 {
                        continue;
                    }
                    exists = true;
                    let owners = coordinator
                        .tasks()
                        .iter()
                        .filter(|task| {
                            task.forced_rows.iter().all(|row| set & (1 << row) != 0)
                                && task.excluded_rows.iter().all(|row| set & (1 << row) == 0)
                        })
                        .count();
                    assert_eq!(owners, 1, "matrix={masks:?}, limit={limit}, set={set}");
                }
                for task in coordinator.tasks().to_vec() {
                    if !matches!(
                        coordinator.decision(),
                        ExactAtMostParallelDecision::Pending { .. }
                    ) {
                        break;
                    }
                    coordinator.accept(terminal(&query, &task)).unwrap();
                }
                assert_eq!(
                    matches!(
                        coordinator.decision(),
                        ExactAtMostParallelDecision::Found(_)
                    ),
                    exists,
                    "matrix={masks:?}, limit={limit}"
                );
                assert!(!matches!(
                    coordinator.decision(),
                    ExactAtMostParallelDecision::Pending { .. }
                ));
            }
        }
    }

    #[test]
    fn missing_stale_forged_duplicate_and_cancelled_never_prove_none() {
        let query = query(&[3, 5, 6], 1);
        let mut coordinator =
            ExactAtMostCoordinator::prepare(query.clone(), 4, &mut |_| Ok(()), &mut || false)
                .unwrap();
        assert!(coordinator.tasks().len() > 1);
        let task = coordinator.tasks()[0].clone();
        let mut stale = terminal(&query, &task);
        stale.task.identity.generation += 1;
        assert_eq!(
            coordinator.accept(stale),
            Err(ExactAtMostParallelError::StaleQuery)
        );
        let forged =
            ExactAtMostReceipt::from_parts(task.clone(), ExactAtMostShardOutcome::Found(vec![0]))
                .unwrap();
        assert_eq!(
            coordinator.accept(forged),
            Err(ExactAtMostParallelError::InvalidWitness)
        );
        let mut wrong = terminal(&query, &task);
        wrong.task.excluded_rows.push(99);
        assert_eq!(
            coordinator.accept(wrong),
            Err(ExactAtMostParallelError::UnknownPartition)
        );
        let receipt = terminal(&query, &task);
        assert!(matches!(
            coordinator.accept(receipt.clone()).unwrap(),
            ExactAtMostParallelDecision::Pending { .. }
        ));
        assert_eq!(
            coordinator.accept(receipt),
            Err(ExactAtMostParallelError::DuplicateReceipt)
        );
        let other = coordinator.tasks()[1].clone();
        let cancelled =
            ExactAtMostReceipt::from_parts(other, ExactAtMostShardOutcome::Cancelled).unwrap();
        assert_eq!(
            coordinator.accept(cancelled).unwrap(),
            ExactAtMostParallelDecision::Cancelled
        );
    }
}
