//! Save keeps the first original-order path to each terminal supply state.
//! Parent links retain that same witness without cloning its entire prefix at
//! every successor. Original graph/pattern windows share one memory ledger.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

use clearra_core_domain::parallel_work::{try_for_each_chunk, ParallelWorkError};
use clearra_replay::{CellOwner, ColoredCellOwner, PlacementStep, ReplayEvent};

use super::*;

type Error = ExactReplayMaterializationError;
type SupplyKey = (u16, Option<PieceKind>);
type SupplyMap = BTreeMap<SupplyKey, usize>;

const ROOT: usize = usize::MAX;
// A std BTreeMap node contains at most eleven inline key/value pairs and
// twelve child pointers. This bound also covers a newly allocated node with
// just one entry and its alignment; empty maps allocate no nodes.
const MAP_ENTRY_BYTES: u128 = 512;
const CELLS_PER_WORKER_WINDOW: usize = 16;

#[derive(Clone, Copy)]
struct Predecessor {
    parent: usize,
    edge: ScoringExecutionEdge,
    hold: HoldDecision,
}

#[derive(Default)]
struct Workspace {
    states: Vec<SupplyMap>,
    arena: Vec<Predecessor>,
    path: Vec<ScoringExecutionEdge>,
    holds: Vec<HoldDecision>,
    live_states: usize,
    pending_output_bytes: u128,
}

impl Workspace {
    fn retained_bytes(&self) -> Option<u128> {
        vec_bytes::<SupplyMap>(self.states.capacity())?
            .checked_add(vec_bytes::<Predecessor>(self.arena.capacity())?)?
            .checked_add(vec_bytes::<ScoringExecutionEdge>(self.path.capacity())?)?
            .checked_add(vec_bytes::<HoldDecision>(self.holds.capacity())?)?
            .checked_add((self.live_states as u128).checked_mul(MAP_ENTRY_BYTES)?)?
            .checked_add(self.pending_output_bytes)
    }

    fn admit(&self, ledger: &Ledger, owner: usize, future: u128) -> Result<(), Error> {
        ledger.ensure(
            owner,
            self.retained_bytes()
                .and_then(|bytes| bytes.checked_add(future))
                .ok_or(Error::ProjectionOverflow)?,
        )
    }

    fn prepare(&mut self, nodes: usize, ledger: &Ledger, owner: usize) -> Result<(), Error> {
        self.states.clear();
        self.arena.clear();
        self.path.clear();
        self.holds.clear();
        self.live_states = 0;
        if self.states.capacity() < nodes {
            self.admit(
                ledger,
                owner,
                vec_bytes::<SupplyMap>(nodes).ok_or(Error::ProjectionOverflow)?,
            )?;
            self.states
                .try_reserve_exact(nodes)
                .map_err(|_| Error::AllocationFailed)?;
        }
        self.states.resize_with(nodes, BTreeMap::new);
        self.admit(ledger, owner, 0)
    }

    fn insert(
        &mut self,
        next: SupplyState,
        parent: usize,
        edge: ScoringExecutionEdge,
        hold: HoldDecision,
        ledger: &Ledger,
        owner: usize,
    ) -> Result<(), Error> {
        let key = (next.cursor, next.hold);
        let index = next.node as usize;
        if self.states[index].contains_key(&key) {
            return Ok(());
        }
        let growth = if self.arena.len() == self.arena.capacity() {
            self.arena
                .capacity()
                .checked_mul(2)
                .map(|capacity| capacity.max(4))
                .ok_or(Error::ProjectionOverflow)?
        } else {
            0
        };
        self.admit(
            ledger,
            owner,
            vec_bytes::<Predecessor>(growth)
                .and_then(|bytes| bytes.checked_add(MAP_ENTRY_BYTES))
                .ok_or(Error::ProjectionOverflow)?,
        )?;
        if growth != 0 {
            self.arena
                .try_reserve_exact(growth - self.arena.len())
                .map_err(|_| Error::AllocationFailed)?;
        }
        let path = self.arena.len();
        self.arena.push(Predecessor { parent, edge, hold });
        self.states[index].insert(key, path);
        self.live_states = self
            .live_states
            .checked_add(1)
            .ok_or(Error::ProjectionOverflow)?;
        self.admit(ledger, owner, 0)
    }

