// SRP rationale: exact visible replay language is this module's single change reason.
// Admitted subset construction, suffix counts and coherent rank selection share
// the same canonical-label invariant; rendering and product paging remain outside.
//! Exact finite visible-language counting and rank/select over replay evidence.
//! Source DAG paths are not identities: subset construction unions equal trk1
//! labels before adding suffix counts. No displayed-page sample grants authority.
use super::{
    exact_scoring_execution_materializer::replay_path,
    execution_supply::{
        first_standard_bag_lookahead, for_each_supply_successor, terminal_supply_state_is_accepted,
        SupplyState,
    },
    CandidateExecution, ExactReplayMaterializationError as Error,
    ExactReplayMaterializationLimits as Limits,
};
use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_replay::{
    trace::solution_trace_builder::SolutionTraceBuilder, ExactScoringExecutionBatch, HoldDecision,
    ScoringExecutionEdge, TraceCanonicalKey,
};
use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    mem::size_of,
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactReplayGraphLocation {
    pub batch: usize,
    pub graph: usize,
}

// Fixed stack labels reuse the canonical writer; decimal fields are compared as
// bytes, not numeric tuples. 192 covers even usize::MAX cursor text; overflow is
// explicitly rejected rather than truncating a label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Label {
    bytes: [u8; 192],
    len: usize,
}
impl Label {
    fn new(
        depth: usize,
        edge: ScoringExecutionEdge,
        hold: HoldDecision,
        mask: u64,
    ) -> Result<Self, Error> {
        let mut label = Self {
            bytes: [0; 192],
            len: 0,
        };
        TraceCanonicalKey::write_scoring_step_key(&mut label, depth, edge, hold, mask)
            .map_err(|_| Error::ProjectionOverflow)?;
        Ok(label)
    }
}
impl fmt::Write for Label {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len.checked_add(text.len()).ok_or(fmt::Error)?;
        self.bytes
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}
impl Ord for Label {
    fn cmp(&self, other: &Self) -> Ordering {
        self.bytes[..self.len].cmp(&other.bytes[..other.len])
    }
}
impl PartialOrd for Label {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct NfaKey {
    location: usize,
    supply: SupplyState,
    board: u64,
    depth: usize,
}
#[derive(Clone, Copy, Debug)]
struct NfaEdge {
    label: Label,
    destination: usize,
    edge: ScoringExecutionEdge,
    hold: HoldDecision,
    end_count: usize,
}
#[derive(Clone, Debug)]
struct NfaNode {
    key: NfaKey,
    accepting: bool,
    edges: Vec<NfaEdge>,
    raw_count: usize,
}
#[derive(Clone, Debug)]
struct DfaEdge {
    label: Label,
    destination: usize,
    end_count: usize,
    predecessors: Vec<Predecessor>,
}
#[derive(Clone, Copy, Debug)]
struct Predecessor {
    source: usize,
    edge: usize,
}
#[derive(Clone, Debug)]
struct DfaNode {
    members: Vec<usize>,
    accepting: Option<usize>,
    edges: Vec<DfaEdge>,
    count: usize,
}
#[derive(Clone, Copy, Debug)]
struct Choice {
    label: Label,
    destination: usize,
    predecessor: Predecessor,
}
// One coherent predecessor suffices for an identical visible transition and
// identical target constituent. Different target languages are never dropped.
impl PartialEq for Choice {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label && self.destination == other.destination
    }
}
impl Eq for Choice {}
impl Ord for Choice {
    fn cmp(&self, other: &Self) -> Ordering {
        self.label
            .cmp(&other.label)
            .then_with(|| self.destination.cmp(&other.destination))
    }
}
impl PartialOrd for Choice {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Roots,
    Nfa,
    NfaCount,
    DfaRoots,
    Dfa,
    DfaCount,
    CheckCaps,
    Complete,
    Failed,
}

/// A source-bound cooperative cursor. Its heap projection excludes the immutable
/// Arc pointee: the App owner must admit that source exactly once beside this
/// cursor. `guard` always receives the full engine inline+heap+temporary peak.
/// Cancellation or any rejection poisons the cursor, never a partial count.
#[derive(Debug)]
pub struct ExactReplayLanguageSession {
    batches: Arc<[ExactScoringExecutionBatch]>,
    locations: Vec<ExactReplayGraphLocation>,
    pattern_id: usize,
    limits: Limits,
    phase: Phase,
    nfa: Vec<NfaNode>,
    nfa_table: Vec<usize>,
    roots: Vec<usize>,
    dfa: Vec<DfaNode>,
    dfa_table: Vec<usize>,
    prefix_roots: Vec<usize>,
    nested_bytes: u128,
    choices: Vec<Choice>,
    subset: Vec<usize>,
    predecessors: Vec<Predecessor>,
    index: usize,
    edge_index: usize,
    member_index: usize,
    group_end: usize,
    sum: usize,
    count: Option<usize>,
    work: u64,
    rehash: Vec<usize>,
    rehash_kind: u8,
    rehash_index: usize,
    rehash_target: usize,
    deterministic: bool,
    fast: bool,
}

