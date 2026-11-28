use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_replay::{RotationRequest, ScoringLockEvidence};
use clearra_rules::kicks::{KickTableProfileId, KickTransition};

use super::{
    extended_board::ExtendedBoard,
    extended_inverse_catalog::ExtendedInstantiation,
    kick_profiles::builtin_kick_profile,
    movement_topology::{
        compile_rotations, compile_translations, lock_anchor_contains, lock_anchor_insert,
        normalized_kick_deltas, source_ceiling, state_from_index, state_index, PoseValidity, State,
        INVALID_STATE_INDEX as INVALID_STATE,
    },
    piece_index,
};

/// A translation-only membership proof for catalog-instantiated locks.
///
/// The exact search seeds every valid pose at or above the field height. A
/// grounded lock whose upward sweep is empty is therefore reachable from the
/// same rotation/x sky seed using only downward translations. No kick ordering
/// is bypassed: this proves one path that contains no rotation at all. The
/// caller still owns shape validity and requests the full search for evidence
/// about rotations, finesse, or a pose that this sufficient condition rejects.
pub(super) struct ExtendedHarddropReachability {
    width: u8,
    board: ExtendedBoard,
    blocked_descent: ExtendedBoard,
}

impl ExtendedHarddropReachability {
    pub fn new(width: u8, board: ExtendedBoard) -> Self {
        debug_assert!((1..=10).contains(&width));
        // Reflect the upward sweep test through the intersection: every
        // occupied cell blocks all lower cells in its column. Reuse that
        // shadow for every candidate realization on this physical board.
        let mut shadow = board.words();
        let mut shift = usize::from(width);
        while shift < 256 {
            let below = shift_words_down(shadow, shift);
            for (word, below) in shadow.iter_mut().zip(below) {
                *word |= below;
            }
            shift *= 2;
        }
        Self {
            width,
            board,
            blocked_descent: ExtendedBoard::from_words(shadow),
        }
    }

    pub fn contains(&self, realization: ExtendedInstantiation) -> bool {
        if self.blocked_descent.intersects(realization.lock_mask) {
            return false;
        }
        realization.lock_y == 0
            || self
                .board
                .intersects(ExtendedBoard::from_words(shift_words_down(
                    realization.lock_mask.words(),
                    usize::from(self.width),
                )))
    }
}

#[inline]
fn shift_words_down(words: [u64; 4], shift: usize) -> [u64; 4] {
    debug_assert!(shift > 0 && shift < 256);
    let whole_words = shift / 64;
    let bits = shift % 64;
    std::array::from_fn(|index| {
        let source = index + whole_words;
        let Some(&word) = words.get(source) else {
            return 0;
        };
        let upper = if bits == 0 {
            0
        } else {
            words.get(source + 1).copied().unwrap_or(0) << (64 - bits)
        };
        (word >> bits) | upper
    })
}

pub(super) struct ExtendedReachableLocks {
    width: u8,
    anchors: [u64; 16],
    // Boolean membership never allocates this evidence-only payload.
    entries: Vec<((u8, i8, i8), ScoringLockEvidence)>,
}

impl ExtendedReachableLocks {
    pub fn contains(&self, rotation: RotationState, x: i8, y: i8) -> bool {
        lock_anchor_contains(&self.anchors, self.width, rotation, x, y)
    }

    pub fn scoring_evidence(&self, rotation: RotationState, x: i8, y: i8) -> ScoringLockEvidence {
        self.entries
            .binary_search_by_key(&(rotation.quarter_turns(), x, y), |entry| entry.0)
            .ok()
            .and_then(|index| self.entries.get(index))
            .map_or_else(
                || ScoringLockEvidence::no_rotation(rotation),
                |entry| entry.1,
            )
    }
}