    fn restore(&mut self, mut parent: usize, ledger: &Ledger, owner: usize) -> Result<(), Error> {
        let first = parent;
        let mut len = 0_usize;
        while parent != ROOT {
            let link = self.arena.get(parent).ok_or(Error::InvalidEvidence)?;
            if link.parent != ROOT && link.parent >= parent {
                return Err(Error::InvalidEvidence);
            }
            len = len.checked_add(1).ok_or(Error::ProjectionOverflow)?;
            parent = link.parent;
        }
        self.path.clear();
        self.holds.clear();
        let path_growth = if self.path.capacity() < len {
            vec_bytes::<ScoringExecutionEdge>(len).ok_or(Error::ProjectionOverflow)?
        } else {
            0
        };
        let hold_growth = if self.holds.capacity() < len {
            vec_bytes::<HoldDecision>(len).ok_or(Error::ProjectionOverflow)?
        } else {
            0
        };
        self.admit(
            ledger,
            owner,
            path_growth
                .checked_add(hold_growth)
                .ok_or(Error::ProjectionOverflow)?,
        )?;
        if path_growth != 0 {
            self.path
                .try_reserve_exact(len)
                .map_err(|_| Error::AllocationFailed)?;
        }
        if hold_growth != 0 {
            self.holds
                .try_reserve_exact(len)
                .map_err(|_| Error::AllocationFailed)?;
        }
        parent = first;
        while parent != ROOT {
            let link = self.arena[parent];
            self.path.push(link.edge);
            self.holds.push(link.hold);
            parent = link.parent;
        }
        self.path.reverse();
        self.holds.reverse();
        self.admit(ledger, owner, 0)
    }
}

struct CellOutput {
    owner: usize,
    executions: Vec<CandidateExecution>,
    retained_bytes: u128,
    complete: bool,
}

struct Ledger {
    max: u128,
    state: Mutex<LedgerState>,
}

struct LedgerState {
    owners: Vec<u128>,
    total: u128,
    peak: u128,
}

impl Ledger {
    fn new(base: u128, max: u128, owners: usize) -> Result<Self, Error> {
        check_limit(base, max)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(owners)
            .map_err(|_| Error::AllocationFailed)?;
        bytes.resize(owners, 0);
        Ok(Self {
            max,
            state: Mutex::new(LedgerState {
                owners: bytes,
                total: base,
                peak: base,
            }),
        })
    }

    fn ensure(&self, owner: usize, bytes: u128) -> Result<(), Error> {
        let mut state = self.state.lock().map_err(|_| Error::ParallelWorkerFailed)?;
        let total = state
            .total
            .checked_sub(state.owners[owner])
            .and_then(|total| total.checked_add(bytes))
            .ok_or(Error::ProjectionOverflow)?;
        check_limit(total, self.max)?;
        state.owners[owner] = bytes;
        state.total = total;
        state.peak = state.peak.max(total);
        Ok(())
    }

    fn transfer(&self, owner: usize, private: u128, aggregate: u128) -> Result<(), Error> {
        let mut state = self.state.lock().map_err(|_| Error::ParallelWorkerFailed)?;
        let total = state
            .total
            .checked_sub(state.owners[owner])
            .and_then(|total| total.checked_sub(state.owners[0]))
            .and_then(|total| total.checked_add(private))
            .and_then(|total| total.checked_add(aggregate))
            .ok_or(Error::ProjectionOverflow)?;
        check_limit(total, self.max)?;
        state.owners[owner] = private;
        state.owners[0] = aggregate;
        state.total = total;
        state.peak = state.peak.max(total);
        Ok(())
    }

    fn peak(&self) -> Result<u128, Error> {
        Ok(self
            .state
            .lock()
            .map_err(|_| Error::ParallelWorkerFailed)?
            .peak)
    }
}

pub(super) fn materialize(
    batch: &ExactScoringExecutionBatch,
    control: &ExecutionControl,
    already_retained_bytes: u128,
    max_memory_bytes: u128,
) -> Result<
    (
        ExactScoringExecutionMaterialization,
        ExactReplayMaterializationReport,
    ),
    Error,