fn bytes<T>(values: &Vec<T>) -> Result<u128, Error> {
    (values.capacity() as u128)
        .checked_mul(size_of::<T>() as u128)
        .ok_or(Error::ProjectionOverflow)
}
fn admit(
    limits: Limits,
    peak: u128,
    guard: &mut impl FnMut(u128) -> Result<(), Error>,
) -> Result<(), Error> {
    // Fixed stack labels, four supply successors, and helper inline carriers.
    // No recursion is used by the counting engine.
    let peak = peak.checked_add(4096).ok_or(Error::ProjectionOverflow)?;
    if peak > limits.max_retained_bytes() {
        return Err(Error::MemoryLimitExceeded {
            required_memory_bytes: peak,
            max_memory_bytes: limits.max_retained_bytes(),
        });
    }
    guard(peak)
}
// Existing storage remains live during reallocation; reserve both requested new
// storage and old storage, then verify actual capacity before any write.
fn reserve<T>(
    values: &mut Vec<T>,
    additional: usize,
    baseline: u128,
    limits: Limits,
    guard: &mut impl FnMut(u128) -> Result<(), Error>,
) -> Result<u128, Error> {
    let needed = values
        .len()
        .checked_add(additional)
        .ok_or(Error::ProjectionOverflow)?;
    if needed <= values.capacity() {
        return Ok(0);
    }
    let wanted = needed
        .max(
            values
                .capacity()
                .checked_mul(2)
                .ok_or(Error::ProjectionOverflow)?,
        )
        .max(4);
    let old = bytes(values)?;
    let new = (wanted as u128)
        .checked_mul(size_of::<T>() as u128)
        .ok_or(Error::ProjectionOverflow)?;
    admit(
        limits,
        baseline.checked_add(new).ok_or(Error::ProjectionOverflow)?,
        guard,
    )?;
    values
        .try_reserve_exact(wanted - values.len())
        .map_err(|_| Error::AllocationFailed)?;
    let delta = bytes(values)?
        .checked_sub(old)
        .ok_or(Error::ProjectionOverflow)?;
    admit(
        limits,
        baseline
            .checked_add(delta)
            .ok_or(Error::ProjectionOverflow)?,
        guard,
    )?;
    Ok(delta)
}

struct KeyHash(u64);
impl Hasher for KeyHash {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211);
        }
    }
}
fn hash(key: &impl Hash) -> usize {
    let mut h = KeyHash(14_695_981_039_346_656_037);
    key.hash(&mut h);
    h.finish() as usize
}

impl ExactReplayLanguageSession {
    /// Zero-based lexical selection. Suffix counts skip complete families;
    /// backtracking through a single coherent NFA predecessor chain prevents
    /// combining hidden witnesses from unrelated graph locations.
    pub fn select(
        &self,
        mut rank: usize,
        control: &ExecutionControl,
        guard: &mut impl FnMut(u128) -> Result<(), Error>,
    ) -> Result<CandidateExecution, Error> {
        if control.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if self.count.is_none_or(|count| rank >= count) {
            return Err(Error::InvalidEvidence);
        }
        if self.fast {
            return self.select_deterministic(rank, control, guard);
        }
        let mut node = *self.prefix_roots.last().ok_or(Error::InvalidEvidence)?;
        let mut route: Vec<(usize, usize)> = Vec::new();
        let mut path = Vec::new();
        let mut holds = Vec::new();
        let original = self.owned()?;
        let mut chosen;
        loop {
            if control.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let state = &self.dfa[node];
            if let Some(accepting) = state.accepting {
                if rank == 0 {
                    chosen = accepting;
                    break;
                }
            }
            let edge_index = state.edges.partition_point(|edge| edge.end_count <= rank);
            let edge = state.edges.get(edge_index).ok_or(Error::InvalidEvidence)?;
            let preceding = if edge_index == 0 {
                usize::from(state.accepting.is_some())
            } else {
                state.edges[edge_index - 1].end_count
            };
            rank = rank
                .checked_sub(preceding)
                .ok_or(Error::ProjectionOverflow)?;
            if route.len() >= self.limits.max_path_steps() {
                return Err(Error::PathStepLimitExceeded {
                    max_path_steps: self.limits.max_path_steps(),
                });
            }
            let route_base = original
                .checked_add(bytes(&route)?)
                .ok_or(Error::ProjectionOverflow)?;
            reserve(&mut route, 1, route_base, self.limits, guard)?;
            route.push((node, edge_index));
            node = edge.destination;
        }
        let base = original
            .checked_add(bytes(&route)?)
            .ok_or(Error::ProjectionOverflow)?;
        reserve(&mut path, route.len(), base, self.limits, guard)?;
        reserve(
            &mut holds,
            route.len(),
            base.checked_add(bytes(&path)?)
                .ok_or(Error::ProjectionOverflow)?,
            self.limits,
            guard,
        )?;
        for (previous, selected) in route.iter().rev() {
            if control.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let selected = &self.dfa[*previous].edges[*selected];
            let position = self.dfa[selected.destination]
                .members
                .binary_search(&chosen)
                .map_err(|_| Error::InvalidEvidence)?;
            let predecessor = selected
                .predecessors
                .get(position)
                .ok_or(Error::InvalidEvidence)?;
            let edge = self.nfa[predecessor.source]
                .edges
                .get(predecessor.edge)
                .ok_or(Error::InvalidEvidence)?;
            if edge.destination != chosen || edge.label != selected.label {
                return Err(Error::InvalidEvidence);
            }
            path.push(edge.edge);
            holds.push(edge.hold);
            chosen = predecessor.source;
        }
        path.reverse();
        holds.reverse();
        let scratch = bytes(&route)?
            .checked_add(bytes(&path)?)
            .and_then(|n| n.checked_add(bytes(&holds).ok()?))
            .ok_or(Error::ProjectionOverflow)?;
        self.project_selected(chosen, &path, &holds, scratch, control, guard)
    }