#[derive(Default)]
struct ReachabilityScratch {
    visited: Vec<u16>,
    evidence_generations: Vec<u16>,
    evidence_ranks: Vec<(bool, bool, u8)>,
    evidence: Vec<ScoringLockEvidence>,
    generation: u16,
    queue: Vec<u16>,
}

impl ReachabilityScratch {
    fn begin(&mut self, state_count: usize) -> u16 {
        if self.visited.len() < state_count {
            self.visited.resize(state_count, 0);
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.visited.fill(0);
            self.evidence_generations.fill(0);
            self.generation = 1;
        }
        self.queue.clear();
        if self.queue.capacity() < state_count {
            // reserve is relative to len, not capacity. With an empty queue
            // this guarantees room for every state in one allocation.
            self.queue.reserve(state_count);
        }
        self.generation
    }

    fn retained_bytes(&self) -> usize {
        self.visited.capacity() * core::mem::size_of::<u16>()
            + self.evidence_generations.capacity() * core::mem::size_of::<u16>()
            + self.evidence_ranks.capacity() * core::mem::size_of::<(bool, bool, u8)>()
            + self.evidence.capacity() * core::mem::size_of::<ScoringLockEvidence>()
            + self.queue.capacity() * core::mem::size_of::<u16>()
    }

    fn prepare_scoring_evidence(&mut self, state_count: usize) {
        if self.evidence_generations.len() < state_count {
            self.evidence_generations.resize(state_count, 0);
            self.evidence_ranks.resize(state_count, (false, false, 0));
            self.evidence.resize(
                state_count,
                ScoringLockEvidence::no_rotation(RotationState::Zero),
            );
        }
    }

    fn record_scoring_evidence(
        &mut self,
        state: usize,
        generation: u16,
        rank: (bool, bool, u8),
        evidence: ScoringLockEvidence,
    ) {
        if self.evidence_generations[state] != generation || rank > self.evidence_ranks[state] {
            self.evidence_generations[state] = generation;
            self.evidence_ranks[state] = rank;
            self.evidence[state] = evidence;
        }
    }
}

pub(super) struct ExtendedReachabilityWorkspace {
    width: u8,
    height: u8,
    kick_profile_id: KickTableProfileId,
    templates: [Option<ExtendedReachabilityTemplate>; 7],
    scratch: ReachabilityScratch,
    visited_state_count: usize,
}

impl ExtendedReachabilityWorkspace {
    pub fn new(width: u8, height: u8, kick_profile_id: KickTableProfileId) -> Self {
        Self {
            width,
            height,
            kick_profile_id,
            templates: std::array::from_fn(|_| None),
            scratch: ReachabilityScratch::default(),
            visited_state_count: 0,
        }
    }

    pub fn reachable_locks_with_scoring(
        &mut self,
        board: ExtendedBoard,
        piece: PieceKind,
        capture_scoring_evidence: bool,
    ) -> ExtendedReachableLocks {
        let index = piece_index(piece);
        let template = self.templates[index].get_or_insert_with(|| {
            ExtendedReachabilityTemplate::compile(
                self.width,
                self.height,
                piece,
                self.kick_profile_id,
            )
        });
        let (locks, visited) =
            search_reachable_locks(template, board, &mut self.scratch, capture_scoring_evidence);
        self.visited_state_count = self.visited_state_count.saturating_add(visited);
        locks
    }

    pub const fn visited_state_count(&self) -> usize {
        self.visited_state_count
    }

    pub fn retained_bytes(&self) -> usize {
        self.scratch.retained_bytes()
            + self
                .templates
                .iter()
                .flatten()
                .map(ExtendedReachabilityTemplate::retained_bytes)
                .sum::<usize>()
    }