> {
    if control.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let pattern_count = batch.patterns().len();
    let cells = batch
        .graphs()
        .len()
        .checked_mul(pattern_count)
        .ok_or(Error::ProjectionOverflow)?;
    let width = control.parallelism().min(cells);
    #[cfg(target_family = "wasm")]
    let width = width.min(1);
    let window = width
        .checked_mul(CELLS_PER_WORKER_WINDOW)
        .ok_or(Error::ProjectionOverflow)?;
    let fixed = vec_bytes::<Mutex<Workspace>>(width)
        .and_then(|bytes| bytes.checked_add(vec_bytes::<Mutex<Option<CellOutput>>>(window)?))
        .and_then(|bytes| bytes.checked_add(vec_bytes::<u128>(width.checked_add(1)?)?))
        .and_then(|bytes| bytes.checked_add((width as u128).checked_mul(2048)?))
        .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Ledger>() as u128))
        .ok_or(Error::ProjectionOverflow)?;
    let base = already_retained_bytes
        .checked_add(fixed)
        .ok_or(Error::ProjectionOverflow)?;
    let ledger = Ledger::new(base, max_memory_bytes, width + 1)?;
    let mut workers = Vec::new();
    workers
        .try_reserve_exact(width)
        .map_err(|_| Error::AllocationFailed)?;
    workers.extend((0..width).map(|_| Mutex::new(Workspace::default())));
    let mut by_graph = Vec::new();
    let mut aggregate_bytes = vec_bytes::<Vec<CandidateExecution>>(batch.graphs().len())
        .ok_or(Error::ProjectionOverflow)?;
    ledger.ensure(0, aggregate_bytes)?;
    by_graph
        .try_reserve_exact(batch.graphs().len())
        .map_err(|_| Error::AllocationFailed)?;
    by_graph.resize_with(batch.graphs().len(), Vec::new);
    aggregate_bytes = vec_bytes::<Vec<CandidateExecution>>(by_graph.capacity())
        .ok_or(Error::ProjectionOverflow)?;
    ledger.ensure(0, aggregate_bytes)?;
    let mut complete = batch.complete();
    let mut execution_count = 0_usize;
    let done = AtomicUsize::new(0);
    control.report_progress("save-terminal-execution", 0, Some(cells as u64));
    for start in (0..cells).step_by(window.max(1)) {
        let count = window.min(cells - start);
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(count)
            .map_err(|_| Error::AllocationFailed)?;
        slots.extend((0..count).map(|_| Mutex::new(None)));
        let next_worker = AtomicUsize::new(0);
        try_for_each_chunk(
            control,
            count,
            1,
            || Ok(next_worker.fetch_add(1, Ordering::Relaxed)),
            |worker_index, range| {
                let mut worker = workers
                    .get(*worker_index)
                    .ok_or(Error::InvalidEvidence)?
                    .lock()
                    .map_err(|_| Error::ParallelWorkerFailed)?;
                let owner = *worker_index + 1;
                for local in range {
                    let ordinal = start + local;
                    let graph = &batch.graphs()[ordinal / pattern_count];
                    let pattern_id = ordinal % pattern_count;
                    let output = materialize_cell(
                        batch,
                        graph,
                        pattern_id,
                        &mut worker,
                        &ledger,
                        owner,
                        control,
                    )?;
                    *slots[local]
                        .lock()
                        .map_err(|_| Error::ParallelWorkerFailed)? = Some(output);
                    let completed = done.fetch_add(1, Ordering::AcqRel) + 1;
                    if completed == cells || completed.is_multiple_of(64) {
                        control.report_progress(
                            "save-terminal-execution",
                            completed as u64,
                            Some(cells as u64),
                        );
                    }
                }
                Ok(())
            },
        )
        .map_err(|error| match error {
            ParallelWorkError::Worker(error) => error,
            ParallelWorkError::Cancelled => Error::Cancelled,
            ParallelWorkError::ThreadSpawnFailed => Error::AllocationFailed,
            ParallelWorkError::WorkerPanicked => Error::ParallelWorkerFailed,
        })?;
        for (local, slot) in slots.into_iter().enumerate() {
            let CellOutput {
                owner,
                mut executions,
                retained_bytes,
                complete: cell_complete,
            } = slot
                .into_inner()
                .map_err(|_| Error::ParallelWorkerFailed)?
                .ok_or(Error::InvalidEvidence)?;
            complete &= cell_complete;
            let output = &mut by_graph[(start + local) / pattern_count];
            let old_capacity = output.capacity();
            let source_inline = vec_bytes::<CandidateExecution>(executions.capacity())
                .ok_or(Error::ProjectionOverflow)?;
            let requested_capacity = output
                .len()
                .checked_add(executions.len())
                .ok_or(Error::ProjectionOverflow)?;
            let future = if requested_capacity > output.capacity() {
                vec_bytes::<CandidateExecution>(requested_capacity)
                    .ok_or(Error::ProjectionOverflow)?
            } else {
                0
            };
            ledger.ensure(
                0,
                aggregate_bytes
                    .checked_add(future)
                    .ok_or(Error::ProjectionOverflow)?,
            )?;
            output
                .try_reserve_exact(executions.len())
                .map_err(|_| Error::AllocationFailed)?;
            execution_count = execution_count
                .checked_add(executions.len())
                .ok_or(Error::ProjectionOverflow)?;
            output.append(&mut executions);
            drop(executions);
            aggregate_bytes = aggregate_bytes
                .checked_add(
                    vec_bytes::<CandidateExecution>(output.capacity() - old_capacity)
                        .ok_or(Error::ProjectionOverflow)?,
                )
                .and_then(|bytes| bytes.checked_add(retained_bytes.checked_sub(source_inline)?))
                .ok_or(Error::ProjectionOverflow)?;
            let mut worker = workers[owner - 1]
                .lock()
                .map_err(|_| Error::ParallelWorkerFailed)?;
            worker.pending_output_bytes = worker
                .pending_output_bytes
                .checked_sub(retained_bytes)
                .ok_or(Error::ProjectionOverflow)?;
            ledger.transfer(
                owner,
                worker.retained_bytes().ok_or(Error::ProjectionOverflow)?,
                aggregate_bytes,
            )?;
        }
    }
    drop(workers);
    for owner in 1..=width {
        ledger.ensure(owner, 0)?;
    }
    let final_inline = vec_bytes::<CandidateExecutionAggregate>(batch.graphs().len())
        .ok_or(Error::ProjectionOverflow)?;
    ledger.ensure(
        0,
        aggregate_bytes
            .checked_add(final_inline)
            .ok_or(Error::ProjectionOverflow)?,
    )?;
    let mut aggregates = Vec::new();
    aggregates
        .try_reserve_exact(batch.graphs().len())
        .map_err(|_| Error::AllocationFailed)?;
    for (graph, executions) in batch.graphs().iter().zip(by_graph) {
        aggregates.push(CandidateExecutionAggregate::new(
            graph.candidate_id(),
            executions,
        ));
    }
    let materialized = ExactScoringExecutionMaterialization {
        aggregates,
        scored_executions: Vec::new(),
        t_spin_single_pattern_ids: BTreeSet::new(),
        t_spin_single_candidate_ids: BTreeSet::new(),
        t_spin_single_execution_count: 0,
        complete,
        #[cfg(feature = "stage-profiling")]
        profile: None,
    };
    let retained_bytes = materialized
        .checked_replay_retained_bytes()
        .ok_or(Error::ProjectionOverflow)?;
    ledger.ensure(0, retained_bytes)?;
    if control.is_cancelled() {
        return Err(Error::Cancelled);
    }
    control.report_progress("save-terminal-execution", cells as u64, Some(cells as u64));
    Ok((
        materialized,
        ExactReplayMaterializationReport {
            execution_count,
            raw_execution_count: execution_count,
            retained_bytes,
            admitted_peak_bytes: ledger.peak()?,
        },
    ))
}

