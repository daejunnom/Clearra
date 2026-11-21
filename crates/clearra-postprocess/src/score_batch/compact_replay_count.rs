//! Experimental count/rank index for a fresh standard-bag ordinal space.
//! A visible label determines its consumed queue symbols. Determinize only
//! equal visible labels and then add their guarded suffix weight functions.

use std::collections::HashMap;

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_replay::{ExactScoringExecutionBatch, HoldDecision, PieceDecision};
use clearra_supply::{
    bag::bag_state::BagState,
    execution_automaton::{
        SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionState, SupplyExecutionStep,
        SupplyObservationIdentity, SupplyProvenanceId,
    },
    hold::hold_policy::HoldPolicy,
    pattern_universe::materialized_pattern_universe::{
        MaterializedPatternUniverse, UniformCompactPatternSource,
    },
    piece_source::{PieceSourceId, PieceSourceKind},
};

use super::{
    compact_replay_language_domain::CompactReplayDomain,
    exact_replay_language::{ExactReplayGraphLocation, Label},
    replay_language_domain::{ReplayLanguageDomain, ReplayLanguageGraph},
    ExactReplayMaterializationError as Error,
};

const WIDTH: usize = PieceKind::STANDARD_TETROMINOES.len();
const FULL_BAG: u8 = (1 << WIDTH) - 1;
const MAX_QUEUED: u16 = 11;
// Even a bypass-only caller cannot create an arbitrarily large experiment.
const MAX_WORK: usize = 65_536;
type NodeId = usize;
const ZERO: NodeId = 0;
type CountResult<T> = Result<T, CountError>;

#[derive(Debug, Eq, PartialEq)]
pub(super) enum CountError {
    Unsupported,
    WorkBudget,
    Replay(Error),
}