    /// Conservative creation/transient bound, charged before a cold template,
    /// scoring scratch, queue, or lock set allocates. Uses the exact profile's
    /// ordered kick lengths and sky ceiling rather than a generic 4L constant.
    pub(super) fn checked_query_future_bytes(
        &self,
        piece: PieceKind,
        scoring: bool,
    ) -> Option<u128> {
        let profile = builtin_kick_profile(self.kick_profile_id)?;
        let ceiling = source_ceiling(self.height, piece, profile.supports_180(), profile);
        let states = 4_u128
            .checked_mul(u128::try_from(ceiling).ok()?.checked_add(1)?)?
            .checked_mul(u128::from(self.width))?;
        let mut kicks = 0_u128;
        for from in RotationState::ALL {
            for to in [
                from.clockwise(),
                from.counter_clockwise(),
                from.rotated_180(),
            ] {
                kicks = kicks.checked_add(
                    profile
                        .sequence_for(KickTransition::new(piece, from, to))
                        .map_or(0, |sequence| sequence.offsets().len()) as u128,
                )?;
            }
        }
        let template = if self.templates[piece_index(piece)].is_none() {
            states
                .checked_mul(
                    (core::mem::size_of::<ExtendedBoard>()
                        + 1
                        + 3 * core::mem::size_of::<u16>()
                        + 3 * core::mem::size_of::<u32>()
                        + core::mem::size_of::<u16>()) as u128,
                )?
                .checked_add(states.checked_mul(kicks)?.checked_mul(3)?)?
                .checked_mul(2)?
                .checked_add(kicks.checked_mul(8)?)?
        } else {
            0
        };
        let mut scratch =
            checked_scratch_growth::<u16>(self.scratch.visited.capacity(), states)?.checked_add(
                checked_scratch_growth::<u16>(self.scratch.queue.capacity(), states)?,
            )?;
        if scoring {
            scratch = scratch
                .checked_add(checked_scratch_growth::<u16>(
                    self.scratch.evidence_generations.capacity(),
                    states,
                )?)?
                .checked_add(checked_scratch_growth::<(bool, bool, u8)>(
                    self.scratch.evidence_ranks.capacity(),
                    states,
                )?)?
                .checked_add(checked_scratch_growth::<ScoringLockEvidence>(
                    self.scratch.evidence.capacity(),
                    states,
                )?)?;
        }
        // Anchors stay inline. Scoring projects their final evidence into one
        // exactly-sized Vec after traversal; there is no intermediate lock
        // index Vec or evidence allocation for a Boolean query.
        let mut lock_set = core::mem::size_of::<ExtendedReachableLocks>() as u128;
        if scoring {
            lock_set = lock_set.checked_add(
                4_u128
                    .checked_mul(u128::from(self.width))?
                    .checked_mul(u128::from(self.height))?
                    .checked_mul(
                        core::mem::size_of::<((u8, i8, i8), ScoringLockEvidence)>() as u128
                    )?,
            )?;
        }
        template.checked_add(scratch)?.checked_add(lock_set)
    }
}

fn checked_scratch_growth<T>(capacity: usize, states: u128) -> Option<u128> {
    let capacity = capacity as u128;
    if states <= capacity {
        Some(0)
    } else {
        // Existing backing is already retained by the workspace. Charge the
        // entire replacement while old/new allocations may coexist.
        states
            .max(capacity.checked_mul(2)?)
            .max(4)
            .checked_mul(core::mem::size_of::<T>() as u128)
    }
}

pub(super) struct ExtendedReachabilityTemplate {
    width: u8,
    height: u8,
    ceiling: i8,
    allow_180: bool,
    state_masks: Vec<ExtendedBoard>,
    valid_states: Vec<bool>,
    translations: Vec<[u16; 3]>,
    sky_seeds: Vec<u16>,
    rotation_offsets: Vec<u32>,
    rotation_targets: Vec<u16>,
    rotation_kick_indices: Vec<u8>,
}