#[allow(clippy::too_many_arguments)]
fn materialize_cell(
    batch: &ExactScoringExecutionBatch,
    graph: &ExactScoringExecutionGraph,
    pattern_id: usize,
    workspace: &mut Workspace,
    ledger: &Ledger,
    owner: usize,
    control: &ExecutionControl,
) -> Result<CellOutput, Error> {
    let sequence = &batch.patterns()[pattern_id];
    workspace.prepare(graph.node_count(), ledger, owner)?;
    let mut executions = Vec::new();
    let mut retained_bytes = 0_u128;
    let mut complete = true;
    if graph.root() as usize >= workspace.states.len() {
        return Ok(CellOutput {
            owner,
            executions,
            retained_bytes,
            complete: false,
        });
    }
    workspace.admit(ledger, owner, MAP_ENTRY_BYTES)?;
    workspace.states[graph.root() as usize]
        .insert((batch.initial_cursor(), batch.initial_hold()), ROOT);
    workspace.live_states = 1;
    workspace.admit(ledger, owner, 0)?;
    for node_index in 0..graph.node_count() {
        if control.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let Some(node) = graph.node(node_index as u32) else {
            complete = false;
            continue;
        };
        let incoming = core::mem::take(&mut workspace.states[node_index]);
        if node.accepting() {
            for (&(cursor, hold), &parent) in &incoming {
                if !terminal_supply_state_is_accepted(
                    batch,
                    sequence,
                    SupplyState {
                        node: node_index as u32,
                        cursor,
                        hold,
                    },
                ) {
                    continue;
                }
                workspace.restore(parent, ledger, owner)?;
                let future = replay_peak_bytes(batch, workspace.path.len())
                    .ok_or(Error::ProjectionOverflow)?;
                workspace.admit(ledger, owner, future)?;
                let Some(trace_identity) =
                    TraceCanonicalKey::from_scoring_path_with_initial_supply(
                        batch.layout(),
                        &workspace.path,
                        &workspace.holds,
                        usize::from(batch.initial_cursor()),
                        batch.initial_hold(),
                    )
                    .map(|key| key.stable_key())
                else {
                    complete = false;
                    continue;
                };
                let Some(trace) =
                    replay_path(batch, graph, pattern_id, &workspace.path, &workspace.holds)
                else {
                    complete = false;
                    continue;
                };
                if !trace.canonical_key_matches(&trace_identity) {
                    complete = false;
                    continue;
                }
                let nested = (trace_identity.capacity() as u128)
                    .checked_add(
                        trace
                            .checked_nested_retained_bytes()
                            .ok_or(Error::ProjectionOverflow)?,
                    )
                    .ok_or(Error::ProjectionOverflow)?;
                if nested > future {
                    return Err(Error::ProjectionOverflow);
                }
                let old_capacity = executions.capacity();
                let inline_growth = if executions.len() == old_capacity {
                    vec_bytes::<CandidateExecution>(
                        executions
                            .len()
                            .checked_add(1)
                            .ok_or(Error::ProjectionOverflow)?,
                    )
                    .ok_or(Error::ProjectionOverflow)?
                } else {
                    0
                };
                workspace.admit(
                    ledger,
                    owner,
                    nested
                        .checked_add(inline_growth)
                        .ok_or(Error::ProjectionOverflow)?,
                )?;
                executions
                    .try_reserve_exact(1)
                    .map_err(|_| Error::AllocationFailed)?;
                let added = nested
                    .checked_add(
                        vec_bytes::<CandidateExecution>(executions.capacity() - old_capacity)
                            .ok_or(Error::ProjectionOverflow)?,
                    )
                    .ok_or(Error::ProjectionOverflow)?;
                retained_bytes = retained_bytes
                    .checked_add(added)
                    .ok_or(Error::ProjectionOverflow)?;
                workspace.pending_output_bytes = workspace
                    .pending_output_bytes
                    .checked_add(added)
                    .ok_or(Error::ProjectionOverflow)?;
                executions.push(CandidateExecution::new(pattern_id, trace_identity, trace));
                workspace.admit(ledger, owner, 0)?;
            }
        } else if let Some(edges) = graph.checked_edges(node) {
            for edge in edges.iter().copied() {
                let child_index = edge.to() as usize;
                let Some(child) = graph.node(edge.to()) else {
                    complete = false;
                    continue;
                };
                if child_index <= node_index || child_index >= workspace.states.len() {
                    complete = false;
                    continue;
                }
                for (&(cursor, hold), &parent) in &incoming {
                    if control.is_cancelled() {
                        return Err(Error::Cancelled);
                    }
                    let state = SupplyState {
                        node: node_index as u32,
                        cursor,
                        hold,
                    };
                    if batch.projects_unplaced_lookahead()
                        && batch.hold_enabled()
                        && usize::from(cursor) == sequence.len()
                        && hold == Some(edge.piece())
                        && child.accepting()
                        && (!batch.projects_standard_bag_lookahead()
                            || first_standard_bag_lookahead(sequence).is_none())
                    {
                        workspace.insert(
                            SupplyState {
                                node: edge.to(),
                                cursor: cursor.saturating_add(1),
                                hold,
                            },
                            parent,
                            edge,
                            HoldDecision::ReleaseHeldAtTerminal {
                                held_piece: edge.piece(),
                            },
                            ledger,
                            owner,
                        )?;
                    }
                    let mut branch_error = None;
                    let result = for_each_supply_successor(
                        batch,
                        sequence,
                        state,
                        edge.piece(),
                        |decision, next| match workspace.insert(
                            SupplyState {
                                node: edge.to(),
                                ..next
                            },
                            parent,
                            edge,
                            decision,
                            ledger,
                            owner,
                        ) {
                            Ok(()) => Ok(()),
                            Err(error) => {
                                branch_error = Some(error);
                                Err(ExactScoringExecutionCancelled)
                            }
                        },
                    );
                    if let Some(error) = branch_error {
                        return Err(error);
                    }
                    if result.is_err() {
                        return Err(if control.is_cancelled() {
                            Error::Cancelled
                        } else {
                            Error::InvalidEvidence
                        });
                    }
                }
            }
        } else {
            complete = false;
        }
        let released = incoming.len();
        drop(incoming);
        workspace.live_states = workspace
            .live_states
            .checked_sub(released)
            .ok_or(Error::ProjectionOverflow)?;
        workspace.admit(ledger, owner, 0)?;
    }
    executions.sort_unstable_by(|left, right| left.trace_identity().cmp(right.trace_identity()));
    Ok(CellOutput {
        owner,
        executions,
        retained_bytes,
        complete,
    })
}

