//! Optional necessary-condition index for fresh, empty-origin 4L P7P4.
//! This index never admits a replay. The existing exact cursor owns every
//! non-zero count, rank, witness, and manifest byte.
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_problem::{goal::SearchGoal, SearchProblem};
use clearra_replay::{
    trace::solution_trace_builder::SolutionTraceBuilder, ExactScoringExecutionBatch,
    ExactScoringExecutionGraph,
};
use clearra_supply::pattern_universe::MaterializedPatternUniverseStructure;

const HEAD: usize = 7;
const HEAD_COUNT: usize = 5_040;
const TAIL: usize = 4;
const TAIL_COUNT: usize = 840;
const QUEUE: usize = HEAD + TAIL;
const LOCKS: u32 = 10;
const PATTERN_COUNT: usize = HEAD_COUNT * TAIL_COUNT;
const ALL_PIECES: u8 = (1 << HEAD) - 1;
const TAIL_WORDS: usize = TAIL_COUNT.div_ceil(u64::BITS as usize);
const TAIL_CLASS_COUNT: usize = 35;
const TAIL_CLASS_BY_MASK: [u8; 1 << HEAD] = tail_classes();
// The compact Core producer has at most 2^n subset nodes. Unknown shapes,
// excess work, or any disagreement in the proof use the exact cursor.
const MAX_NODES: usize = 1 << LOCKS;
const MAX_EDGES: usize = 16_384;
const INDEX_VALIDATION_CHUNK: usize = 64;
const DEPTH_SHIFT: u32 = 28;

const P7_PATTERN_COUNT: usize = HEAD_COUNT;
const P7_PREFIX_BLOCK: usize = 120; // (7 - 2)!
const P7_PREFIX_COUNT: usize = HEAD * (HEAD - 1);
const P7_ALL_PREFIXES: u64 = (1 << P7_PREFIX_COUNT) - 1;
const MAX_P7_ROOT_EDGES: usize = 256;

/// Necessary first-lock availability for an authenticated, complete P7 axis.
/// A zero prefix bit only rules out a cell; the exact cursor still owns all
/// counts, projected witnesses, and ranking among the surviving cells.
pub(super) struct P7FirstLockIndex {
    hold_enabled: bool,
    prefixes: Option<u64>,
}

impl P7FirstLockIndex {
    pub(super) fn new(problem: &SearchProblem, batch: &ExactScoringExecutionBatch) -> Option<Self> {
        let ordinals = batch.standard_bag_ordinals()?;
        let source = problem.piece_source().materialized_universe()?;
        let supply = problem.supply();
        if ordinals.sequence_len() != HEAD
            || ordinals.len() != P7_PATTERN_COUNT
            || source.complete_standard_bag_ordinals() != Some(ordinals)
            || supply.source_sequence_length() != HEAD
            || !matches!(problem.search_goal(), SearchGoal::ClearToEmpty)
            || problem
                .queue_observation_policy()
                .requires_observation_policy()
            || problem
                .objective()
                .execution_constraints()
                .preserves_back_to_back()
            || problem.initial_hold().cursor() != 0
            || problem.initial_hold().hold_piece().is_some()
            || supply.projects_unplaced_lookahead()
            || !batch.complete()
            || batch.pattern_count() != P7_PATTERN_COUNT
            || batch.initial_cursor() != 0
            || batch.initial_hold().is_some()
            || batch.hold_enabled() != supply.hold_enabled()
            || batch.projects_unplaced_lookahead()
            || batch.projects_standard_bag_lookahead()
        {
            return None;
        }
        Some(Self {
            hold_enabled: batch.hold_enabled(),
            prefixes: None,
        })
    }

    pub(super) fn has_geometry(&self) -> bool {
        self.prefixes.is_some()
    }

    pub(super) fn clear_geometry(&mut self) {
        self.prefixes = None;
    }