    fn project_selected(
        &self,
        chosen: usize,
        path: &[ScoringExecutionEdge],
        holds: &[HoldDecision],
        scratch: u128,
        control: &ExecutionControl,
        guard: &mut impl FnMut(u128) -> Result<(), Error>,
    ) -> Result<CandidateExecution, Error> {
        if control.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let original = self.owned()?;
        let root_key = self.nfa[chosen].key;
        if root_key.depth != 0 || path.is_empty() {
            return Err(Error::InvalidEvidence);
        }
        let location = self.locations[root_key.location];
        let batch = &self.batches[location.batch];
        let graph = &batch.graphs()[location.graph];
        let projection = replay_projection(path.len(), usize::from(batch.layout().cell_count()))?;
        admit(
            self.limits,
            original
                .checked_add(scratch)
                .and_then(|n| n.checked_add(projection))
                .ok_or(Error::ProjectionOverflow)?,
            guard,
        )?;
        let trace = replay_path(batch, graph, self.pattern_id, &path, &holds)
            .ok_or(Error::InvalidEvidence)?;
        let trace_bytes = (size_of::<clearra_replay::ReplayTrace>() as u128)
            .checked_add(
                trace
                    .checked_nested_retained_bytes()
                    .ok_or(Error::ProjectionOverflow)?,
            )
            .ok_or(Error::ProjectionOverflow)?;
        let key_bytes = trace
            .checked_canonical_key_requested_bytes()
            .ok_or(Error::ProjectionOverflow)?;
        admit(
            self.limits,
            original
                .checked_add(scratch)
                .and_then(|n| n.checked_add(trace_bytes))
                .and_then(|n| n.checked_add(key_bytes))
                .ok_or(Error::ProjectionOverflow)?,
            guard,
        )?;
        let identity = trace.canonical_key();
        let mut remaining = identity
            .strip_prefix("trk1:")
            .ok_or(Error::InvalidEvidence)?;
        let mut board = batch.initial_occupied();
        for (index, (&edge, &hold)) in path.iter().zip(holds).enumerate() {
            if control.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if index != 0 {
                remaining = remaining.strip_prefix('~').ok_or(Error::InvalidEvidence)?;
            }
            let (mask, next_board) =
                SolutionTraceBuilder::project_scoring_step(batch.layout(), board, edge)
                    .ok_or(Error::InvalidEvidence)?;
            board = next_board;
            let canonical = Label::new(index, edge, hold, mask)?;
            let label = std::str::from_utf8(&canonical.bytes[..canonical.len])
                .map_err(|_| Error::InvalidEvidence)?;
            remaining = remaining
                .strip_prefix(label)
                .ok_or(Error::InvalidEvidence)?;
        }
        if !remaining.is_empty() {
            return Err(Error::InvalidEvidence);
        }
        let candidate = CandidateExecution::new(self.pattern_id, identity, trace);
        admit(
            self.limits,
            original
                .checked_add(scratch)
                .and_then(|n| n.checked_add(size_of::<CandidateExecution>() as u128))
                .and_then(|n| n.checked_add(candidate.checked_nested_retained_bytes()?))
                .ok_or(Error::ProjectionOverflow)?,
            guard,
        )?;
        Ok(candidate)
    }

    fn select_deterministic(
        &self,
        mut rank: usize,
        control: &ExecutionControl,
        guard: &mut impl FnMut(u128) -> Result<(), Error>,
    ) -> Result<CandidateExecution, Error> {
        let root = self.roots[0];
        let mut node = root;
        let mut path = Vec::new();
        let mut holds = Vec::new();
        loop {
            if control.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let state = &self.nfa[node];
            if state.accepting && rank == 0 {
                break;
            }
            let index = state.edges.partition_point(|edge| edge.end_count <= rank);
            let edge = state.edges.get(index).ok_or(Error::InvalidEvidence)?;
            let preceding = if index == 0 {
                usize::from(state.accepting)
            } else {
                state.edges[index - 1].end_count
            };
            rank = rank
                .checked_sub(preceding)
                .ok_or(Error::ProjectionOverflow)?;
            let base = self
                .owned()?
                .checked_add(bytes(&path)?)
                .and_then(|n| n.checked_add(bytes(&holds).ok()?))
                .ok_or(Error::ProjectionOverflow)?;
            reserve(&mut path, 1, base, self.limits, guard)?;
            let base = self
                .owned()?
                .checked_add(bytes(&path)?)
                .and_then(|n| n.checked_add(bytes(&holds).ok()?))
                .ok_or(Error::ProjectionOverflow)?;
            reserve(&mut holds, 1, base, self.limits, guard)?;
            path.push(edge.edge);
            holds.push(edge.hold);
            node = edge.destination;
        }
        let scratch = bytes(&path)?
            .checked_add(bytes(&holds)?)
            .ok_or(Error::ProjectionOverflow)?;
        self.project_selected(root, &path, &holds, scratch, control, guard)
    }