// All transient owners of replay_path and its canonical key. There are at
// most thirteen replay events per lock plus two initial events. Four backing
// copies cover Vec growth/reallocation; only Lock has nested event storage.
// The key has two usize cursor fields and fixed-width piece/rotation/mask
// fields, bounded by 256 bytes per step on supported 32/64-bit hosts.
fn replay_peak_bytes(batch: &ExactScoringExecutionBatch, steps: usize) -> Option<u128> {
    let steps = steps as u128;
    let cells = u128::from(batch.layout().cell_count());
    let per_step = (core::mem::size_of::<BuildVariantOperation>() as u128)
        .checked_mul(2)?
        .checked_add((core::mem::size_of::<HoldDecision>() as u128).checked_mul(4)?)?
        .checked_add((core::mem::size_of::<usize>() as u128).checked_mul(2)?)?
        .checked_add(core::mem::size_of::<MovementEvidenceEvent>() as u128)?
        .checked_add((core::mem::size_of::<KickEvidenceEvent>() as u128).checked_mul(4)?)?
        .checked_add(core::mem::size_of::<PlacementStep>() as u128)?
        .checked_add(1 + 512)?
        .checked_add(
            cells
                .max(4)
                .checked_mul(4)?
                .checked_mul(core::mem::size_of::<CellOwner>() as u128)?,
        )?;
    let events = steps
        .checked_mul(13)?
        .checked_add(2)?
        .max(4)
        .checked_mul(4)?
        .checked_mul(core::mem::size_of::<ReplayEvent>() as u128)?;
    let owner_work = cells.checked_mul(
        (core::mem::size_of::<Option<ColoredCellOwner>>() as u128)
            .checked_mul(2)?
            .checked_add((core::mem::size_of::<Option<CellOwner>>() as u128).checked_mul(3)?)?,
    )?;
    steps
        .checked_mul(per_step)?
        .checked_add(events)?
        .checked_add(owner_work)?
        .checked_add(512)
}