    pub(super) fn prepare_geometry(
        &mut self,
        geometry: Option<(&ExactScoringExecutionBatch, &ExactScoringExecutionGraph)>,
    ) {
        self.prefixes = Some(
            geometry
                .and_then(|(batch, graph)| root_piece_mask(batch, graph))
                .map_or(P7_ALL_PREFIXES, |pieces| {
                    p7_prefix_mask(pieces, self.hold_enabled)
                }),
        );
    }

    pub(super) fn next_pattern(&self, ordinal: usize) -> usize {
        let Some(prefixes) = self.prefixes else {
            return ordinal;
        };
        if ordinal >= P7_PATTERN_COUNT {
            return P7_PATTERN_COUNT;
        }
        let prefix = ordinal / P7_PREFIX_BLOCK;
        let remaining = prefixes >> prefix;
        if remaining & 1 != 0 {
            ordinal
        } else if remaining != 0 {
            (prefix + remaining.trailing_zeros() as usize) * P7_PREFIX_BLOCK
        } else {
            P7_PATTERN_COUNT
        }
    }
}

fn root_piece_mask(
    batch: &ExactScoringExecutionBatch,
    graph: &ExactScoringExecutionGraph,
) -> Option<u8> {
    let root = graph.root();
    let node = graph.node(root)?;
    if node.accepting() || batch.initial_occupied() & !batch.layout().all_cells_mask() != 0 {
        return None;
    }
    let edges = graph.checked_edges(node)?;
    if edges.len() > MAX_P7_ROOT_EDGES {
        return None;
    }
    let mut pieces = 0;
    for &edge in edges {
        if edge.to() <= root || graph.node(edge.to()).is_none() {
            return None;
        }
        // Do not turn a malformed root frame or edge into a successful zero
        // count. Any unknown root keeps the original exact cursor path.
        SolutionTraceBuilder::project_scoring_step(batch.layout(), batch.initial_occupied(), edge)?;
        pieces |= 1 << piece_slot(edge.piece());
    }
    Some(pieces)
}

fn p7_prefix_mask(root_pieces: u8, hold_enabled: bool) -> u64 {
    let mut prefix = 0;
    let mut mask = 0;
    // StandardBagOrdinals orders each bag by STANDARD_TETROMINOES. Each
    // distinct two-piece prefix is followed by the same 5! suffixes.
    // With cursor 0 and an empty hold, the first lock is Current(q0)
    // or StoreCurrent(q1). Swap/release cannot place any other piece.
    for first in PieceKind::STANDARD_TETROMINOES {
        for second in PieceKind::STANDARD_TETROMINOES {
            if first == second {
                continue;
            }
            let first_mask = 1 << piece_slot(first);
            let second_mask = u8::from(hold_enabled) << piece_slot(second);
            if root_pieces & (first_mask | second_mask) != 0 {
                mask |= 1 << prefix;
            }
            prefix += 1;
        }
    }
    debug_assert_eq!(prefix, P7_PREFIX_COUNT);
    mask
}

pub(super) struct P7P4InventoryIndex {
    tail: [u8; TAIL_COUNT],
    tail_classes: [[u64; TAIL_WORDS]; TAIL_CLASS_COUNT],
    validated: usize,
    selection: Option<GeometrySelection>,
}