    pub fn new(
        batches: Arc<[ExactScoringExecutionBatch]>,
        locations: Vec<ExactReplayGraphLocation>,
        pattern_id: usize,
        limits: Limits,
        guard: &mut impl FnMut(u128) -> Result<(), Error>,
    ) -> Result<Self, Error> {
        if locations.is_empty() {
            return Err(Error::InvalidEvidence);
        }
        let session = Self {
            batches,
            locations,
            pattern_id,
            limits,
            phase: Phase::Roots,
            nfa: Vec::new(),
            nfa_table: Vec::new(),
            roots: Vec::new(),
            dfa: Vec::new(),
            dfa_table: Vec::new(),
            prefix_roots: Vec::new(),
            nested_bytes: 0,
            choices: Vec::new(),
            subset: Vec::new(),
            predecessors: Vec::new(),
            index: 0,
            edge_index: 0,
            member_index: 0,
            group_end: 0,
            sum: 0,
            count: None,
            work: 0,
            rehash: Vec::new(),
            rehash_kind: 0,
            rehash_index: 0,
            rehash_target: 0,
            deterministic: true,
            fast: false,
        };
        admit(limits, session.owned()?, guard)?;
        Ok(session)
    }

    pub fn checked_retained_bytes(&self) -> Option<u128> {
        self.owned().ok()
    }
    pub fn count(&self) -> Option<usize> {
        self.count
    }
    pub fn work_units(&self) -> u64 {
        self.work
    }
    fn owned(&self) -> Result<u128, Error> {
        let mut n = (size_of::<Self>() as u128)
            .checked_add(self.nested_bytes)
            .ok_or(Error::ProjectionOverflow)?;
        for extra in [
            bytes(&self.locations)?,
            bytes(&self.nfa)?,
            bytes(&self.nfa_table)?,
            bytes(&self.roots)?,
            bytes(&self.dfa)?,
            bytes(&self.dfa_table)?,
            bytes(&self.prefix_roots)?,
            bytes(&self.choices)?,
            bytes(&self.subset)?,
            bytes(&self.predecessors)?,
            bytes(&self.rehash)?,
        ] {
            n = n.checked_add(extra).ok_or(Error::ProjectionOverflow)?;
        }
        Ok(n)
    }
    fn switch(&mut self, phase: Phase) {
        self.phase = phase;
        self.index = 0;
        self.edge_index = 0;
        self.member_index = 0;
        self.group_end = 0;
        self.sum = 0;
    }