impl ExtendedReachabilityTemplate {
    fn compile(width: u8, height: u8, piece: PieceKind, profile_id: KickTableProfileId) -> Self {
        let profile = builtin_kick_profile(profile_id)
            .expect("WASM reachability only compiles connected exact kick profiles");
        let allow_180 = profile.supports_180();
        let ceiling = source_ceiling(height, piece, allow_180, profile);
        let state_count = 4 * (ceiling as usize + 1) * width as usize;
        let definition = standard_tetromino_registry()
            .get(piece)
            .expect("standard tetromino exists");
        let mut state_masks = vec![ExtendedBoard::EMPTY; state_count];
        let mut valid_states = vec![false; state_count];
        for rotation in RotationState::ALL {
            let shape = definition.shape(rotation);
            for y in 0..=ceiling {
                for x in 0..width as i8 {
                    let state = State { rotation, x, y };
                    let index = state_index(width, ceiling, state).expect("state is in range");
                    let mut mask = ExtendedBoard::EMPTY;
                    let mut valid = true;
                    for cell in shape.cells() {
                        let cell_x = i16::from(x) + i16::from(cell.x());
                        let cell_y = i16::from(y) + i16::from(cell.y());
                        if cell_x < 0 || cell_x >= i16::from(width) || cell_y < 0 {
                            valid = false;
                            break;
                        }
                        if cell_y < i16::from(height) {
                            mask.insert(cell_y as u16 * u16::from(width) + cell_x as u16);
                        }
                    }
                    if valid {
                        state_masks[index] = mask;
                        valid_states[index] = true;
                    }
                }
            }
        }

        let kick_deltas = normalized_kick_deltas(piece, allow_180, profile);
        let valid = PoseValidity::FullHeightFlags(&valid_states);
        let (translations, sky_seeds) = compile_translations(width, height, ceiling, valid);
        let (rotation_offsets, rotation_targets, rotation_kick_indices) = compile_rotations(
            width,
            ceiling,
            valid,
            &kick_deltas.each_ref().map(Vec::as_slice),
        );
        Self {
            width,
            height,
            ceiling,
            allow_180,
            state_masks,
            valid_states,
            translations,
            sky_seeds,
            rotation_offsets,
            rotation_targets,
            rotation_kick_indices,
        }
    }

    fn retained_bytes(&self) -> usize {
        self.state_masks.capacity() * core::mem::size_of::<ExtendedBoard>()
            + self.valid_states.capacity() * core::mem::size_of::<bool>()
            + self.translations.capacity() * core::mem::size_of::<[u16; 3]>()
            + self.sky_seeds.capacity() * core::mem::size_of::<u16>()
            + self.rotation_offsets.capacity() * core::mem::size_of::<u32>()
            + self.rotation_targets.capacity() * core::mem::size_of::<u16>()
            + self.rotation_kick_indices.capacity() * core::mem::size_of::<u8>()
    }
}