enum GeometrySelection {
    All,
    Restricted([u64; TAIL_WORDS]),
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct PhysicalPrefix {
    board: u64,
    counts_with_depth: u32,
}

impl PhysicalPrefix {
    const ROOT: Self = Self {
        board: 0,
        counts_with_depth: 0,
    };
    const UNKNOWN: Self = Self {
        board: 0,
        counts_with_depth: u32::MAX,
    };
}

impl P7P4InventoryIndex {
    pub(super) fn eligible(problem: &SearchProblem, batch: &ExactScoringExecutionBatch) -> bool {
        let supply = problem.supply();
        problem.initial_board().width() == 10
            && problem.initial_board().occupied_mask() == 0
            && problem.visible_height() == 4
            && problem.search_height() == 4
            && matches!(problem.search_goal(), SearchGoal::ClearToEmpty)
            && !problem
                .objective()
                .execution_constraints()
                .preserves_back_to_back()
            && !problem
                .queue_observation_policy()
                .requires_observation_policy()
            && problem.initial_hold().cursor() == 0
            && problem.initial_hold().hold_piece().is_none()
            && supply.source_sequence_length() == QUEUE
            && !supply.projects_unplaced_lookahead()
            && batch.complete()
            && batch.initial_occupied() == 0
            && batch.initial_cursor() == 0
            && batch.initial_hold().is_none()
            && batch.layout().width() == 10
            && batch.layout().height() == 4
            && batch.hold_enabled() == supply.hold_enabled()
            && !batch.projects_unplaced_lookahead()
            && batch.pattern_count() == PATTERN_COUNT
            && problem
                .piece_source()
                .materialized_universe()
                .is_some_and(|source| {
                    source.complete()
                        && source.pattern_count() == PATTERN_COUNT
                        && source.total_possible_pattern_count() == PATTERN_COUNT as u128
                        && matches!(
                            source.structure(),
                            MaterializedPatternUniverseStructure::Standard7BagLexicographic {
                                sequence_len: 11
                            }
                        )
                        && batch.standard_bag_ordinals().is_none_or(|ordinals| {
                            source.complete_standard_bag_ordinals() == Some(ordinals)
                        })
                })
    }

    pub(super) fn new() -> Self {
        Self {
            tail: [0; TAIL_COUNT],
            tail_classes: [[0; TAIL_WORDS]; TAIL_CLASS_COUNT],
            validated: 0,
            selection: None,
        }
    }

    pub(super) fn ready(&self) -> bool {
        self.validated == PATTERN_COUNT
    }

    /// Use the actual batch ordinals. Dense storage must validate every
    /// stored queue; an authenticated ordinal source only needs to decode
    /// one tail block. Both use bounded cooperative work.
    pub(super) fn advance_layout(&mut self, batch: &ExactScoringExecutionBatch) -> bool {
        // Eligibility already bound the opaque ordinal descriptor to the
        // authentic query. Its mixed-radix layout repeats the first tail block
        // for every first-bag permutation, unlike arbitrary dense sources.
        let validation_rows = if batch.standard_bag_ordinals().is_some() {
            TAIL_COUNT
        } else {
            PATTERN_COUNT
        };
        let end = (self.validated + INDEX_VALIDATION_CHUNK).min(validation_rows);
        for id in self.validated..end {
            let Some(queue) = batch.pattern_at(id) else {
                return false;
            };
            if !self.accept_layout_at(id, queue.as_ref()) {
                return false;
            }
            self.validated += 1;
        }
        if end == validation_rows {
            self.validated = PATTERN_COUNT;
        }
        true
    }

    fn accept_layout_at(&mut self, ordinal: usize, queue: &[PieceKind]) -> bool {
        if queue.len() != QUEUE || distinct_mask(&queue[..HEAD]) != Some(ALL_PIECES) {
            return false;
        }
        let Some(mask) = distinct_mask(&queue[HEAD..]) else {
            return false;
        };
        let rank = ordinal % TAIL_COUNT;
        let slot = &mut self.tail[rank];
        if ordinal < TAIL_COUNT {
            *slot = mask;
            let class = usize::from(TAIL_CLASS_BY_MASK[usize::from(mask)]);
            self.tail_classes[class][rank / 64] |= 1 << (rank % 64);
            true
        } else {
            *slot == mask
        }
    }

    pub(super) fn has_geometry(&self) -> bool {
        self.selection.is_some()
    }

    pub(super) fn clear_geometry(&mut self) {
        self.selection = None;
    }

    pub(super) fn use_all_patterns(&mut self) {
        self.selection = Some(GeometrySelection::All);
    }