    /// A primitive advances one source edge, one subset member/edge, one suffix
    /// count edge, or one prefix obligation. It never enumerates terminal paths.
    pub fn advance(
        &mut self,
        work: usize,
        control: &ExecutionControl,
        guard: &mut impl FnMut(u128) -> Result<(), Error>,
    ) -> Result<bool, Error> {
        if self.phase == Phase::Failed {
            return Err(Error::InvalidEvidence);
        }
        for _ in 0..work.max(1) {
            let result = if control.is_cancelled() {
                Err(Error::Cancelled)
            } else {
                self.step(guard)
            };
            if let Err(error) = result {
                self.phase = Phase::Failed;
                self.count = None;
                // A rejected actual-capacity check may have resized a nested
                // owner before its cached delta was committed. Release all
                // optional state now; no stale projection survives failure.
                self.nfa = Vec::new();
                self.nfa_table = Vec::new();
                self.roots = Vec::new();
                self.dfa = Vec::new();
                self.dfa_table = Vec::new();
                self.prefix_roots = Vec::new();
                self.choices = Vec::new();
                self.subset = Vec::new();
                self.predecessors = Vec::new();
                self.rehash = Vec::new();
                self.nested_bytes = 0;
                return Err(error);
            }
            self.work = self.work.checked_add(1).ok_or(Error::ProjectionOverflow)?;
            if self.phase == Phase::Complete {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Table initialization and rehashing are themselves resumable work. Four
    /// free NFA admissions cover the maximum supply fanout of one source edge.
    fn maintain_tables(
        &mut self,
        guard: &mut impl FnMut(u128) -> Result<(), Error>,
    ) -> Result<bool, Error> {
        if self.rehash_kind != 0 {
            if self.rehash.len() < self.rehash_target {
                self.rehash.push(0);
                return Ok(true);
            }
            let count = if self.rehash_kind == 1 {
                self.nfa.len()
            } else {
                self.dfa.len()
            };
            if self.rehash_index < count {
                let hash = if self.rehash_kind == 1 {
                    hash(&self.nfa[self.rehash_index].key)
                } else {
                    hash(&self.dfa[self.rehash_index].members)
                };
                let mut slot = hash & (self.rehash.len() - 1);
                while self.rehash[slot] != 0 {
                    slot = (slot + 1) & (self.rehash.len() - 1);
                }
                self.rehash[slot] = self.rehash_index + 1;
                self.rehash_index += 1;
                return Ok(true);
            }
            if self.rehash_kind == 1 {
                self.nfa_table = std::mem::take(&mut self.rehash);
            } else {
                self.dfa_table = std::mem::take(&mut self.rehash);
            }
            self.rehash_kind = 0;
            return Ok(true);
        }
        let (kind, length, wanted) = match self.phase {
            Phase::Roots | Phase::Nfa => (
                1,
                self.nfa_table.len(),
                self.nfa
                    .len()
                    .checked_add(4)
                    .ok_or(Error::ProjectionOverflow)?,
            ),
            Phase::DfaRoots | Phase::Dfa => (
                2,
                self.dfa_table.len(),
                self.dfa
                    .len()
                    .checked_add(1)
                    .ok_or(Error::ProjectionOverflow)?,
            ),
            _ => return Ok(false),
        };
        if wanted <= length / 2 {
            return Ok(false);
        }
        self.rehash_target = length
            .checked_mul(2)
            .ok_or(Error::ProjectionOverflow)?
            .max(16);
        let base = self.owned()?;
        reserve(
            &mut self.rehash,
            self.rehash_target,
            base,
            self.limits,
            guard,
        )?;
        self.rehash_kind = kind;
        self.rehash_index = 0;
        Ok(true)
    }

    fn ensure_nfa_table(&self) -> Result<(), Error> {
        if self
            .nfa
            .len()
            .checked_add(1)
            .ok_or(Error::ProjectionOverflow)?
            <= self.nfa_table.len() / 2
        {
            Ok(())
        } else {
            Err(Error::InvalidEvidence)
        }
    }
    fn intern_nfa(
        &mut self,
        key: NfaKey,
        guard: &mut impl FnMut(u128) -> Result<(), Error>,
    ) -> Result<usize, Error> {
        self.ensure_nfa_table()?;
        let mut slot = hash(&key) & (self.nfa_table.len() - 1);
        while self.nfa_table[slot] != 0 {
            let index = self.nfa_table[slot] - 1;
            if self.nfa[index].key == key {
                return Ok(index);
            }
            slot = (slot + 1) & (self.nfa_table.len() - 1);
        }
        let base = self.owned()?;
        reserve(&mut self.nfa, 1, base, self.limits, guard)?;
        let index = self.nfa.len();
        self.nfa.push(NfaNode {
            key,
            accepting: false,
            edges: Vec::new(),
            raw_count: 0,
        });
        self.nfa_table[slot] = index + 1;
        Ok(index)
    }
    fn ensure_dfa_table(&self) -> Result<(), Error> {
        if self
            .dfa
            .len()
            .checked_add(1)
            .ok_or(Error::ProjectionOverflow)?
            <= self.dfa_table.len() / 2
        {
            Ok(())
        } else {
            Err(Error::InvalidEvidence)
        }
    }
    // Consumes the already-accounted temporary subset without cloning it.
    fn intern_subset(
        &mut self,
        guard: &mut impl FnMut(u128) -> Result<(), Error>,
    ) -> Result<usize, Error> {
        self.ensure_dfa_table()?;
        let mut slot = hash(&self.subset) & (self.dfa_table.len() - 1);
        while self.dfa_table[slot] != 0 {
            let index = self.dfa_table[slot] - 1;
            if self.dfa[index].members == self.subset {
                self.subset.clear();
                return Ok(index);
            }
            slot = (slot + 1) & (self.dfa_table.len() - 1);
        }
        let base = self.owned()?;
        reserve(&mut self.dfa, 1, base, self.limits, guard)?;
        let accepting = self
            .subset
            .iter()
            .copied()
            .find(|&id| self.nfa[id].accepting);
        self.nested_bytes = self
            .nested_bytes
            .checked_add(bytes(&self.subset)?)
            .ok_or(Error::ProjectionOverflow)?;
        let members = std::mem::take(&mut self.subset);
        let index = self.dfa.len();
        self.dfa.push(DfaNode {
            members,
            accepting,
            edges: Vec::new(),
            count: 0,
        });
        self.dfa_table[slot] = index + 1;
        Ok(index)
    }

    fn step(&mut self, guard: &mut impl FnMut(u128) -> Result<(), Error>) -> Result<(), Error> {
        if self.maintain_tables(guard)? {
            return Ok(());
        }
        match self.phase {
            Phase::Roots => {
                if self.index == self.locations.len() {
                    self.switch(Phase::Nfa);
                    return Ok(());
                }
                let location = self.locations[self.index];
                let batch = self
                    .batches
                    .get(location.batch)
                    .ok_or(Error::InvalidEvidence)?;
                let graph = batch
                    .graphs()
                    .get(location.graph)
                    .ok_or(Error::InvalidEvidence)?;
                let first = self.locations[0];
                let first_batch = self
                    .batches
                    .get(first.batch)
                    .ok_or(Error::InvalidEvidence)?;
                let first_graph = first_batch
                    .graphs()
                    .get(first.graph)
                    .ok_or(Error::InvalidEvidence)?;
                if !batch.complete()
                    || self.pattern_id >= batch.patterns().len()
                    || graph.node(graph.root()).is_none()
                    || graph.candidate_id() != first_graph.candidate_id()
                    || graph.identity() != first_graph.identity()
                    || batch.layout() != first_batch.layout()
                    || batch.initial_occupied() != first_batch.initial_occupied()
                    || batch.initial_cursor() != first_batch.initial_cursor()
                    || batch.initial_hold() != first_batch.initial_hold()
                    || batch.patterns().get(self.pattern_id)
                        != first_batch.patterns().get(self.pattern_id)
                    || batch.hold_enabled() != first_batch.hold_enabled()
                    || batch.projects_unplaced_lookahead()
                        != first_batch.projects_unplaced_lookahead()
                    || batch.projects_standard_bag_lookahead()
                        != first_batch.projects_standard_bag_lookahead()
                    || batch.kick_table_id() != first_batch.kick_table_id()
                    || batch.rule_profile_id() != first_batch.rule_profile_id()
                    || batch.initial_occupied() & !batch.layout().all_cells_mask() != 0
                {
                    return Err(Error::InvalidEvidence);
                }
                let key = NfaKey {
                    location: self.index,
                    supply: SupplyState {
                        node: graph.root(),
                        cursor: batch.initial_cursor(),
                        hold: batch.initial_hold(),
                    },
                    board: batch.initial_occupied(),
                    depth: 0,
                };
                let root = self.intern_nfa(key, guard)?;
                let base = self.owned()?;
                reserve(&mut self.roots, 1, base, self.limits, guard)?;
                self.roots.push(root);
                self.index += 1;
            }
            Phase::Nfa => self.expand_nfa(guard)?,
            Phase::NfaCount => {
                if self.index == self.nfa.len() {
                    if self.locations.len() == 1 && self.deterministic {
                        let count = self.nfa[self.roots[0]].raw_count;
                        if count > self.limits.max_executions() {
                            return Err(Error::ExecutionLimitExceeded {
                                max_executions: self.limits.max_executions(),
                            });
                        }
                        self.fast = true;
                        self.count = Some(count);
                        self.phase = Phase::Complete;
                        return Ok(());
                    }
                    self.switch(Phase::DfaRoots);
                    return Ok(());
                }
                let id = self.nfa.len() - 1 - self.index;
                if self.edge_index == 0 {
                    self.sum = usize::from(self.nfa[id].accepting);
                }
                if let Some(edge) = self.nfa[id].edges.get(self.edge_index) {
                    if edge.destination <= id {
                        return Err(Error::InvalidEvidence);
                    }
                    self.sum = self
                        .sum
                        .checked_add(self.nfa[edge.destination].raw_count)
                        .ok_or(Error::ProjectionOverflow)?;
                    self.nfa[id].edges[self.edge_index].end_count = self.sum;
                    self.edge_index += 1;
                } else {
                    self.nfa[id].raw_count = self.sum;
                    self.index += 1;
                    self.edge_index = 0;
                }
            }
            Phase::DfaRoots => {
                if self.index > self.roots.len() {
                    self.switch(Phase::Dfa);
                    return Ok(());
                }
                // The legacy accumulator deduplicates inside each location,
                // and across locations only after its last raw-count check.
                // Keep singleton roots for that cap, and a final union root.
                let members = if self.index < self.roots.len() {
                    1
                } else {
                    self.roots.len()
                };
                if self.member_index < members {
                    let base = self.owned()?;
                    reserve(&mut self.subset, 1, base, self.limits, guard)?;
                    self.subset.push(
                        self.roots[if self.index < self.roots.len() {
                            self.index
                        } else {
                            self.member_index
                        }],
                    );
                    self.member_index += 1;
                } else {
                    let root = self.intern_subset(guard)?;
                    let base = self.owned()?;
                    reserve(&mut self.prefix_roots, 1, base, self.limits, guard)?;
                    self.prefix_roots.push(root);
                    self.index += 1;
                    self.member_index = 0;
                }
            }
            Phase::Dfa => self.expand_dfa(guard)?,
            Phase::DfaCount => {
                if self.index == self.dfa.len() {
                    self.switch(Phase::CheckCaps);
                    return Ok(());
                }
                let id = self.dfa.len() - 1 - self.index;
                if self.edge_index == 0 {
                    self.sum = usize::from(self.dfa[id].accepting.is_some());
                }
                if let Some(edge) = self.dfa[id].edges.get(self.edge_index) {
                    if edge.destination <= id {
                        return Err(Error::InvalidEvidence);
                    }
                    self.sum = self
                        .sum
                        .checked_add(self.dfa[edge.destination].count)
                        .ok_or(Error::ProjectionOverflow)?;
                    self.dfa[id].edges[self.edge_index].end_count = self.sum;
                    self.edge_index += 1;
                } else {
                    self.dfa[id].count = self.sum;
                    self.index += 1;
                    self.edge_index = 0;
                }
            }
            Phase::CheckCaps => {
                if self.index == self.roots.len() {
                    self.count = Some(
                        self.dfa[*self.prefix_roots.last().ok_or(Error::InvalidEvidence)?].count,
                    );
                    self.phase = Phase::Complete;
                    return Ok(());
                }
                let raw = self.nfa[self.roots[self.index]].raw_count;
                let combined = self.sum.checked_add(raw).ok_or(Error::ProjectionOverflow)?;
                if combined > self.limits.max_executions() {
                    return Err(Error::ExecutionLimitExceeded {
                        max_executions: self.limits.max_executions(),
                    });
                }
                self.sum = self
                    .sum
                    .checked_add(self.dfa[self.prefix_roots[self.index]].count)
                    .ok_or(Error::ProjectionOverflow)?;
                self.index += 1;
            }
            Phase::Complete => (),
            Phase::Failed => return Err(Error::InvalidEvidence),
        }
        admit(self.limits, self.owned()?, guard)
    }

    fn expand_nfa(
        &mut self,
        guard: &mut impl FnMut(u128) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if self.index == self.nfa.len() {
            self.switch(Phase::NfaCount);
            return Ok(());
        }
        let key = self.nfa[self.index].key;
        let location = self.locations[key.location];
        let batch = &self.batches[location.batch];
        let graph = &batch.graphs()[location.graph];
        let node = graph.node(key.supply.node).ok_or(Error::InvalidEvidence)?;
        let edges = graph.checked_edges(node).ok_or(Error::InvalidEvidence)?;
        let sequence = &batch.patterns()[self.pattern_id];
        if node.accepting() {
            let accepted = terminal_supply_state_is_accepted(batch, sequence, key.supply);
            // The existing public projection starts at synthetic cursor zero
            // and hold None. Do not count an unshown trace that the unchanged
            // PC chain validator would reject against a nondefault query start.
            if accepted
                && (key.depth == 0
                    || key.board != 0
                    || batch.initial_cursor() != 0
                    || batch.initial_hold().is_some())
            {
                return Err(Error::InvalidEvidence);
            }
            self.nfa[self.index].accepting = accepted;
            self.index += 1;
            self.edge_index = 0;
            return Ok(());
        }
        let Some(&edge) = edges.get(self.edge_index) else {
            self.index += 1;
            self.edge_index = 0;
            return Ok(());
        };
        if edge.to() <= key.supply.node || graph.node(edge.to()).is_none() {
            return Err(Error::InvalidEvidence);
        }
        let mut branches = [None; 4];
        let mut length = 0;
        if batch.projects_unplaced_lookahead()
            && batch.hold_enabled()
            && usize::from(key.supply.cursor) == sequence.len()
            && key.supply.hold == Some(edge.piece())
            && graph
                .node(edge.to())
                .ok_or(Error::InvalidEvidence)?
                .accepting()
            && (!batch.projects_standard_bag_lookahead()
                || first_standard_bag_lookahead(sequence).is_none())
        {
            branches[length] = Some((
                HoldDecision::ReleaseHeldAtTerminal {
                    held_piece: edge.piece(),
                },
                SupplyState {
                    node: edge.to(),
                    cursor: key
                        .supply
                        .cursor
                        .checked_add(1)
                        .ok_or(Error::ProjectionOverflow)?,
                    hold: key.supply.hold,
                },
            ));
            length += 1;
        }
        for_each_supply_successor(batch, sequence, key.supply, edge.piece(), |hold, next| {
            if length == branches.len() {
                return Err(super::ExactScoringExecutionCancelled);
            }
            branches[length] = Some((
                hold,
                SupplyState {
                    node: edge.to(),
                    ..next
                },
            ));
            length += 1;
            Ok(())
        })
        .map_err(|_| Error::InvalidEvidence)?;
        if length != 0 {
            if key.depth >= self.limits.max_path_steps() {
                return Err(Error::PathStepLimitExceeded {
                    max_path_steps: self.limits.max_path_steps(),
                });
            }
            let (mask, board) =
                SolutionTraceBuilder::project_scoring_step(batch.layout(), key.board, edge)
                    .ok_or(Error::InvalidEvidence)?;
            for branch in branches.into_iter().take(length) {
                let (hold, supply) = branch.ok_or(Error::InvalidEvidence)?;
                let label = Label::new(key.depth, edge, hold, mask)?;
                let destination = self.intern_nfa(
                    NfaKey {
                        location: key.location,
                        supply,
                        board,
                        depth: key.depth.checked_add(1).ok_or(Error::ProjectionOverflow)?,
                    },
                    guard,
                )?;
                let base = self.owned()?;
                let delta = reserve(&mut self.nfa[self.index].edges, 1, base, self.limits, guard)?;
                self.nested_bytes = self
                    .nested_bytes
                    .checked_add(delta)
                    .ok_or(Error::ProjectionOverflow)?;
                let position = self.nfa[self.index]
                    .edges
                    .partition_point(|old| old.label < label);
                if self.nfa[self.index]
                    .edges
                    .get(position)
                    .is_some_and(|old| old.label == label)
                {
                    self.deterministic = false;
                }
                self.nfa[self.index].edges.insert(
                    position,
                    NfaEdge {
                        label,
                        destination,
                        edge,
                        hold,
                        end_count: 0,
                    },
                );
            }
        }
        self.edge_index += 1;
        Ok(())
    }

    fn expand_dfa(
        &mut self,
        guard: &mut impl FnMut(u128) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if self.index == self.dfa.len() {
            self.switch(Phase::DfaCount);
            return Ok(());
        }
        // Collect one primitive transition at a time in canonical order. Binary
        // insertion uses bounded allocated state, never recursively expands paths.
        if let Some(&member) = self.dfa[self.index].members.get(self.member_index) {
            if let Some(edge) = self.nfa[member].edges.get(self.edge_index) {
                let choice = Choice {
                    label: edge.label,
                    destination: edge.destination,
                    predecessor: Predecessor {
                        source: member,
                        edge: self.edge_index,
                    },
                };
                if let Err(position) = self.choices.binary_search(&choice) {
                    let base = self.owned()?;
                    reserve(&mut self.choices, 1, base, self.limits, guard)?;
                    self.choices.insert(position, choice);
                }
                self.edge_index += 1;
            } else {
                self.member_index += 1;
                self.edge_index = 0;
            }
            return Ok(());
        }
        if self.group_end < self.choices.len() {
            let choice = self.choices[self.group_end];
            let base = self.owned()?;
            reserve(&mut self.subset, 1, base, self.limits, guard)?;
            self.subset.push(choice.destination);
            let base = self.owned()?;
            reserve(&mut self.predecessors, 1, base, self.limits, guard)?;
            self.predecessors.push(choice.predecessor);
            self.group_end += 1;
            if self.group_end == self.choices.len()
                || self.choices[self.group_end].label != choice.label
            {
                let destination = self.intern_subset(guard)?;
                let base = self.owned()?;
                let delta = reserve(&mut self.dfa[self.index].edges, 1, base, self.limits, guard)?;
                self.nested_bytes = self
                    .nested_bytes
                    .checked_add(delta)
                    .ok_or(Error::ProjectionOverflow)?;
                self.nested_bytes = self
                    .nested_bytes
                    .checked_add(bytes(&self.predecessors)?)
                    .ok_or(Error::ProjectionOverflow)?;
                self.dfa[self.index].edges.push(DfaEdge {
                    label: choice.label,
                    destination,
                    end_count: 0,
                    predecessors: std::mem::take(&mut self.predecessors),
                });
            }
            return Ok(());
        }
        self.choices.clear();
        self.index += 1;
        self.member_index = 0;
        self.edge_index = 0;
        self.group_end = 0;
        Ok(())
    }
}

// ReplayEngine's compatibility projection creates at most 12 events/placement,
// two operation/order/hold generations, three cell-owner working buffers and
// one cleared-owner vector/placement. Four times each requested growable buffer
// admits old/new capacity overlap; fixed structs/variant strings get a separate
// 4096-byte reserve. This bound is only for the selected <=max_path_steps trace,
// never for the number of paths in the language. Retained output is checked again.
fn replay_projection(steps: usize, cells: usize) -> Result<u128, Error> {
    use clearra_replay::{
        BuildVariantOperation, CellOwner, ColoredCellOwner, KickEvidenceEvent,
        MovementEvidenceEvent, PlacementStep, ReplayEvent,
    };
    let per_step = 4usize
        .checked_mul(
            size_of::<BuildVariantOperation>()
                + 4 * size_of::<usize>()
                + 4 * size_of::<HoldDecision>()
                + 2 * size_of::<MovementEvidenceEvent>()
                + 2 * size_of::<KickEvidenceEvent>()
                + size_of::<PlacementStep>(),
        )
        .and_then(|n| n.checked_add(48 * size_of::<ReplayEvent>()))
        .and_then(|n| n.checked_add(cells.checked_mul(4 * size_of::<CellOwner>())?))
        .and_then(|n| n.checked_add(4 * 193))
        .ok_or(Error::ProjectionOverflow)?;
    (steps as u128)
        .checked_mul(per_step as u128)
        .and_then(|n| {
            n.checked_add((cells as u128).checked_mul(
                (4 * size_of::<Option<CellOwner>>() + 4 * size_of::<Option<ColoredCellOwner>>())
                    as u128,
            )?)
        })
        .and_then(|n| n.checked_add(4096))
        .ok_or(Error::ProjectionOverflow)
}

#[cfg(test)]
#[path = "exact_replay_language_tests.rs"]
mod tests;
