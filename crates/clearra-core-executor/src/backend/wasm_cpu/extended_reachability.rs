use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_replay::{RotationRequest, ScoringLockEvidence};
use clearra_rules::kicks::{KickTableProfile, KickTableProfileId, KickTransition};

use super::{extended_board::ExtendedBoard, kick_profiles::builtin_kick_profile, piece_index};

const INVALID_STATE: u16 = u16::MAX;

#[derive(Clone, Copy)]
struct State {
    rotation: RotationState,
    x: i8,
    y: i8,
}

pub(super) struct ExtendedReachableLocks {
    entries: Vec<((u8, i8, i8), ScoringLockEvidence)>,
}

impl ExtendedReachableLocks {
    pub fn contains(&self, rotation: RotationState, x: i8, y: i8) -> bool {
        self.entries
            .binary_search_by_key(&(rotation.quarter_turns(), x, y), |entry| entry.0)
            .is_ok()
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
            self.queue
                .reserve(state_count.saturating_sub(self.queue.capacity()));
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.visited.fill(0);
            self.evidence_generations.fill(0);
            self.generation = 1;
        }
        self.queue.clear();
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

        let mut kick_deltas: [Vec<(i8, i8)>; 12] = std::array::from_fn(|_| Vec::new());
        for from in RotationState::ALL {
            for (slot, to) in [
                from.clockwise(),
                from.counter_clockwise(),
                from.rotated_180(),
            ]
            .into_iter()
            .enumerate()
            {
                if slot == 2 && !allow_180 {
                    continue;
                }
                if let Some(sequence) = profile.sequence_for(KickTransition::new(piece, from, to)) {
                    kick_deltas[from.quarter_turns() as usize * 3 + slot].extend(
                        sequence.offsets().iter().map(|offset| {
                            normalized_kick_delta(piece, from, to, offset.dx(), offset.dy())
                        }),
                    );
                }
            }
        }
        let (translations, sky_seeds) = compile_translations(width, height, ceiling, &valid_states);
        let (rotation_offsets, rotation_targets, rotation_kick_indices) =
            compile_rotations(width, ceiling, &valid_states, &kick_deltas);
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
    let mut locks = Vec::new();
    let mut cursor = 0usize;
    while cursor < scratch.queue.len() {
        let source = scratch.queue[cursor];
        cursor += 1;
        let index = usize::from(source);
        let state = state_from_index(template.width, template.ceiling, index);
        if state.y < template.height as i8 && grounded(template, board, index) {
            locks.push(source);
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
    let mut entries = locks
        .into_iter()
        .map(|index| {
            let state = state_from_index(template.width, template.ceiling, usize::from(index));
            let state_index = usize::from(index);
            let evidence = if capture_scoring_evidence {
                let evidence = if scratch.evidence_generations[state_index] == generation {
                    scratch.evidence[state_index]
                } else {
                    ScoringLockEvidence::no_rotation(state.rotation)
                };
                evidence.with_immobile_before_clear(scoring_lock_is_immobile(
                    template,
                    board,
                    state_index,
                ))
            } else {
                ScoringLockEvidence::no_rotation(state.rotation)
            };
            ((state.rotation.quarter_turns(), state.x, state.y), evidence)
        })
        .collect::<Vec<_>>();
    entries.sort_unstable_by_key(|entry| entry.0);
    entries.dedup_by_key(|entry| entry.0);
    (ExtendedReachableLocks { entries }, cursor)
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
    state_index: usize,
) -> bool {
    template.translations[state_index].iter().all(|target| {
        *target == INVALID_STATE || board.intersects(template.state_masks[usize::from(*target)])
    })
}

fn compile_translations(
    width: u8,
    height: u8,
    ceiling: i8,
    valid: &[bool],
) -> (Vec<[u16; 3]>, Vec<u16>) {
    let mut translations = Vec::with_capacity(valid.len());
    let mut seeds = Vec::new();
    for source in 0..valid.len() {
        let state = state_from_index(width, ceiling, source);
        let candidates = [
            State {
                y: state.y - 1,
                ..state
            },
            State {
                x: state.x - 1,
                ..state
            },
            State {
                x: state.x + 1,
                ..state
            },
        ];
        let mut targets = [INVALID_STATE; 3];
        for (slot, candidate) in candidates.into_iter().enumerate() {
            if let Some(index) =
                state_index(width, ceiling, candidate).filter(|index| valid[*index])
            {
                targets[slot] = u16::try_from(index).unwrap_or(INVALID_STATE);
            }
        }
        translations.push(targets);
        if state.y >= height as i8 && valid[source] {
            if let Ok(source) = u16::try_from(source) {
                seeds.push(source);
            }
        }
    }
    (translations, seeds)
}

fn compile_rotations(
    width: u8,
    ceiling: i8,
    valid: &[bool],
    kick_deltas: &[Vec<(i8, i8)>; 12],
) -> (Vec<u32>, Vec<u16>, Vec<u8>) {
    let mut offsets = Vec::with_capacity(valid.len() * 3 + 1);
    let mut targets = Vec::new();
    let mut kick_indices = Vec::new();
    offsets.push(0);
    for source in 0..valid.len() {
        let state = state_from_index(width, ceiling, source);
        for (slot, to) in [
            state.rotation.clockwise(),
            state.rotation.counter_clockwise(),
            state.rotation.rotated_180(),
        ]
        .into_iter()
        .enumerate()
        {
            let transition = state.rotation.quarter_turns() as usize * 3 + slot;
            for (kick_index, &(dx, dy)) in kick_deltas[transition].iter().enumerate() {
                let candidate = State {
                    rotation: to,
                    x: state.x + dx,
                    y: state.y + dy,
                };
                let Some(index) =
                    state_index(width, ceiling, candidate).filter(|index| valid[*index])
                else {
                    continue;
                };
                if let Ok(index) = u16::try_from(index) {
                    targets.push(index);
                    kick_indices.push(kick_index as u8);
                }
            }
            offsets.push(targets.len() as u32);
        }
    }
    (offsets, targets, kick_indices)
}

fn state_index(width: u8, ceiling: i8, state: State) -> Option<usize> {
    if state.x < 0 || state.x >= width as i8 || state.y < 0 || state.y > ceiling {
        return None;
    }
    Some(
        (state.rotation.quarter_turns() as usize * (ceiling as usize + 1) + state.y as usize)
            * width as usize
            + state.x as usize,
    )
}

fn state_from_index(width: u8, ceiling: i8, index: usize) -> State {
    let rotation_stride = (ceiling as usize + 1) * width as usize;
    let rotation = RotationState::from_quarter_turns((index / rotation_stride) as u8)
        .expect("compiled rotation is valid");
    let remainder = index % rotation_stride;
    State {
        rotation,
        x: (remainder % width as usize) as i8,
        y: (remainder / width as usize) as i8,
    }
}

fn source_ceiling(height: u8, piece: PieceKind, allow_180: bool, profile: &KickTableProfile) -> i8 {
    let maximum_downward = profile
        .entries()
        .iter()
        .filter(|entry| entry.transition().piece() == piece)
        .filter(|entry| allow_180 || !entry.transition().is_180())
        .flat_map(|entry| {
            entry.sequence().offsets().iter().map(move |offset| {
                normalized_kick_delta(
                    piece,
                    entry.transition().from(),
                    entry.transition().to(),
                    offset.dx(),
                    offset.dy(),
                )
                .1
            })
        })
        .filter(|dy| *dy < 0)
        .map(|dy| -dy)
        .max()
        .unwrap_or(0);
    height.saturating_add(maximum_downward as u8) as i8
}

fn normalized_kick_delta(
    piece: PieceKind,
    from: RotationState,
    to: RotationState,
    kick_dx: i8,
    kick_dy: i8,
) -> (i8, i8) {
    let (from_x, from_y) = normalized_rotation_center(piece, from);
    let (to_x, to_y) = normalized_rotation_center(piece, to);
    (kick_dx + from_x - to_x, kick_dy + from_y - to_y)
}

fn normalized_rotation_center(piece: PieceKind, rotation: RotationState) -> (i8, i8) {
    const JLSTZ: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, 1)];
    const I: [(i8, i8); 4] = [(0, 0), (-2, 2), (0, 1), (-1, 2)];
    let index = rotation.quarter_turns() as usize;
    match piece_index(piece) {
        0 => I[index],
        1 => (0, 0),
        _ => JLSTZ[index],
    }
}