    pub(super) fn scratch_projection(graph: &ExactScoringExecutionGraph) -> Option<u128> {
        if graph.node_count() > MAX_NODES || graph.all_edges().len() > MAX_EDGES {
            return None;
        }
        Some(core::mem::size_of::<[PhysicalPrefix; MAX_NODES]>() as u128)
    }

    pub(super) fn prepare_geometry(
        &mut self,
        batch: &ExactScoringExecutionBatch,
        graph: &ExactScoringExecutionGraph,
    ) {
        let Some(inventory) = graph_inventory(batch, graph) else {
            self.use_all_patterns();
            return;
        };
        self.set_inventory(inventory);
    }

    fn set_inventory(&mut self, inventory: u32) {
        let mut bits = [0; TAIL_WORDS];
        for_each_possible_tail(inventory, |mask| {
            let class = usize::from(TAIL_CLASS_BY_MASK[usize::from(mask)]);
            for (out, word) in bits.iter_mut().zip(self.tail_classes[class]) {
                *out |= word;
            }
        });
        self.selection = Some(GeometrySelection::Restricted(bits));
    }

    pub(super) fn next_pattern(&self, ordinal: usize) -> usize {
        match self.selection.as_ref() {
            Some(GeometrySelection::Restricted(bits)) => next_repeated(bits, ordinal),
            _ => ordinal,
        }
    }
}

fn piece_slot(piece: PieceKind) -> u32 {
    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

fn distinct_mask(queue: &[PieceKind]) -> Option<u8> {
    let mut mask = 0;
    for &piece in queue {
        let bit = 1 << piece_slot(piece);
        if mask & bit != 0 {
            return None;
        }
        mask |= bit;
    }
    Some(mask)
}

fn graph_inventory(
    batch: &ExactScoringExecutionBatch,
    graph: &ExactScoringExecutionGraph,
) -> Option<u32> {
    let root = graph.root() as usize;
    let limit = graph.node_count();
    if limit > MAX_NODES || graph.all_edges().len() > MAX_EDGES || root >= limit {
        return None;
    }
    // Transient and pre-admitted by the caller. Dropped before any language
    // cell allocation, so the optional proof cannot starve the real cursor.
    let mut scratch = [PhysicalPrefix::UNKNOWN; MAX_NODES];
    scratch[root] = PhysicalPrefix::ROOT;
    let mut stamped_inventory = None;
    let mut visited_edges = 0_usize;
    for node in root..limit {
        let prefix = scratch[node];
        if prefix == PhysicalPrefix::UNKNOWN {
            continue;
        }
        let graph_node = graph.node(node as u32)?;
        let edges = graph.checked_edges(graph_node)?;
        let depth = prefix.counts_with_depth >> DEPTH_SHIFT;
        if graph_node.accepting() {
            if depth != LOCKS || prefix.board != 0 {
                return None;
            }
            if stamped_inventory
                .replace(prefix.counts_with_depth)
                .is_some_and(|inventory| inventory != prefix.counts_with_depth)
            {
                return None;
            }
            continue;
        }
        // Range validity alone doesn't stop a malformed graph from reusing
        // the same large edge slice at every node.
        visited_edges = visited_edges.checked_add(edges.len())?;
        if visited_edges > MAX_EDGES {
            return None;
        }
        for &edge in edges {
            // The cursor ignores supply here; check every possible physical
            // prefix once. All actual witnesses are a subset of these paths.
            let target = edge.to() as usize;
            if target <= node || target >= limit || depth >= LOCKS {
                return None;
            }
            let (_, board) =
                SolutionTraceBuilder::project_scoring_step(batch.layout(), prefix.board, edge)?;
            let counts_with_depth = prefix.counts_with_depth
                + (1 << (piece_slot(edge.piece()) * 4))
                + (1 << DEPTH_SHIFT);
            let next = PhysicalPrefix {
                board,
                counts_with_depth,
            };
            if scratch[target] != PhysicalPrefix::UNKNOWN && scratch[target] != next {
                return None;
            }
            scratch[target] = next;
        }
    }
    stamped_inventory
}

const fn tail_classes() -> [u8; 1 << HEAD] {
    let mut classes = [u8::MAX; 1 << HEAD];
    let mut index = 0;
    let mut mask = 0_usize;
    while mask < classes.len() {
        if mask.count_ones() == TAIL as u32 {
            classes[mask] = index;
            index += 1;
        }
        mask += 1;
    }
    classes
}

fn for_each_possible_tail(inventory: u32, mut emit: impl FnMut(u8)) {
    // With fresh empty hold the only unplaced draw is either the 11th
    // unread piece (no store) or the final held piece (one store), by
    // the queue+hold+placed conservation law. Swaps preserve this equation.
    let mut missing = 0_u8;
    let mut required = 0_u8;
    for slot in 0..HEAD {
        match (inventory >> (slot * 4)) & 0xf {
            0 => missing |= 1 << slot,
            1 => (),
            2 => required |= 1 << slot,
            _ => return,
        }
    }
    // Every proven graph places 10 pieces. With cardinalities <= 2,
    // doubling d kinds and omitting z kinds gives 7 + d - z = 10.
    // Four second-bag slots therefore either exactly cover four doubled
    // kinds (z=1), or add one of the four singleton kinds (z=0).
    match (required.count_ones(), missing.count_ones()) {
        (4, 1) => emit(required),
        (3, 0) => {
            let mut extra = ALL_PIECES & !required;
            while extra != 0 {
                let bit = extra & extra.wrapping_neg();
                emit(required | bit);
                extra &= !bit;
            }
        }
        _ => (),
    }
}

fn next_repeated(bits: &[u64; TAIL_WORDS], ordinal: usize) -> usize {
    if ordinal >= PATTERN_COUNT {
        return PATTERN_COUNT;
    }
    let base = ordinal / TAIL_COUNT * TAIL_COUNT;
    let rank = ordinal % TAIL_COUNT;
    let start = rank / 64;
    let word = bits[start] & (u64::MAX << (rank % 64));
    if word != 0 {
        return base + start * 64 + word.trailing_zeros() as usize;
    }
    if let Some(tail) = first_set(&bits[start + 1..], start + 1) {
        return base + tail;
    }
    if let Some(tail) = first_set(bits, 0) {
        return (base + TAIL_COUNT + tail).min(PATTERN_COUNT);
    }
    PATTERN_COUNT
}

fn first_set(bits: &[u64], first_word: usize) -> Option<usize> {
    bits.iter()
        .enumerate()
        .find(|(_, &word)| word != 0)
        .map(|(index, word)| (first_word + index) * 64 + word.trailing_zeros() as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIECES: [PieceKind; HEAD] = [
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::J,
        PieceKind::L,
    ];

    #[test]
    fn p7_first_lock_prefixes_keep_exactly_the_supply_eligible_ordinals() {
        use clearra_core_domain::piece::standard_bag_ordinals::StandardBagOrdinals;
        let ordinals = StandardBagOrdinals::new(HEAD).unwrap();
        assert_eq!(ordinals.len(), P7_PATTERN_COUNT);
        for hold_enabled in [false, true] {
            let root_pieces = 1 << piece_slot(PieceKind::I);
            let mut index = P7FirstLockIndex {
                hold_enabled,
                prefixes: Some(p7_prefix_mask(root_pieces, hold_enabled)),
            };
            let expected = (0..ordinals.len()).filter(|&ordinal| {
                let (sequence, len) = ordinals.sequence_at(ordinal).unwrap();
                assert_eq!(len, HEAD);
                sequence[0] == PieceKind::I || (hold_enabled && sequence[1] == PieceKind::I)
            });
            let found: Vec<_> = std::iter::successors(Some(index.next_pattern(0)), |&ordinal| {
                (ordinal < P7_PATTERN_COUNT).then(|| index.next_pattern(ordinal + 1))
            })
            .take_while(|&ordinal| ordinal < P7_PATTERN_COUNT)
            .collect();
            assert!(found.iter().copied().eq(expected));
            assert_eq!(found.len(), if hold_enabled { 1440 } else { 720 });
            index.prepare_geometry(None);
            assert_eq!(index.next_pattern(321), 321, "unknown evidence never skips");
            index.clear_geometry();
            assert!(!index.has_geometry());
        }
    }

    fn visit_second_bag(
        queue: &mut Vec<PieceKind>,
        remaining: u8,
        emit: &mut impl FnMut(&[PieceKind]),
    ) {
        if queue.len() == QUEUE {
            emit(queue);
            return;
        }
        for (slot, &piece) in PIECES.iter().enumerate() {
            let bit = 1 << slot;
            if remaining & bit != 0 {
                queue.push(piece);
                visit_second_bag(queue, remaining & !bit, emit);
                queue.pop();
            }
        }
    }

    fn count_after_removing(queue: &[PieceKind], removed: usize) -> [u8; HEAD] {
        let mut counts = [0; HEAD];
        for (position, &piece) in queue.iter().enumerate() {
            if position != removed {
                counts[piece_slot(piece) as usize] += 1;
            }
        }
        counts
    }

    #[test]
    fn rejects_only_inventory_incompatible_ordinals_and_keeps_the_original_order() {
        let mut index = P7P4InventoryIndex::new();
        let mut queues = Vec::new();
        for head in [PIECES, {
            let mut reversed = PIECES;
            reversed.reverse();
            reversed
        }] {
            let mut queue = head.to_vec();
            visit_second_bag(&mut queue, ALL_PIECES, &mut |queue| {
                assert!(index.accept_layout_at(queues.len(), queue));
                queues.push(queue.to_vec());
            });
        }
        assert_eq!(queues.len(), 2 * TAIL_COUNT);
        for (counts, per_block) in [
            ([1, 1, 2, 2, 2, 1, 1], 96),
            ([0, 1, 2, 2, 2, 2, 1], 24),
            ([3, 0, 1, 2, 2, 1, 1], 0),
        ] {
            let inventory = counts
                .iter()
                .enumerate()
                .fold(LOCKS << DEPTH_SHIFT, |bits, (slot, &count)| {
                    bits | (u32::from(count) << (4 * slot))
                });
            index.set_inventory(inventory);
            let expected: Vec<_> = queues
                .iter()
                .enumerate()
                .filter_map(|(ordinal, queue)| {
                    (0..QUEUE)
                        .any(|removed| count_after_removing(queue, removed) == counts)
                        .then_some(ordinal)
                })
                .collect();
            let actual: Vec<_> = std::iter::successors(Some(index.next_pattern(0)), |&ordinal| {
                (ordinal + 1 < queues.len()).then(|| index.next_pattern(ordinal + 1))
            })
            .take_while(|&ordinal| ordinal < queues.len())
            .collect();
            assert_eq!(actual, expected);
            assert_eq!(actual.len(), 2 * per_block);
            assert_eq!(index.next_pattern(PATTERN_COUNT - 1), PATTERN_COUNT);
        }
    }

    #[test]
    fn inconsistent_queue_shape_cannot_certify_the_inventory_index() {
        let mut index = P7P4InventoryIndex::new();
        let mut queue = PIECES.to_vec();
        queue.extend_from_slice(&PIECES[..TAIL]);
        assert!(index.accept_layout_at(0, &queue));
        queue.swap(HEAD, HEAD + 1);
        assert!(index.accept_layout_at(TAIL_COUNT, &queue));
        queue[HEAD] = PieceKind::L;
        assert!(!index.accept_layout_at(TAIL_COUNT, &queue));
        queue[0] = queue[1];
        assert!(!index.accept_layout_at(0, &queue));
        assert!(!index.accept_layout_at(0, &queue[..HEAD]));
        assert!(!index.ready());
    }
}