fn vec_bytes<T>(capacity: usize) -> Option<u128> {
    (capacity as u128).checked_mul(core::mem::size_of::<T>() as u128)
}

fn check_limit(required_memory_bytes: u128, max_memory_bytes: u128) -> Result<(), Error> {
    if required_memory_bytes > max_memory_bytes {
        Err(Error::MemoryLimitExceeded {
            required_memory_bytes,
            max_memory_bytes,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_ledger_admits_all_eight_live_owners_and_moves_output_without_duplication() {
        let external = 12_345;
        let workspace = 4096;
        let cap = external + workspace * 8;
        let exact = Ledger::new(external, cap, 9).unwrap();
        let short = Ledger::new(external, cap - 1, 9).unwrap();
        for owner in 1..=8 {
            exact.ensure(owner, workspace).unwrap();
            if owner == 8 {
                assert!(
                    matches!(short.ensure(owner, workspace), Err(Error::MemoryLimitExceeded { required_memory_bytes, max_memory_bytes }) if required_memory_bytes == cap && max_memory_bytes == cap - 1)
                );
            } else {
                short.ensure(owner, workspace).unwrap();
            }
        }
        assert_eq!(exact.peak().unwrap(), cap);
        exact.transfer(3, workspace - 100, 100).unwrap();
        assert_eq!(exact.state.lock().unwrap().total, cap);
        assert_eq!(exact.peak().unwrap(), cap);
        assert!(matches!(
            exact.ensure(0, 101),
            Err(Error::MemoryLimitExceeded { .. })
        ));
    }
}
