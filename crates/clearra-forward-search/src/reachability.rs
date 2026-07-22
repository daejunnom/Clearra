use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_replay::{RotationRequest as ReplayRotationRequest, ScoringLockEvidence};
use clearra_rules::{
    kicks::{KickTableProfile, KickTransition, NoKick, SrsKicks},
    profile::rule_profile::RuleProfileId,
};

use crate::board::ForwardBoard;

const INVALID_STATE: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct State {
    rotation: RotationState,
    x: i8,
    y: i8,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct RotationTarget {
    state: u16,
    kick_index: u8,
    kick_dx: i8,
    kick_dy: i8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReachableRotationEdge {
    source: u16,
    slot: u8,
    target: RotationTarget,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ReachableLock {
    pub rotation: RotationState,
    pub x: i8,
    pub y: i8,
    pub mask: ForwardBoard,
    pub evidence: LockEvidence,
    pub immobile: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum LockEvidence {
    NoRotation,
    Rotation {
        from: RotationState,
        request: RotationMove,
        kick_index: u8,
        kick_dx: i8,
        kick_dy: i8,
        predecessor_x: i8,
        predecessor_y: i8,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum RotationMove {
    Clockwise,
    CounterClockwise,
    HalfTurn,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ScoringEvidenceClass {
    NoRotation,
    Rotation,
    FinalKickOverride,
}

impl LockEvidence {
    pub fn scoring(self, rotation: RotationState, immobile: bool) -> ScoringLockEvidence {
        match self {
            Self::NoRotation => ScoringLockEvidence::no_rotation(rotation),
            Self::Rotation {
                from,
                request,
                kick_index,
                kick_dx,
                kick_dy,
                predecessor_x,
                predecessor_y,
            } => ScoringLockEvidence::rotation(
                from,
                request.replay(),
                kick_index,
                kick_dx,
                kick_dy,
                predecessor_x,
                predecessor_y,
            ),
        }
        .with_immobile_before_clear(immobile)
    }

    const fn scoring_class(self) -> ScoringEvidenceClass {
        match self {
            Self::NoRotation => ScoringEvidenceClass::NoRotation,
            Self::Rotation {
                request: RotationMove::Clockwise | RotationMove::CounterClockwise,
                kick_index: 4,
                ..
            } => ScoringEvidenceClass::FinalKickOverride,
            Self::Rotation { .. } => ScoringEvidenceClass::Rotation,
        }
    }

    pub(crate) const fn last_action_was_rotation(self) -> bool {
        matches!(self, Self::Rotation { .. })
    }
}

const LOCK_EVIDENCE_CLASS_COUNT: usize = 3;
const LOCK_PRESENCE_WORDS: usize = 4 * 10 * LOCK_EVIDENCE_CLASS_COUNT;

struct DenseLockTable {
    slots: Vec<Option<ReachableLock>>,
    presence_by_rotation_x_class: [u64; LOCK_PRESENCE_WORDS],
}

impl Default for DenseLockTable {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            presence_by_rotation_x_class: [0; LOCK_PRESENCE_WORDS],
        }
    }
}

impl DenseLockTable {
    fn begin(&mut self, state_count: usize) {
        let slot_count = state_count.saturating_mul(LOCK_EVIDENCE_CLASS_COUNT);
        if self.slots.len() < slot_count {
            self.slots.resize(slot_count, None);
        }
        self.presence_by_rotation_x_class.fill(0);
    }

    fn insert(&mut self, state_index: usize, state: State, lock: ReachableLock) {
        let class = scoring_evidence_class_index(lock.evidence.scoring_class());
        let y_bit = 1_u64 << state.y as u32;
        let presence = (state.rotation.quarter_turns() as usize * 10 + state.x as usize)
            * LOCK_EVIDENCE_CLASS_COUNT
            + class;
        if self.presence_by_rotation_x_class[presence] & y_bit != 0 {
            return;
        }
        self.presence_by_rotation_x_class[presence] |= y_bit;
        self.slots[state_index * LOCK_EVIDENCE_CLASS_COUNT + class] = Some(lock);
    }

    fn write_sorted(&self, template: &ReachabilityTemplate, output: &mut Vec<ReachableLock>) {
        output.clear();
        for rotation in RotationState::ALL {
            for x in 0..10_i8 {
                let presence_base = (rotation.quarter_turns() as usize * 10 + x as usize)
                    * LOCK_EVIDENCE_CLASS_COUNT;
                let mut ys = self.presence_by_rotation_x_class[presence_base]
                    | self.presence_by_rotation_x_class[presence_base + 1]
                    | self.presence_by_rotation_x_class[presence_base + 2];
                while ys != 0 {
                    let y = ys.trailing_zeros() as i8;
                    ys &= ys - 1;
                    let state = State { rotation, x, y };
                    let state_index = state_index(10, template.ceiling, state)
                        .expect("dense lock state belongs to its template");
                    let slot_base = state_index * LOCK_EVIDENCE_CLASS_COUNT;
                    let mut candidates = [None; LOCK_EVIDENCE_CLASS_COUNT];
                    let mut count = 0;
                    for class in 0..LOCK_EVIDENCE_CLASS_COUNT {
                        if self.presence_by_rotation_x_class[presence_base + class]
                            & (1_u64 << y as u32)
                            != 0
                        {
                            candidates[count] = self.slots[slot_base + class];
                            count += 1;
                        }
                    }
                    candidates[..count].sort_unstable_by_key(|lock| {
                        lock.map(lock_evidence_sort_key).unwrap_or((
                            u8::MAX,
                            u8::MAX,
                            i8::MAX,
                            i8::MAX,
                        ))
                    });
                    output.extend(candidates[..count].iter().flatten().copied());
                }
            }
        }
    }
}

const fn scoring_evidence_class_index(class: ScoringEvidenceClass) -> usize {
    match class {
        ScoringEvidenceClass::NoRotation => 0,
        ScoringEvidenceClass::Rotation => 1,
        ScoringEvidenceClass::FinalKickOverride => 2,
    }
}

const fn lock_evidence_sort_key(lock: ReachableLock) -> (u8, u8, i8, i8) {
    match lock.evidence {
        LockEvidence::NoRotation => (0, 0, 0, 0),
        LockEvidence::Rotation {
            kick_index,
            predecessor_x,
            predecessor_y,
            ..
        } => (1, kick_index, predecessor_x, predecessor_y),
    }
}

#[derive(Default)]
struct ReachabilityScratch {
    visited: Vec<u16>,
    non_rotation_arrival: Vec<u16>,
    first_rotation_arrival: Vec<u16>,
    first_rotation_edge: Vec<Option<ReachableRotationEdge>>,
    immobile: Vec<bool>,
    generation: u16,
    queue: Vec<u16>,
    rotation_edges: Vec<ReachableRotationEdge>,
}

impl ReachabilityScratch {
    fn begin(&mut self, state_count: usize) -> u16 {
        if self.visited.len() < state_count {
            self.visited.resize(state_count, 0);
            self.non_rotation_arrival.resize(state_count, 0);
            self.first_rotation_arrival.resize(state_count, 0);
            self.first_rotation_edge.resize(state_count, None);
            self.immobile.resize(state_count, false);
            self.queue
                .reserve(state_count.saturating_sub(self.queue.capacity()));
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.visited.fill(0);
            self.non_rotation_arrival.fill(0);
            self.first_rotation_arrival.fill(0);
            self.generation = 1;
        }
        self.queue.clear();
        self.rotation_edges.clear();
        self.generation
    }
}

pub(crate) struct ReachabilityWorkspace {
    templates: [Option<ReachabilityTemplate>; 7],
    scratch: ReachabilityScratch,
    locks: DenseLockTable,
    lock_output: Vec<ReachableLock>,
    rule_profile: RuleProfileId,
    height: u8,
}

impl ReachabilityWorkspace {
    pub fn new(height: u8, rule_profile: RuleProfileId) -> Result<Self, &'static str> {
        builtin_kick_profile(rule_profile)?;
        Ok(Self {
            templates: std::array::from_fn(|_| None),
            scratch: ReachabilityScratch::default(),
            locks: DenseLockTable::default(),
            lock_output: Vec::new(),
            rule_profile,
            height,
        })
    }

    pub fn reachable_locks(
        &mut self,
        board: ForwardBoard,
        piece: PieceKind,
        retain_rotation_evidence: bool,
        measure_immobility: bool,
    ) -> &[ReachableLock] {
        let index = piece_index(piece);
        let template = self.templates[index].get_or_insert_with(|| {
            ReachabilityTemplate::compile(self.height, piece, self.rule_profile)
                .expect("validated kick profile")
        });
        match self.height {
            0..=6 => search_reachable_locks::<1>(
                template,
                board,
                &mut self.scratch,
                &mut self.locks,
                &mut self.lock_output,
                retain_rotation_evidence,
                measure_immobility,
            ),
            7..=12 => search_reachable_locks::<2>(
                template,
                board,
                &mut self.scratch,
                &mut self.locks,
                &mut self.lock_output,
                retain_rotation_evidence,
                measure_immobility,
            ),
            _ => search_reachable_locks::<4>(
                template,
                board,
                &mut self.scratch,
                &mut self.locks,
                &mut self.lock_output,
                retain_rotation_evidence,
                measure_immobility,
            ),
        }
        &self.lock_output
    }
}

struct ReachabilityTemplate {
    ceiling: i8,
    allow_180: bool,
    state_masks: Vec<ForwardBoard>,
    valid_states: Vec<bool>,
    lock_inside_field: Vec<bool>,
    translations: Vec<[u16; 3]>,
    upward_translations: Vec<u16>,
    sky_seeds: Vec<u16>,
    rotations: Vec<[Vec<RotationTarget>; 3]>,
}

impl ReachabilityTemplate {
    fn compile(
        height: u8,
        piece: PieceKind,
        rule_profile: RuleProfileId,
    ) -> Result<Self, &'static str> {
        let width = 10_u8;
        let profile = builtin_kick_profile(rule_profile)?;
        let allow_180 = profile.supports_180();
        let ceiling = source_ceiling(height, piece, allow_180, &profile);
        let state_count = 4 * (ceiling as usize + 1) * width as usize;
        let definition = standard_tetromino_registry()
            .get(piece)
            .expect("standard tetromino exists");
        let mut state_masks = vec![ForwardBoard::EMPTY; state_count];
        let mut valid_states = vec![false; state_count];
        let mut lock_inside_field = vec![false; state_count];
        for rotation in RotationState::ALL {
            let shape = definition.shape(rotation);
            for y in 0..=ceiling {
                for x in 0..width as i8 {
                    let state = State { rotation, x, y };
                    let index = state_index(width, ceiling, state).expect("state in range");
                    let mut mask = ForwardBoard::EMPTY;
                    let mut valid = true;
                    let mut inside = true;
                    for cell in shape.cells() {
                        let cell_x = i16::from(x) + i16::from(cell.x());
                        let cell_y = i16::from(y) + i16::from(cell.y());
                        if cell_x < 0 || cell_x >= i16::from(width) || cell_y < 0 {
                            valid = false;
                            break;
                        }
                        if cell_y >= i16::from(height) {
                            inside = false;
                        } else {
                            mask.insert(cell_y as u16 * u16::from(width) + cell_x as u16);
                        }
                    }
                    if valid {
                        state_masks[index] = mask;
                        valid_states[index] = true;
                        lock_inside_field[index] = inside;
                    }
                }
            }
        }

        let (translations, upward_translations) =
            compile_translations(width, ceiling, &valid_states);
        let sky_seeds = (0..state_count)
            .filter(|index| {
                let state = state_from_index(width, ceiling, *index);
                state.y >= height as i8 && valid_states[*index]
            })
            .filter_map(|index| u16::try_from(index).ok())
            .collect();
        let rotations =
            compile_rotations(width, ceiling, piece, allow_180, &profile, &valid_states);
        Ok(Self {
            ceiling,
            allow_180,
            state_masks,
            valid_states,
            lock_inside_field,
            translations,
            upward_translations,
            sky_seeds,
            rotations,
        })
    }
}

fn search_reachable_locks<const ACTIVE_WORDS: usize>(
    template: &ReachabilityTemplate,
    board: ForwardBoard,
    scratch: &mut ReachabilityScratch,
    locks: &mut DenseLockTable,
    output: &mut Vec<ReachableLock>,
    retain_rotation_evidence: bool,
    measure_immobility: bool,
) {
    let generation = scratch.begin(template.state_masks.len());
    for &seed in &template.sky_seeds {
        push_if_placeable::<ACTIVE_WORDS>(template, board, seed, scratch, generation, true);
    }
    let mut cursor = 0_usize;
    while cursor < scratch.queue.len() {
        let source = scratch.queue[cursor];
        cursor += 1;
        let index = usize::from(source);
        for &target in &template.translations[index] {
            if target != INVALID_STATE {
                push_if_placeable::<ACTIVE_WORDS>(
                    template, board, target, scratch, generation, true,
                );
            }
        }
        let rotation_count = if template.allow_180 { 3 } else { 2 };
        for slot in 0..rotation_count {
            if let Some(target) =
                first_successful_kick::<ACTIVE_WORDS>(template, board, index, slot)
            {
                let edge = ReachableRotationEdge {
                    source,
                    slot: slot as u8,
                    target,
                };
                let target_index = usize::from(target.state);
                if scratch.first_rotation_arrival[target_index] != generation {
                    scratch.first_rotation_arrival[target_index] = generation;
                    scratch.first_rotation_edge[target_index] = Some(edge);
                }
                if retain_rotation_evidence {
                    scratch.rotation_edges.push(edge);
                }
                push_if_placeable::<ACTIVE_WORDS>(
                    template,
                    board,
                    target.state,
                    scratch,
                    generation,
                    false,
                );
            }
        }
    }

    locks.begin(template.state_masks.len());
    for &state_id in &scratch.queue {
        let index = usize::from(state_id);
        if !template.lock_inside_field[index] || !grounded::<ACTIVE_WORDS>(template, board, index) {
            continue;
        }
        let state = state_from_index(10, template.ceiling, index);
        let lock_immobile = measure_immobility && immobile::<ACTIVE_WORDS>(template, board, index);
        scratch.immobile[index] = lock_immobile;
        if scratch.non_rotation_arrival[index] == generation {
            let lock = ReachableLock {
                rotation: state.rotation,
                x: state.x,
                y: state.y,
                mask: template.state_masks[index],
                evidence: LockEvidence::NoRotation,
                immobile: lock_immobile,
            };
            locks.insert(index, state, lock);
        } else if !retain_rotation_evidence && scratch.first_rotation_arrival[index] == generation {
            let edge = scratch.first_rotation_edge[index]
                .expect("rotation arrival generation has representative edge");
            let source_state = state_from_index(10, template.ceiling, usize::from(edge.source));
            let lock = ReachableLock {
                rotation: state.rotation,
                x: state.x,
                y: state.y,
                mask: template.state_masks[index],
                evidence: LockEvidence::Rotation {
                    from: source_state.rotation,
                    request: replay_rotation_request(usize::from(edge.slot)),
                    kick_index: edge.target.kick_index,
                    kick_dx: edge.target.kick_dx,
                    kick_dy: edge.target.kick_dy,
                    predecessor_x: source_state.x,
                    predecessor_y: source_state.y,
                },
                immobile: lock_immobile,
            };
            locks.insert(index, state, lock);
        }
    }
    if retain_rotation_evidence {
        for &edge in &scratch.rotation_edges {
            let source = usize::from(edge.source);
            let source_state = state_from_index(10, template.ceiling, source);
            let target_index = usize::from(edge.target.state);
            if scratch.visited[target_index] != generation
                || !template.lock_inside_field[target_index]
                || !grounded::<ACTIVE_WORDS>(template, board, target_index)
            {
                continue;
            }
            let target_state = state_from_index(10, template.ceiling, target_index);
            let lock = ReachableLock {
                rotation: target_state.rotation,
                x: target_state.x,
                y: target_state.y,
                mask: template.state_masks[target_index],
                evidence: LockEvidence::Rotation {
                    from: source_state.rotation,
                    request: replay_rotation_request(usize::from(edge.slot)),
                    kick_index: edge.target.kick_index,
                    kick_dx: edge.target.kick_dx,
                    kick_dy: edge.target.kick_dy,
                    predecessor_x: source_state.x,
                    predecessor_y: source_state.y,
                },
                immobile: scratch.immobile[target_index],
            };
            locks.insert(target_index, target_state, lock);
        }
    }
    locks.write_sorted(template, output);
}

fn immobile<const ACTIVE_WORDS: usize>(
    template: &ReachabilityTemplate,
    board: ForwardBoard,
    source: usize,
) -> bool {
    let blocked = |target: u16| {
        target == INVALID_STATE
            || board.intersects_words::<ACTIVE_WORDS>(template.state_masks[usize::from(target)])
    };
    template.translations[source].iter().copied().all(blocked)
        && blocked(template.upward_translations[source])
}

fn push_if_placeable<const ACTIVE_WORDS: usize>(
    template: &ReachabilityTemplate,
    board: ForwardBoard,
    state: u16,
    scratch: &mut ReachabilityScratch,
    generation: u16,
    non_rotation_arrival: bool,
) {
    let index = usize::from(state);
    if !template.valid_states[index]
        || board.intersects_words::<ACTIVE_WORDS>(template.state_masks[index])
    {
        return;
    }
    if non_rotation_arrival {
        scratch.non_rotation_arrival[index] = generation;
    }
    if scratch.visited[index] != generation {
        scratch.visited[index] = generation;
        scratch.queue.push(state);
    }
}

fn grounded<const ACTIVE_WORDS: usize>(
    template: &ReachabilityTemplate,
    board: ForwardBoard,
    source: usize,
) -> bool {
    let state = state_from_index(10, template.ceiling, source);
    if state.y == 0 {
        return true;
    }
    let down = template.translations[source][0];
    down == INVALID_STATE
        || board.intersects_words::<ACTIVE_WORDS>(template.state_masks[usize::from(down)])
}

fn first_successful_kick<const ACTIVE_WORDS: usize>(
    template: &ReachabilityTemplate,
    board: ForwardBoard,
    source: usize,
    slot: usize,
) -> Option<RotationTarget> {
    template.rotations[source][slot]
        .iter()
        .copied()
        .find(|target| {
            !board.intersects_words::<ACTIVE_WORDS>(template.state_masks[usize::from(target.state)])
        })
}

fn compile_translations(width: u8, ceiling: i8, valid: &[bool]) -> (Vec<[u16; 3]>, Vec<u16>) {
    let mut translations = Vec::with_capacity(valid.len());
    let mut upward_translations = Vec::with_capacity(valid.len());
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
            if let Some(index) = state_index(width, ceiling, candidate).filter(|i| valid[*i]) {
                targets[slot] = u16::try_from(index).unwrap_or(INVALID_STATE);
            }
        }
        translations.push(targets);
        let upward = State {
            y: state.y + 1,
            ..state
        };
        upward_translations.push(
            state_index(width, ceiling, upward)
                .filter(|index| valid[*index])
                .and_then(|index| u16::try_from(index).ok())
                .unwrap_or(INVALID_STATE),
        );
    }
    (translations, upward_translations)
}

fn compile_rotations(
    width: u8,
    ceiling: i8,
    piece: PieceKind,
    allow_180: bool,
    profile: &KickTableProfile,
    valid: &[bool],
) -> Vec<[Vec<RotationTarget>; 3]> {
    let mut rotations = Vec::with_capacity(valid.len());
    for source in 0..valid.len() {
        let state = state_from_index(width, ceiling, source);
        let mut slots: [Vec<RotationTarget>; 3] = std::array::from_fn(|_| Vec::new());
        for (slot, to) in [
            state.rotation.clockwise(),
            state.rotation.counter_clockwise(),
            state.rotation.rotated_180(),
        ]
        .into_iter()
        .enumerate()
        {
            if slot == 2 && !allow_180 {
                continue;
            }
            let Some(sequence) =
                profile.sequence_for(KickTransition::new(piece, state.rotation, to))
            else {
                continue;
            };
            for (kick_index, offset) in sequence.offsets().iter().enumerate() {
                let (dx, dy) =
                    normalized_kick_delta(piece, state.rotation, to, offset.dx(), offset.dy());
                let candidate = State {
                    rotation: to,
                    x: state.x + dx,
                    y: state.y + dy,
                };
                let Some(index) = state_index(width, ceiling, candidate).filter(|i| valid[*i])
                else {
                    continue;
                };
                if let Ok(state_id) = u16::try_from(index) {
                    slots[slot].push(RotationTarget {
                        state: state_id,
                        kick_index: kick_index as u8,
                        kick_dx: offset.dx(),
                        kick_dy: offset.dy(),
                    });
                }
            }
        }
        rotations.push(slots);
    }
    rotations
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
        .expect("compiled rotation");
    let remainder = index % rotation_stride;
    State {
        rotation,
        x: (remainder % width as usize) as i8,
        y: (remainder / width as usize) as i8,
    }
}

fn source_ceiling(height: u8, piece: PieceKind, allow_180: bool, profile: &KickTableProfile) -> i8 {
    let downward = profile
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
    height.saturating_add(downward as u8) as i8
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
    match piece {
        PieceKind::I => I[index],
        PieceKind::O => (0, 0),
        PieceKind::T | PieceKind::S | PieceKind::Z | PieceKind::J | PieceKind::L => JLSTZ[index],
    }
}

fn replay_rotation_request(slot: usize) -> RotationMove {
    match slot {
        0 => RotationMove::Clockwise,
        1 => RotationMove::CounterClockwise,
        _ => RotationMove::HalfTurn,
    }
}

impl RotationMove {
    const fn replay(self) -> ReplayRotationRequest {
        match self {
            Self::Clockwise => ReplayRotationRequest::Clockwise,
            Self::CounterClockwise => ReplayRotationRequest::CounterClockwise,
            Self::HalfTurn => ReplayRotationRequest::HalfTurn,
        }
    }
}

fn builtin_kick_profile(rule: RuleProfileId) -> Result<KickTableProfile, &'static str> {
    match rule {
        RuleProfileId::SrsPlus => Ok(SrsKicks::srs_plus_profile()),
        RuleProfileId::Srs => Ok(SrsKicks::profile()),
        RuleProfileId::SrsX => Ok(SrsKicks::srs_x_profile()),
        RuleProfileId::NoKick => Ok(NoKick::profile()),
        RuleProfileId::Asc | RuleProfileId::Ars | RuleProfileId::Custom => {
            Err("forward_search_rule_profile_not_connected")
        }
    }
}

const fn piece_index(piece: PieceKind) -> usize {
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