impl From<Error> for CountError {
    fn from(error: Error) -> Self {
        Self::Replay(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum NodeKey {
    Leaf(u128),
    Branch([NodeId; WIDTH]),
}

#[derive(Clone, Copy, Debug)]
struct CountNode {
    key: NodeKey,
    total: u128,
    nonempty: u128,
    maximum: u128,
}

#[derive(Debug)]
pub(super) struct CompactBagReplayCount {
    nodes: Vec<CountNode>,
    root: NodeId,
    queue_len: u16,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct CountedPatternRank {
    pub(super) ordinal: usize,
    pub(super) within_pattern: u128,
    pub(super) pattern_count: u128,
}

/// The prototype limits the work that can allocate. Product-grade byte
/// admission (including the co-owned source) is a separate caller contract
/// and intentionally is not offered by this module.
#[derive(Clone, Copy)]
pub(super) struct CountLimits {
    pub(super) work: usize,
    pub(super) raw_cell_paths: usize,
    pub(super) path_steps: usize,
}

impl CompactBagReplayCount {
    pub(super) fn total(&self) -> u128 {
        self.nodes[self.root].total
    }

    pub(super) fn nonempty_patterns(&self) -> u128 {
        self.nodes[self.root].nonempty
    }

    /// Queue-major rank; excluded siblings still contribute their full
    /// original-universe cardinality to the absolute ordinal.
    pub(super) fn select_pattern(&self, mut rank: u128) -> Option<CountedPatternRank> {
        if rank >= self.total() {
            return None;
        }
        let mut node = self.root;
        let mut ordinal = 0usize;
        let mut remaining = FULL_BAG;
        for cursor in 0..self.queue_len {
            if remaining == 0 {
                remaining = FULL_BAG;
            }
            let NodeKey::Branch(children) = self.nodes[node].key else {
                return None;
            };
            let mut child = None;
            for (slot, child_node) in children.into_iter().enumerate() {
                let bit = 1 << slot;
                if remaining & bit == 0 {
                    continue;
                }
                let suffix = remaining ^ bit;
                let count = self.nodes[child_node].total;
                if rank < count {
                    child = Some(child_node);
                    remaining = suffix;
                    break;
                }
                rank -= count;
                ordinal += queue_suffixes(self.queue_len - cursor - 1, suffix.count_ones() as u16);
            }
            node = child?;
        }
        match self.nodes[node].key {
            NodeKey::Leaf(count) if rank < count => Some(CountedPatternRank {
                ordinal,
                within_pattern: rank,
                pattern_count: count,
            }),
            _ => None,
        }
    }
}

/// Only used under MAX_QUEUED; the largest supported universe is 4,233,600.
fn queue_suffixes(mut len: u16, mut available: u16) -> usize {
    let mut count = 1;
    while len != 0 {
        if available == 0 {
            available = WIDTH as u16;
        }
        count *= usize::from(available);
        available -= 1;
        len -= 1;
    }
    count
}

fn piece_slot(piece: PieceKind) -> usize {
    PieceKind::STANDARD_TETROMINOES
        .iter()
        .position(|&item| item == piece)
        .expect("a standard mino has an ordinal slot")
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Member {
    location: usize,
    node: u32,
    board: u64,
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct ProductKey {
    frontier: Vec<Member>,
    supply: SupplyExecutionState,
    depth: usize,
    raw: bool,
}

#[derive(Clone, Copy)]
struct Choice {
    label: Label,
    child: Member,
    next: SupplyExecutionState,
    draws: [Option<PieceKind>; 2],
}

#[derive(Default)]
struct Arena {
    nodes: Vec<CountNode>,
    interner: HashMap<NodeKey, NodeId>,
    sums: HashMap<(NodeId, NodeId), NodeId>,
}

struct Budget<'a> {
    work: usize,
    control: &'a ExecutionControl,
}

impl Budget<'_> {
    fn spend(&mut self, work: usize) -> CountResult<()> {
        if self.control.is_cancelled() {
            return Err(Error::Cancelled.into());
        }
        self.work = self.work.checked_sub(work).ok_or(CountError::WorkBudget)?;
        Ok(())
    }
}

fn reserve<T>(values: &mut Vec<T>, count: usize) -> CountResult<()> {
    values
        .try_reserve(count)
        .map_err(|_| Error::AllocationFailed.into())
}

impl Arena {
    fn intern(&mut self, key: NodeKey, budget: &mut Budget<'_>) -> CountResult<NodeId> {
        budget.spend(1)?;
        if let Some(&id) = self.interner.get(&key) {
            return Ok(id);
        }
        let (total, nonempty, maximum) = match key {
            NodeKey::Leaf(value) => (value, u128::from(value != 0), value),
            NodeKey::Branch(children) => {
                if children.iter().all(|&id| id == ZERO) {
                    return Ok(ZERO);
                }
                let mut total = 0_u128;
                let mut nonempty = 0_u128;
                let mut maximum = 0;
                for id in children {
                    let value = self.nodes[id];
                    total = total
                        .checked_add(value.total)
                        .ok_or(Error::ProjectionOverflow)?;
                    nonempty = nonempty
                        .checked_add(value.nonempty)
                        .ok_or(Error::ProjectionOverflow)?;
                    maximum = maximum.max(value.maximum);
                }
                (total, nonempty, maximum)
            }
        };
        let id = self.nodes.len();
        reserve(&mut self.nodes, 1)?;
        self.interner
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        self.nodes.push(CountNode {
            key,
            total,
            nonempty,
            maximum,
        });
        self.interner.insert(key, id);
        Ok(id)
    }

    fn sum(&mut self, left: NodeId, right: NodeId, budget: &mut Budget<'_>) -> CountResult<NodeId> {
        budget.spend(1)?;
        match (left, right) {
            (ZERO, other) | (other, ZERO) => return Ok(other),
            _ => {}
        }
        let key = (left.min(right), left.max(right));
        if let Some(&id) = self.sums.get(&key) {
            return Ok(id);
        }
        let node = match (self.nodes[left].key, self.nodes[right].key) {
            (NodeKey::Leaf(a), NodeKey::Leaf(b)) => {
                NodeKey::Leaf(a.checked_add(b).ok_or(Error::ProjectionOverflow)?)
            }
            (NodeKey::Branch(a), NodeKey::Branch(b)) => {
                let mut children = [ZERO; WIDTH];
                for slot in 0..WIDTH {
                    children[slot] = self.sum(a[slot], b[slot], budget)?;
                }
                NodeKey::Branch(children)
            }
            _ => return Err(Error::InvalidEvidence.into()),
        };
        let id = self.intern(node, budget)?;
        self.sums
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        self.sums.insert(key, id);
        Ok(id)
    }

    fn guard(
        &mut self,
        draws: [Option<PieceKind>; 2],
        mut child: NodeId,
        budget: &mut Budget<'_>,
    ) -> CountResult<NodeId> {
        for piece in draws.into_iter().rev().flatten() {
            let mut children = [ZERO; WIDTH];
            children[piece_slot(piece)] = child;
            child = self.intern(NodeKey::Branch(children), budget)?;
        }
        Ok(child)
    }
}

struct Compiler<'a> {
    batches: &'a [ExactScoringExecutionBatch],
    locations: &'a [ExactReplayGraphLocation],
    queue_len: u16,
    limits: CountLimits,
    budget: Budget<'a>,
    supply_automaton: SupplyExecutionAutomaton,
    arena: Arena,
    products: HashMap<ProductKey, NodeId>,
    tails: HashMap<(u16, u64), NodeId>,
}

impl Compiler<'_> {
    fn all_tails(&mut self, supply: SupplyExecutionState) -> CountResult<NodeId> {
        self.budget.spend(1)?;
        if supply.cursor() == self.queue_len {
            return self.arena.intern(NodeKey::Leaf(1), &mut self.budget);
        }
        let key = (supply.cursor(), supply.bag_remainder_key());
        if let Some(&id) = self.tails.get(&key) {
            return Ok(id);
        }
        let mut children = [ZERO; WIDTH];
        for (slot, piece) in PieceKind::STANDARD_TETROMINOES.into_iter().enumerate() {
            let child = self
                .supply_automaton
                .advance_bag_cursor(supply, piece)
                .map_err(|_| Error::InvalidEvidence)?;
            if let Some(child) = child {
                children[slot] = self.all_tails(child)?;
            }
        }
        let id = self
            .arena
            .intern(NodeKey::Branch(children), &mut self.budget)?;
        self.tails
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        self.tails.insert(key, id);
        Ok(id)
    }

    fn choices(&mut self, key: &ProductKey) -> CountResult<(bool, Vec<Choice>)> {
        self.budget.spend(key.frontier.len())?;
        let mut accepting = false;
        let mut choices = Vec::new();
        let mut supply: [Option<[Option<SupplyExecutionStep>; WIDTH + 1]>; WIDTH] = [None; WIDTH];
        for member in &key.frontier {
            let location = self.locations[member.location];
            let batch = &self.batches[location.batch];
            let graph = &batch.graphs()[location.graph];
            let node = graph.node(member.node).ok_or(Error::InvalidEvidence)?;
            let edges = graph.checked_edges(node).ok_or(Error::InvalidEvidence)?;
            if node.accepting() {
                CompactReplayDomain::terminal(member.board, key.depth)?;
                accepting = true;
                continue;
            }
            for &edge in edges {
                self.budget.spend(1)?;
                if edge.to() <= member.node || graph.node(edge.to()).is_none() {
                    return Err(Error::InvalidEvidence.into());
                }
                let slot = piece_slot(edge.piece());
                let steps = if let Some(steps) = supply[slot].as_ref() {
                    steps
                } else {
                    let mut steps = [None; WIDTH + 1];
                    let mut count = 0;
                    self.supply_automaton
                        .for_each_matching_bag_step(key.supply, edge.piece(), |step| {
                            if step.next_state.cursor() <= self.queue_len {
                                // Current plus at most one hold branch per incoming kind.
                                steps[count] = Some(step);
                                count += 1;
                            }
                        })
                        .map_err(|_| Error::InvalidEvidence)?;
                    supply[slot] = Some(steps);
                    supply[slot]
                        .as_ref()
                        .expect("piece transitions were prepared")
                };
                if steps[0].is_none() {
                    continue;
                }
                if key.depth >= self.limits.path_steps {
                    return Err(Error::PathStepLimitExceeded {
                        max_path_steps: self.limits.path_steps,
                    }
                    .into());
                }
                let (mask, board) = CompactReplayDomain::transition(batch, member.board, edge)?;
                for step in steps.iter().flatten() {
                    self.budget.spend(1)?;
                    let (decision, draws) = decision(key.supply, *step)?;
                    reserve(&mut choices, 1)?;
                    choices.push(Choice {
                        label: CompactReplayDomain::label(edge, decision, mask)?,
                        child: Member {
                            location: member.location,
                            node: edge.to(),
                            board,
                        },
                        next: step.next_state,
                        draws,
                    });
                }
            }
        }
        Ok((accepting, choices))
    }

    fn product(&mut self, key: ProductKey) -> CountResult<NodeId> {
        self.budget.spend(1)?;
        if let Some(&id) = self.products.get(&key) {
            return Ok(id);
        }
        let (accepting, mut choices) = self.choices(&key)?;
        let mut total = if accepting {
            self.all_tails(key.supply)?
        } else {
            ZERO
        };
        if !key.raw {
            choices.sort_unstable_by(|a, b| a.label.cmp(&b.label).then(a.child.cmp(&b.child)));
        }
        let mut start = 0;
        while start < choices.len() {
            let first = choices[start];
            let mut end = start + 1;
            if !key.raw {
                while end < choices.len() && choices[end].label == first.label {
                    end += 1;
                }
            }
            self.budget.spend(end - start)?;
            let mut frontier = Vec::new();
            reserve(&mut frontier, end - start)?;
            for entry in &choices[start..end] {
                if entry.draws != first.draws || entry.next != first.next {
                    return Err(Error::InvalidEvidence.into());
                }
                if frontier.last() != Some(&entry.child) {
                    frontier.push(entry.child);
                }
            }
            let child = self.product(ProductKey {
                frontier,
                supply: first.next,
                depth: key.depth + 1,
                raw: key.raw,
            })?;
            let child = self.arena.guard(first.draws, child, &mut self.budget)?;
            total = self.arena.sum(total, child, &mut self.budget)?;
            start = end;
        }
        self.products
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        self.products.insert(key, total);
        Ok(total)
    }
}

fn decision(
    supply: SupplyExecutionState,
    step: SupplyExecutionStep,
) -> CountResult<(PieceDecision, [Option<PieceKind>; 2])> {
    let evidence = step.evidence;
    let incoming = evidence.queue_current_piece;
    let hold = match evidence.branch_kind {
        SupplyBranchKind::Current => HoldDecision::None,
        SupplyBranchKind::SwapHeld => HoldDecision::SwapWithHold {
            incoming_piece: incoming,
            held_piece: step.used_piece,
        },
        SupplyBranchKind::StoreCurrent => HoldDecision::StoreIncoming {
            stored_piece: incoming,
            drawn_piece: step.used_piece,
        },
    };
    let decision = PieceDecision::from_selected_hold(
        step.used_piece,
        usize::from(supply.cursor()),
        supply.hold_piece(),
        hold,
    )
    .ok_or(Error::InvalidEvidence)?;
    Ok((decision, [Some(incoming), evidence.queue_next_piece]))
}

/// Bounded prototype, not a product admission path. The count domain is the
/// verified storage of the supplied compact universe, not a shape label or
/// dense vectors on a replay batch. Binding these graphs to that universe is
/// the caller's contract. Nonzero cursors and projected lookahead are excluded.
pub(super) fn compile_compact_bag_replay_count(
    batches: &[ExactScoringExecutionBatch],
    locations: &[ExactReplayGraphLocation],
    universe: &MaterializedPatternUniverse,
    limits: CountLimits,
    control: &ExecutionControl,
) -> CountResult<CompactBagReplayCount> {
    let Some(UniformCompactPatternSource::Standard7Bag {
        sequence_len,
        pattern_count,
    }) = universe.uniform_compact_source()
    else {
        return Err(CountError::Unsupported);
    };
    let queue_len = u16::try_from(sequence_len).map_err(|_| CountError::Unsupported)?;
    if !(1..=MAX_QUEUED).contains(&queue_len) || locations.is_empty() {
        return Err(CountError::Unsupported);
    }
    if !universe.complete() || pattern_count != queue_suffixes(queue_len, WIDTH as u16) {
        return Err(CountError::Unsupported);
    }
    let mut budget = Budget {
        work: limits.work.min(MAX_WORK),
        control,
    };
    budget.spend(locations.len())?;
    let batch_at = |location: ExactReplayGraphLocation| {
        let batch = batches.get(location.batch).ok_or(Error::InvalidEvidence)?;
        let graph = batch
            .graphs()
            .get(location.graph)
            .ok_or(Error::InvalidEvidence)?;
        Ok::<_, Error>((batch, graph))
    };
    let (first, first_graph) = batch_at(locations[0])?;
    if first.initial_cursor() != 0 || first.projects_unplaced_lookahead() {
        return Err(CountError::Unsupported);
    }
    let mut roots = Vec::new();
    reserve(&mut roots, locations.len())?;
    for (position, &location) in locations.iter().enumerate() {
        let (batch, graph) = batch_at(location)?;
        if !batch.complete()
            || batch.initial_cursor() != 0
            || batch.initial_hold() != first.initial_hold()
            || batch.hold_enabled() != first.hold_enabled()
            || batch.projects_unplaced_lookahead()
            || batch.projects_standard_bag_lookahead()
            || batch.kick_table_id() != first.kick_table_id()
            || batch.rule_profile_id() != first.rule_profile_id()
            || !graph.same_candidate(first_graph)
            || !CompactReplayDomain::same_frame(batch, first)
        {
            return Err(Error::InvalidEvidence.into());
        }
        roots.push(Member {
            location: position,
            node: graph.root(),
            board: CompactReplayDomain::initial(batch, graph)?,
        });
    }
    let supply = SupplyExecutionState::with_contract(
        PieceSourceId::new(1),
        PieceSourceKind::BagUniverse,
        0,
        first.initial_hold(),
        if first.hold_enabled() {
            HoldPolicy::Allowed
        } else {
            HoldPolicy::Forbidden
        },
        0,
        BagState::fresh_standard_7_bag().packed_remainder_key(),
        SupplyObservationIdentity::full_queue_oracle(),
        SupplyProvenanceId(1),
    );
    let supply_automaton = SupplyExecutionAutomaton::for_bag(&PieceKind::STANDARD_TETROMINOES)
        .map_err(|_| Error::InvalidEvidence)?;
    let mut compiler = Compiler {
        batches,
        locations,
        queue_len,
        limits,
        budget,
        supply_automaton,
        arena: Arena::default(),
        products: HashMap::new(),
        tails: HashMap::new(),
    };
    let zero = compiler
        .arena
        .intern(NodeKey::Leaf(0), &mut compiler.budget)?;
    debug_assert_eq!(zero, ZERO);
    let mut previous = ZERO;
    for &member in &roots {
        let singleton = |raw| {
            let mut frontier = Vec::new();
            reserve(&mut frontier, 1)?;
            frontier.push(member);
            Ok::<_, CountError>(ProductKey {
                frontier,
                supply,
                depth: 0,
                raw,
            })
        };
        let raw = compiler.product(singleton(true)?)?;
        let checked = compiler.arena.sum(previous, raw, &mut compiler.budget)?;
        if compiler.arena.nodes[checked].maximum > limits.raw_cell_paths as u128 {
            return Err(Error::ExecutionLimitExceeded {
                max_executions: limits.raw_cell_paths,
            }
            .into());
        }
        // Match the legacy per-cell cap: add each earlier location's
        // singleton distinct count before testing the next raw count.
        let distinct = compiler.product(singleton(false)?)?;
        previous = compiler
            .arena
            .sum(previous, distinct, &mut compiler.budget)?;
    }
    let root = compiler.product(ProductKey {
        frontier: roots,
        supply,
        depth: 0,
        raw: false,
    })?;
    Ok(CompactBagReplayCount {
        nodes: compiler.arena.nodes,
        root,
        queue_len,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clearra_core_domain::{
        board::board_size::BoardSize,
        execution_cancellation::ExecutionCancellationToken,
        piece::rotation::RotationState,
        solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
    };
    use clearra_geometry::layout::board64_layout::Board64Layout;
    use clearra_replay::{
        ExactScoringExecutionGraph, ScoringExecutionEdge, ScoringExecutionNode, ScoringLockEvidence,
    };
    use clearra_supply::pattern_universe::PatternUniverseMaterializer;

    use super::*;
    use crate::{ExactReplayLanguageSession, ExactReplayMaterializationLimits};

    const INITIAL: u64 = 0xcc;
    const OCCUPIES: [PiecePlacementMask; 2] = [
        PiecePlacementMask::new(PieceKind::I, 0xf00),
        PiecePlacementMask::new(PieceKind::O, 0x33),
    ];

    fn edge(
        target: u32,
        piece: PieceKind,
        rotation: RotationState,
        y: i8,
        clears: u8,
    ) -> ScoringExecutionEdge {
        ScoringExecutionEdge::new(
            target,
            u8::from(piece == PieceKind::O),
            piece,
            rotation,
            0,
            y,
            clears,
            0,
            0,
            ScoringLockEvidence::no_rotation(rotation),
        )
    }

    // I leaves its straight path independent of O. Two equal visible I
    // prefixes have both overlapping and different hidden O continuations.
    fn fixture_graph(orders: &[(bool, RotationState)]) -> ExactScoringExecutionGraph {
        let identity = StandardBoard64TilingIdentity::from_placements(INITIAL, OCCUPIES).unwrap();
        let len = orders.len() as u32;
        let mut edges = Vec::new();
        let mut nodes = vec![ScoringExecutionNode::new(0, len, false)];
        for (index, &(first_o, rot)) in orders.iter().enumerate() {
            edges.push(if first_o {
                edge(1 + index as u32, PieceKind::O, rot, 0, 2)
            } else {
                edge(1 + index as u32, PieceKind::I, RotationState::Zero, 2, 1)
            });
        }
        for (index, &(first_o, rot)) in orders.iter().enumerate() {
            nodes.push(ScoringExecutionNode::new(edges.len() as u32, 1, false));
            let target = 1 + len + index as u32;
            edges.push(if first_o {
                edge(target, PieceKind::I, RotationState::Zero, 0, 1)
            } else {
                edge(target, PieceKind::O, rot, 0, 2)
            });
        }
        nodes.extend((0..len).map(|_| ScoringExecutionNode::new(2 * len, 0, true)));
        ExactScoringExecutionGraph::new(17, identity, 0, nodes, edges)
    }

    fn batch(
        graphs: Vec<ExactScoringExecutionGraph>,
        patterns: Vec<Vec<PieceKind>>,
        hold: bool,
        held: Option<PieceKind>,
    ) -> ExactScoringExecutionBatch {
        ExactScoringExecutionBatch::new(
            Board64Layout::new(BoardSize::new(4, 3).unwrap()).unwrap(),
            INITIAL,
            patterns,
            0,
            held,
            hold,
            false,
            false,
            1,
            1,
            graphs,
            true,
        )
    }

    fn locations(graph_count: usize) -> Vec<ExactReplayGraphLocation> {
        (0..graph_count)
            .map(|graph| ExactReplayGraphLocation { batch: 0, graph })
            .collect()
    }

    fn limits(work: usize, raw_cell_paths: usize) -> CountLimits {
        CountLimits {
            work,
            raw_cell_paths,
            path_steps: 11,
        }
    }

    fn fixed_count(
        batches: Arc<[ExactScoringExecutionBatch]>,
        locations: &[ExactReplayGraphLocation],
        ordinal: usize,
        cap: usize,
    ) -> Result<usize, Error> {
        let mut session = ExactReplayLanguageSession::new(
            batches,
            locations.to_vec(),
            ordinal,
            ExactReplayMaterializationLimits::new(cap, 11, 64 * 1024 * 1024),
            &mut |_| Ok(()),
        )?;
        assert!(session.advance(32768, &ExecutionControl::default(), &mut |_| Ok(()))?);
        session.count().ok_or(Error::InvalidEvidence)
    }

    #[test]
    fn exact_counts_and_ranks_match_fixed_cells_with_hidden_unions_and_hold() {
        let universe = PatternUniverseMaterializer::standard_7_bag(3, 0, 1).unwrap();
        let patterns: Vec<_> = (0..universe.pattern_count())
            .map(|ordinal| universe.try_sequence_at(ordinal).unwrap().into_owned())
            .collect();
        for (hold, held) in [(false, None), (true, None), (true, Some(PieceKind::O))] {
            let batches: Arc<[ExactScoringExecutionBatch]> = vec![batch(
                vec![
                    fixture_graph(&[
                        (false, RotationState::Zero),
                        (false, RotationState::Right),
                        (true, RotationState::Zero),
                    ]),
                    fixture_graph(&[(false, RotationState::Zero), (false, RotationState::Zero)]),
                ],
                patterns.clone(),
                hold,
                held,
            )]
            .into();
            let loc = locations(2);
            let index = compile_compact_bag_replay_count(
                &batches,
                &loc,
                &universe,
                limits(MAX_WORK, 100),
                &ExecutionControl::default(),
            )
            .unwrap();
            let mut total = 0;
            let mut nonempty = 0;
            for ordinal in 0..universe.pattern_count() {
                let count = fixed_count(Arc::clone(&batches), &loc, ordinal, 100).unwrap() as u128;
                nonempty += u128::from(count != 0);
                for within in 0..count {
                    assert_eq!(
                        index.select_pattern(total + within),
                        Some(CountedPatternRank {
                            ordinal,
                            within_pattern: within,
                            pattern_count: count,
                        })
                    );
                }
                total += count;
            }
            assert!(nonempty > 1 && nonempty < universe.pattern_count() as u128);
            assert_eq!(index.total(), total);
            assert_eq!(index.nonempty_patterns(), nonempty);
            assert_eq!(index.select_pattern(total), None);
        }
    }

    #[test]
    fn full_two_bag_ordinal_suffixes_are_shared_without_materialized_queues() {
        let universe = PatternUniverseMaterializer::standard_7_bag(11, 0, 1).unwrap();
        assert_eq!(universe.pattern_count(), 4_233_600);
        let batches = [batch(
            vec![fixture_graph(&[(false, RotationState::Zero)])],
            Vec::new(),
            false,
            None,
        )];
        let index = compile_compact_bag_replay_count(
            &batches,
            &locations(1),
            &universe,
            limits(4096, 1),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(index.total(), 100_800);
        assert_eq!(index.nonempty_patterns(), index.total());
        assert!(index.nodes.len() < 256);
        for rank in [0, index.total() - 1] {
            let selected = index.select_pattern(rank).unwrap();
            assert_eq!(selected.ordinal as u128, rank);
            let queue = universe.try_sequence_at(selected.ordinal).unwrap();
            assert_eq!(&queue[..2], &[PieceKind::I, PieceKind::O]);
        }
    }

    #[test]
    fn raw_cell_cap_budget_and_cancellation_do_not_publish_partial_counts() {
        let universe = PatternUniverseMaterializer::standard_7_bag(2, 0, 1).unwrap();
        let patterns = (0..universe.pattern_count())
            .map(|ordinal| universe.try_sequence_at(ordinal).unwrap().into_owned())
            .collect();
        let batches: Arc<[ExactScoringExecutionBatch]> = vec![batch(
            vec![
                fixture_graph(&[(false, RotationState::Zero)]),
                fixture_graph(&[(false, RotationState::Zero), (false, RotationState::Zero)]),
            ],
            patterns,
            false,
            None,
        )]
        .into();
        let loc = locations(2);
        let expected = Error::ExecutionLimitExceeded { max_executions: 2 };
        assert_eq!(fixed_count(Arc::clone(&batches), &loc, 0, 2), Err(expected));
        assert_eq!(
            compile_compact_bag_replay_count(
                &batches,
                &loc,
                &universe,
                limits(MAX_WORK, 2),
                &ExecutionControl::default(),
            )
            .unwrap_err(),
            CountError::Replay(expected)
        );
        assert_eq!(
            compile_compact_bag_replay_count(
                &batches,
                &loc,
                &universe,
                limits(3, 3),
                &ExecutionControl::default(),
            )
            .unwrap_err(),
            CountError::WorkBudget
        );
        let token = ExecutionCancellationToken::new();
        token.handle().cancel();
        assert_eq!(
            compile_compact_bag_replay_count(
                &batches,
                &loc,
                &universe,
                limits(MAX_WORK, 3),
                &ExecutionControl::new(token),
            )
            .unwrap_err(),
            CountError::Replay(Error::Cancelled)
        );
        let clipped = PatternUniverseMaterializer::standard_7_bag(2, 1, 1).unwrap();
        assert_eq!(
            compile_compact_bag_replay_count(
                &batches,
                &loc,
                &clipped,
                limits(MAX_WORK, 3),
                &ExecutionControl::default(),
            )
            .unwrap_err(),
            CountError::Unsupported
        );
    }
}