fn search_reachable_locks(
    template: &ExtendedReachabilityTemplate,
    board: ExtendedBoard,
    scratch: &mut ReachabilityScratch,
    capture_scoring_evidence: bool,
) -> (ExtendedReachableLocks, usize) {
    let generation = scratch.begin(template.state_masks.len());
    if capture_scoring_evidence {
        scratch.prepare_scoring_evidence(template.state_masks.len());
    }
    for &seed in &template.sky_seeds {
        push_if_placeable(template, board, seed, scratch, generation);
    }
    let mut locks = ExtendedReachableLocks {
        width: template.width,
        anchors: [0; 16],
        entries: Vec::new(),
    };
    let mut cursor = 0usize;
    while cursor < scratch.queue.len() {
        let source = scratch.queue[cursor];
        cursor += 1;
        let index = usize::from(source);
        let state = state_from_index(template.width, template.ceiling, index);
        if state.y < template.height as i8 && grounded(template, board, index) {
            lock_anchor_insert(
                &mut locks.anchors,
                template.width,
                state.rotation,
                state.x,
                state.y,
            );
        }
        for &target in &template.translations[index] {
            if target != INVALID_STATE {
                push_if_placeable(template, board, target, scratch, generation);
            }
        }
        let rotation_count = if template.allow_180 { 3 } else { 2 };
        for slot in 0..rotation_count {
            if let Some((target, kick_index)) = first_successful_kick(template, board, index, slot)
            {
                if capture_scoring_evidence {
                    let target_state =
                        state_from_index(template.width, template.ceiling, usize::from(target));
                    let request = match slot {
                        0 => RotationRequest::Clockwise,
                        1 => RotationRequest::CounterClockwise,
                        _ => RotationRequest::HalfTurn,
                    };
                    let evidence = ScoringLockEvidence::rotation(
                        state.rotation,
                        request,
                        kick_index,
                        target_state.x - state.x,
                        target_state.y - state.y,
                        state.x,
                        state.y,
                    );
                    let is_quarter_turn = slot < 2;
                    scratch.record_scoring_evidence(
                        usize::from(target),
                        generation,
                        (
                            is_quarter_turn && kick_index == 4,
                            is_quarter_turn,
                            kick_index,
                        ),
                        evidence,
                    );
                }
                push_if_placeable(template, board, target, scratch, generation);
            }
        }
    }
    if capture_scoring_evidence {
        let lock_count = locks
            .anchors
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum();
        let mut entries = Vec::with_capacity(lock_count);
        for (word_index, mut word) in locks.anchors.iter().copied().enumerate() {
            let rotation = RotationState::from_quarter_turns((word_index / 4) as u8)
                .expect("four lock rotation planes");
            while word != 0 {
                let anchor = (word_index % 4) * 64 + word.trailing_zeros() as usize;
                word &= word - 1;
                let state = State {
                    rotation,
                    x: (anchor % usize::from(template.width)) as i8,
                    y: (anchor / usize::from(template.width)) as i8,
                };
                let state_index = state_index(template.width, template.ceiling, state)
                    .expect("reachable lock is in the compiled pose domain");
                let evidence = if scratch.evidence_generations[state_index] == generation {
                    scratch.evidence[state_index]
                } else {
                    ScoringLockEvidence::no_rotation(state.rotation)
                };
                let evidence = evidence.with_immobile_before_clear(scoring_lock_is_immobile(
                    template,
                    board,
                    state_index,
                ));
                entries.push(((state.rotation.quarter_turns(), state.x, state.y), evidence));
            }
        }
        entries.sort_unstable_by_key(|entry| entry.0);
        locks.entries = entries;
    }
    (locks, cursor)
}

fn push_if_placeable(
    template: &ExtendedReachabilityTemplate,
    board: ExtendedBoard,
    state: u16,
    scratch: &mut ReachabilityScratch,
    generation: u16,
) {
    let index = usize::from(state);
    if !template.valid_states[index] || board.intersects(template.state_masks[index]) {
        return;
    }
    if scratch.visited[index] != generation {
        scratch.visited[index] = generation;
        scratch.queue.push(state);
    }
}

fn grounded(
    template: &ExtendedReachabilityTemplate,
    board: ExtendedBoard,
    state_index: usize,
) -> bool {
    let state = state_from_index(template.width, template.ceiling, state_index);
    if state.y == 0 {
        return true;
    }
    let down = template.translations[state_index][0];
    down == INVALID_STATE || board.intersects(template.state_masks[usize::from(down)])
}

fn first_successful_kick(
    template: &ExtendedReachabilityTemplate,
    board: ExtendedBoard,
    source: usize,
    slot: usize,
) -> Option<(u16, u8)> {
    let transition = source * 3 + slot;
    let start = template.rotation_offsets[transition] as usize;
    let end = template.rotation_offsets[transition + 1] as usize;
    template.rotation_targets[start..end]
        .iter()
        .copied()
        .zip(template.rotation_kick_indices[start..end].iter().copied())
        .find(|(target, _)| !board.intersects(template.state_masks[usize::from(*target)]))
}

fn scoring_lock_is_immobile(
    template: &ExtendedReachabilityTemplate,
    board: ExtendedBoard,
    lock_state_index: usize,
) -> bool {
    let translations_blocked = template.translations[lock_state_index]
        .iter()
        .all(|target| {
            *target == INVALID_STATE || board.intersects(template.state_masks[usize::from(*target)])
        });
    let state = state_from_index(template.width, template.ceiling, lock_state_index);
    let upward_blocked = state_index(
        template.width,
        template.ceiling,
        State {
            y: state.y.saturating_add(1),
            ..state
        },
    )
    .is_none_or(|target| {
        !template.valid_states[target] || board.intersects(template.state_masks[target])
    });
    translations_blocked && upward_blocked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_topology_preserves_locks_across_field_heights_and_words() {
        const WIDTH: u8 = 10;
        const BASE_HEIGHT: u8 = 6;
        let mut boards = [0_u64; 3];
        for x in 0..WIDTH {
            for y in 0..x % 4 {
                boards[1] |= 1_u64 << (y * WIDTH + x);
            }
            if (2..8).contains(&x) {
                boards[2] |= 1_u64 << (3 * WIDTH + x);
            }
        }
        for y in 0..3 {
            boards[2] |= 1_u64 << (y * WIDTH + 2);
        }

        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            let expected = PieceKind::STANDARD_TETROMINOES.map(|piece| {
                boards.map(|board| {
                    super::super::reachability::exact_spawn_lock_anchors(
                        WIDTH,
                        BASE_HEIGHT,
                        board,
                        piece,
                        profile,
                    )
                    .unwrap()
                })
            });
            for height in [6, 7, 13, 20, 24] {
                let offset = height - BASE_HEIGHT;
                let mut workspace = ExtendedReachabilityWorkspace::new(WIDTH, height, profile);
                for (board_index, board) in boards.into_iter().enumerate() {
                    // A solid floor translates the same playable six rows
                    // across the compact boundary and every board word.
                    let mut shifted = ExtendedBoard::EMPTY;
                    for cell in 0..u16::from(offset) * u16::from(WIDTH) {
                        shifted.insert(cell);
                    }
                    for cell in 0..BASE_HEIGHT * WIDTH {
                        if board & (1_u64 << cell) != 0 {
                            shifted.insert(u16::from(cell + offset * WIDTH));
                        }
                    }
                    for (piece_index, piece) in
                        PieceKind::STANDARD_TETROMINOES.into_iter().enumerate()
                    {
                        let locks = workspace.reachable_locks_with_scoring(shifted, piece, false);
                        assert_eq!(
                            locks.entries.capacity(),
                            0,
                            "Boolean locks allocate no evidence"
                        );
                        let scored = workspace.reachable_locks_with_scoring(shifted, piece, true);
                        assert_eq!(locks.anchors, scored.anchors);
                        assert_eq!(
                            scored.entries.len(),
                            locks
                                .anchors
                                .iter()
                                .map(|word| word.count_ones() as usize)
                                .sum::<usize>()
                        );
                        for rotation in RotationState::ALL {
                            let anchors = expected[piece_index][board_index]
                                [rotation.quarter_turns() as usize];
                            for y in 0..height {
                                for x in 0..WIDTH {
                                    let expected_lock = y >= offset
                                        && anchors & (1_u64 << ((y - offset) * WIDTH + x)) != 0;
                                    assert_eq!(
                                        locks.contains(rotation, x as i8, y as i8),
                                        expected_lock,
                                        "{profile:?} {piece:?} {rotation:?}, height={height}, board={board_index}, {x},{y}"
                                    );
                                    assert_eq!(
                                        scored
                                            .entries
                                            .binary_search_by_key(
                                                &(rotation.quarter_turns(), x as i8, y as i8),
                                                |entry| entry.0,
                                            )
                                            .is_ok(),
                                        expected_lock,
                                        "scoring evidence has exactly the Boolean lock domain"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn full_height_lock_bitsets_preserve_triple_kick_evidence_and_warm_query_bounds() {
        const WIDTH: u8 = 10;
        let cavity: [(u8, u8); 14] = [
            (0, 0),
            (1, 0),
            (0, 1),
            (0, 2),
            (3, 0),
            (4, 0),
            (2, 1),
            (3, 1),
            (3, 2),
            (4, 2),
            (2, 3),
            (3, 3),
            (4, 1),
            (4, 3),
        ];
        let mut reference = None;
        for offset in [0_u8, 18] {
            let height = offset + 6;
            let mut without_j = ExtendedBoard::EMPTY;
            for y in 0..height {
                for x in 0..WIDTH {
                    if y < offset || x >= 5 {
                        without_j.insert(u16::from(y) * u16::from(WIDTH) + u16::from(x));
                    }
                }
            }
            for (x, y) in cavity {
                without_j.insert(u16::from(y + offset) * u16::from(WIDTH) + u16::from(x));
            }
            let mut with_j = without_j;
            for (x, y) in [(0_u8, 3_u8), (0, 4), (0, 5), (1, 5)] {
                with_j.insert(u16::from(y + offset) * u16::from(WIDTH) + u16::from(x));
            }
            let mut workspace =
                ExtendedReachabilityWorkspace::new(WIDTH, height, KickTableProfileId::SrsPlus);
            let target = (RotationState::Right, 1, offset as i8 + 1);
            let without = workspace.reachable_locks_with_scoring(without_j, PieceKind::T, false);
            assert!(!without.contains(target.0, target.1, target.2));
            assert_eq!(without.entries.capacity(), 0);
            assert_eq!(
                workspace.checked_query_future_bytes(PieceKind::T, false),
                Some(core::mem::size_of::<ExtendedReachableLocks>() as u128),
                "warm Boolean queries have no future heap allocation"
            );

            let before = workspace.retained_bytes();
            let future = workspace
                .checked_query_future_bytes(PieceKind::T, true)
                .unwrap();
            let locks = workspace.reachable_locks_with_scoring(with_j, PieceKind::T, true);
            let retained_growth = workspace.retained_bytes() - before
                + core::mem::size_of::<ExtendedReachableLocks>()
                + locks.entries.capacity()
                    * core::mem::size_of::<((u8, i8, i8), ScoringLockEvidence)>();
            assert!(retained_growth as u128 <= future);
            assert!(locks.contains(target.0, target.1, target.2));
            let evidence = locks.scoring_evidence(target.0, target.1, target.2);
            assert!(evidence.last_action_was_rotation());
            assert!(evidence.used_kick());
            assert!(evidence.first_success_confirmed());

            let shape = standard_tetromino_registry()
                .get(PieceKind::T)
                .unwrap()
                .shape(target.0);
            let can_translate = |dx, dy| {
                shape.cells().iter().all(|cell| {
                    let x = target.1 + dx + cell.x();
                    let y = target.2 + dy + cell.y();
                    x >= 0
                        && x < WIDTH as i8
                        && y >= 0
                        && (y >= height as i8
                            || !with_j.contains(y as u16 * u16::from(WIDTH) + x as u16))
                })
            };
            assert_eq!(
                evidence.immobile_before_clear(),
                [(0, -1), (-1, 0), (1, 0), (0, 1)]
                    .into_iter()
                    .all(|(dx, dy)| !can_translate(dx, dy))
            );
            let (x, y) = evidence.predecessor();
            let normalized = ScoringLockEvidence::rotation(
                evidence.from_rotation(),
                evidence.rotation_request(),
                evidence.kick_index(),
                evidence.kick_dx(),
                evidence.kick_dy(),
                x,
                y - offset as i8,
            )
            .with_immobile_before_clear(evidence.immobile_before_clear());
            if let Some(reference) = reference {
                assert_eq!(
                    normalized, reference,
                    "high-word storage preserves evidence ranking and ties"
                );
            } else {
                reference = Some(normalized);
            }
            let boolean = workspace.reachable_locks_with_scoring(with_j, PieceKind::T, false);
            assert_eq!(boolean.anchors, locks.anchors);
            assert_eq!(boolean.entries.capacity(), 0);
        }
    }

    #[test]
    fn extended_harddrop_proof_matches_swept_cells_and_full_search_across_words() {
        let mut positive_words = 0_u8;
        let mut proved = 0_usize;
        let mut fallback_reachable = 0_usize;
        for (width, height) in [(4, 8), (10, 7), (10, 10), (10, 14), (10, 20), (10, 24)] {
            let mut stairs = ExtendedBoard::EMPTY;
            let mut shelf = ExtendedBoard::EMPTY;
            for x in 0..width {
                for y in 0..(x * 3 + 1) % height {
                    stairs.insert(u16::from(y) * u16::from(width) + u16::from(x));
                }
                if x > 0 && x + 1 < width {
                    shelf.insert(u16::from(height - 2) * u16::from(width) + u16::from(x));
                }
            }
            for profile in [KickTableProfileId::Srs90, KickTableProfileId::SrsPlus] {
                let mut workspace = ExtendedReachabilityWorkspace::new(width, height, profile);
                for board in [ExtendedBoard::EMPTY, stairs, shelf] {
                    let proof = ExtendedHarddropReachability::new(width, board);
                    for piece in PieceKind::STANDARD_TETROMINOES {
                        let locks = workspace.reachable_locks_with_scoring(board, piece, false);
                        let definition = standard_tetromino_registry().get(piece).unwrap();
                        for rotation in RotationState::ALL {
                            let shape = definition.shape(rotation);
                            for y in 0..height as i8 {
                                for x in 0..width as i8 {
                                    let mut mask = ExtendedBoard::EMPTY;
                                    let valid = shape.cells().iter().all(|cell| {
                                        let cell_x = x + cell.x();
                                        let cell_y = y + cell.y();
                                        if cell_x < 0
                                            || cell_x >= width as i8
                                            || cell_y < 0
                                            || cell_y >= height as i8
                                        {
                                            return false;
                                        }
                                        mask.insert(
                                            cell_y as u16 * u16::from(width) + cell_x as u16,
                                        )
                                    });
                                    if !valid {
                                        continue;
                                    }
                                    let realization = ExtendedInstantiation {
                                        lock_mask: mask,
                                        rotation,
                                        x,
                                        lock_y: y,
                                    };
                                    // An independent cell-wise upward sweep checks both
                                    // word carries and the floor/support boundary.
                                    let grounded = y == 0
                                        || mask.cells().any(|cell| {
                                            cell >= u16::from(width)
                                                && board.contains(cell - u16::from(width))
                                        });
                                    let blocked = mask.cells().any(|cell| {
                                        let column = cell % u16::from(width);
                                        (cell / u16::from(width)..u16::from(height)).any(|row| {
                                            board.contains(row * u16::from(width) + column)
                                        })
                                    });
                                    let actual = proof.contains(realization);
                                    assert_eq!(actual, grounded && !blocked);
                                    let full = locks.contains(rotation, x, y);
                                    if actual {
                                        assert!(
                                            full,
                                            "proof must be a real BFS lock: {profile:?} {piece:?} {rotation:?} {x},{y}"
                                        );
                                        proved += 1;
                                        for (word, bits) in mask.words().into_iter().enumerate() {
                                            if bits != 0 {
                                                positive_words |= 1 << word;
                                            }
                                        }
                                    } else if full {
                                        fallback_reachable += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(proved > 0);
        assert_eq!(positive_words, 0b1111);
        assert!(
            fallback_reachable > 0,
            "overhang locks still need the full search"
        );
    }
}
