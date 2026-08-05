use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_replay::{RotationRequest as ReplayRotationRequest, ScoringLockEvidence};
use clearra_rules::{
    kicks::{KickTableProfile, KickTransition, NoKick, SrsKicks},
    profile::rule_profile::RuleProfileId,
};

use crate::{board::StructureBoard, model::piece_index, support};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EntryLock {
    pub rotation: RotationState,
    pub x: i8,
    pub y: i8,
    pub mask: StructureBoard,
    pub evidence: ScoringLockEvidence,
    pub immobile: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct EntryResult {
    pub locks: Vec<EntryLock>,
    pub visited_states: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct State {
    rotation: RotationState,
    x: i8,
    y: i8,
}

#[derive(Clone, Copy, Debug)]
struct RotationTarget {
    state: usize,
    kick_index: u8,
    kick_dx: i8,
    kick_dy: i8,
}

#[derive(Clone, Copy, Debug)]
struct RotationEdge {
    source: usize,
    slot: u8,
    target: RotationTarget,
}

struct EntryTemplate {
    ceiling: i8,
    allow_180: bool,
    masks: Vec<StructureBoard>,
    valid: Vec<bool>,
    inside: Vec<bool>,
    /// down, left, right, up
    translations: Vec<[Option<usize>; 4]>,
    sky_seeds: Vec<usize>,
    rotations: Vec<[Vec<RotationTarget>; 3]>,
}

#[derive(Default)]
struct EntryScratch {
    generation: u32,
    visited: Vec<u32>,
    non_rotation_arrival: Vec<u32>,
    queue: Vec<usize>,
    rotation_edges: Vec<RotationEdge>,
}

impl EntryScratch {
    fn begin(&mut self, state_count: usize) -> u32 {
        if self.visited.len() < state_count {
            self.visited.resize(state_count, 0);
            self.non_rotation_arrival.resize(state_count, 0);
            self.queue
                .reserve(state_count.saturating_sub(self.queue.capacity()));
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.visited.fill(0);
            self.non_rotation_arrival.fill(0);
            self.generation = 1;
        }
        self.queue.clear();
        self.rotation_edges.clear();
        self.generation
    }
}

pub(crate) struct EntryCatalog {
    templates: [Option<EntryTemplate>; 7],
    scratch: EntryScratch,
    height: u8,
    rule_profile: RuleProfileId,
}

impl EntryCatalog {
    pub fn new(height: u8, rule_profile: RuleProfileId) -> Self {
        Self {
            templates: std::array::from_fn(|_| None),
            scratch: EntryScratch::default(),
            height,
            rule_profile,
        }
    }

    pub fn reachable_locks(
        &mut self,
        board: StructureBoard,
        piece: PieceKind,
        retain_scoring_evidence: bool,
        measure_immobility: bool,
    ) -> EntryResult {
        let template = self.templates[piece_index(piece)].get_or_insert_with(|| {
            EntryTemplate::compile(self.height, piece, self.rule_profile)
                .expect("query validation accepts only connected rule profiles")
        });
        search(
            template,
            board,
            &mut self.scratch,
            retain_scoring_evidence,
            measure_immobility,
        )
    }
}

impl EntryTemplate {
    fn compile(
        height: u8,
        piece: PieceKind,
        rule_profile: RuleProfileId,
    ) -> Result<Self, &'static str> {
        let profile = kick_profile(rule_profile)?;
        let allow_180 = profile.supports_180();
        let ceiling = source_ceiling(height, piece, allow_180, &profile);
        let state_count = 4 * (ceiling as usize + 1) * usize::from(StructureBoard::WIDTH);
        let definition = standard_tetromino_registry()
            .get(piece)
            .expect("standard tetromino exists");
        let mut masks = vec![StructureBoard::EMPTY; state_count];
        let mut valid = vec![false; state_count];
        let mut inside = vec![false; state_count];

        for rotation in RotationState::ALL {
            let shape = definition.shape(rotation);
            for y in 0..=ceiling {
                for x in 0..StructureBoard::WIDTH as i8 {
                    let state = State { rotation, x, y };
                    let index = state_index(ceiling, state).expect("compiled state in range");
                    let mut mask = StructureBoard::EMPTY;
                    let mut state_valid = true;
                    let mut state_inside = true;
                    for cell in shape.cells() {
                        let cell_x = i16::from(x) + i16::from(cell.x());
                        let cell_y = i16::from(y) + i16::from(cell.y());
                        if cell_x < 0 || cell_x >= i16::from(StructureBoard::WIDTH) || cell_y < 0 {
                            state_valid = false;
                            break;
                        }
                        if cell_y >= i16::from(height) {
                            state_inside = false;
                        } else {
                            mask.insert_index(
                                cell_y as u16 * u16::from(StructureBoard::WIDTH) + cell_x as u16,
                            );
                        }
                    }
                    if state_valid {
                        masks[index] = mask;
                        valid[index] = true;
                        inside[index] = state_inside;
                    }
                }
            }
        }

        let translations = compile_translations(ceiling, &valid);
        let sky_seeds = (0..state_count)
            .filter(|index| {
                let state = state_from_index(ceiling, *index);
                state.y >= height as i8 && valid[*index]
            })
            .collect();
        let rotations = compile_rotations(ceiling, piece, allow_180, &profile, &valid);
        Ok(Self {
            ceiling,
            allow_180,
            masks,
            valid,
            inside,
            translations,
            sky_seeds,
            rotations,
        })
    }
}

fn search(
    template: &EntryTemplate,
    board: StructureBoard,
    scratch: &mut EntryScratch,
    retain_scoring_evidence: bool,
    measure_immobility: bool,
) -> EntryResult {
    let generation = scratch.begin(template.masks.len());
    for seed in template.sky_seeds.iter().copied() {
        push_if_placeable(template, board, seed, scratch, generation, true);
    }
    let mut cursor = 0;
    while cursor < scratch.queue.len() {
        let source = scratch.queue[cursor];
        cursor += 1;
        for target in template.translations[source][..3].iter().flatten().copied() {
            push_if_placeable(template, board, target, scratch, generation, true);
        }
        let rotation_count = if template.allow_180 { 3 } else { 2 };
        for slot in 0..rotation_count {
            if let Some(target) = first_successful_kick(template, board, source, slot) {
                scratch.rotation_edges.push(RotationEdge {
                    source,
                    slot: slot as u8,
                    target,
                });
                push_if_placeable(template, board, target.state, scratch, generation, false);
            }
        }
    }

    let mut locks = Vec::new();
    for state_index in scratch.queue.iter().copied() {
        let state = state_from_index(template.ceiling, state_index);
        if !template.inside[state_index]
            || !support::grounded(
                state.y,
                template.translations[state_index][0],
                &template.masks,
                board,
            )
            || scratch.non_rotation_arrival[state_index] != generation
        {
            continue;
        }
        let immobile = measure_immobility
            && support::immobile(template.translations[state_index], &template.masks, board);
        locks.push(EntryLock {
            rotation: state.rotation,
            x: state.x,
            y: state.y,
            mask: template.masks[state_index],
            evidence: ScoringLockEvidence::no_rotation(state.rotation)
                .with_immobile_before_clear(immobile),
            immobile,
        });
    }

    for edge in scratch.rotation_edges.iter().copied() {
        let target_index = edge.target.state;
        let target = state_from_index(template.ceiling, target_index);
        if scratch.visited[target_index] != generation
            || !template.inside[target_index]
            || !support::grounded(
                target.y,
                template.translations[target_index][0],
                &template.masks,
                board,
            )
        {
            continue;
        }
        let source = state_from_index(template.ceiling, edge.source);
        let immobile = measure_immobility
            && support::immobile(template.translations[target_index], &template.masks, board);
        locks.push(EntryLock {
            rotation: target.rotation,
            x: target.x,
            y: target.y,
            mask: template.masks[target_index],
            evidence: ScoringLockEvidence::rotation(
                source.rotation,
                replay_request(edge.slot),
                edge.target.kick_index,
                edge.target.kick_dx,
                edge.target.kick_dy,
                source.x,
                source.y,
            )
            .with_immobile_before_clear(immobile),
            immobile,
        });
    }

    locks.sort_unstable_by_key(entry_lock_sort_key);
    locks.dedup_by(|left, right| entry_lock_equivalent(*left, *right, retain_scoring_evidence));
    EntryResult {
        locks,
        visited_states: scratch.queue.len() as u64,
    }
}

fn push_if_placeable(
    template: &EntryTemplate,
    board: StructureBoard,
    state: usize,
    scratch: &mut EntryScratch,
    generation: u32,
    non_rotation: bool,
) {
    if !template.valid[state] || board.intersects(template.masks[state]) {
        return;
    }
    if non_rotation {
        scratch.non_rotation_arrival[state] = generation;
    }
    if scratch.visited[state] != generation {
        scratch.visited[state] = generation;
        scratch.queue.push(state);
    }
}

fn first_successful_kick(
    template: &EntryTemplate,
    board: StructureBoard,
    source: usize,
    slot: usize,
) -> Option<RotationTarget> {
    template.rotations[source][slot]
        .iter()
        .copied()
        .find(|target| !board.intersects(template.masks[target.state]))
}

fn compile_translations(ceiling: i8, valid: &[bool]) -> Vec<[Option<usize>; 4]> {
    (0..valid.len())
        .map(|source| {
            let state = state_from_index(ceiling, source);
            [
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
                State {
                    y: state.y + 1,
                    ..state
                },
            ]
            .map(|target| state_index(ceiling, target).filter(|index| valid[*index]))
        })
        .collect()
}

fn compile_rotations(
    ceiling: i8,
    piece: PieceKind,
    allow_180: bool,
    profile: &KickTableProfile,
    valid: &[bool],
) -> Vec<[Vec<RotationTarget>; 3]> {
    (0..valid.len())
        .map(|source| {
            let state = state_from_index(ceiling, source);
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
                    if let Some(target) =
                        state_index(ceiling, candidate).filter(|index| valid[*index])
                    {
                        slots[slot].push(RotationTarget {
                            state: target,
                            kick_index: kick_index as u8,
                            kick_dx: offset.dx(),
                            kick_dy: offset.dy(),
                        });
                    }
                }
            }
            slots
        })
        .collect()
}

fn state_index(ceiling: i8, state: State) -> Option<usize> {
    if state.x < 0 || state.x >= StructureBoard::WIDTH as i8 || state.y < 0 || state.y > ceiling {
        return None;
    }
    Some(
        (state.rotation.quarter_turns() as usize * (ceiling as usize + 1) + state.y as usize)
            * usize::from(StructureBoard::WIDTH)
            + state.x as usize,
    )
}

fn state_from_index(ceiling: i8, index: usize) -> State {
    let stride = (ceiling as usize + 1) * usize::from(StructureBoard::WIDTH);
    let rotation =
        RotationState::from_quarter_turns((index / stride) as u8).expect("compiled rotation state");
    let remainder = index % stride;
    State {
        rotation,
        x: (remainder % usize::from(StructureBoard::WIDTH)) as i8,
        y: (remainder / usize::from(StructureBoard::WIDTH)) as i8,
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

fn kick_profile(rule: RuleProfileId) -> Result<KickTableProfile, &'static str> {
    match rule {
        RuleProfileId::SrsPlus => Ok(SrsKicks::srs_plus_profile()),
        RuleProfileId::Srs => Ok(SrsKicks::profile()),
        RuleProfileId::SrsX => Ok(SrsKicks::srs_x_profile()),
        RuleProfileId::Jstris180 => Ok(SrsKicks::jstris_180_profile()),
        RuleProfileId::NoKick => Ok(NoKick::profile()),
        RuleProfileId::Asc | RuleProfileId::Ars | RuleProfileId::Custom => {
            Err("spin_structure_rule_profile_not_connected")
        }
    }
}

const fn replay_request(slot: u8) -> ReplayRotationRequest {
    match slot {
        0 => ReplayRotationRequest::Clockwise,
        1 => ReplayRotationRequest::CounterClockwise,
        _ => ReplayRotationRequest::HalfTurn,
    }
}

fn evidence_class(evidence: ScoringLockEvidence) -> u8 {
    if !evidence.last_action_was_rotation() {
        0
    } else if evidence.kick_index() == 4
        && matches!(
            evidence.rotation_request(),
            ReplayRotationRequest::Clockwise | ReplayRotationRequest::CounterClockwise
        )
    {
        2
    } else {
        1
    }
}

fn entry_lock_sort_key(lock: &EntryLock) -> (u8, i8, i8, u8, u8, i8, i8) {
    let (predecessor_x, predecessor_y) = lock.evidence.predecessor();
    (
        lock.rotation.quarter_turns(),
        lock.x,
        lock.y,
        evidence_class(lock.evidence),
        lock.evidence.kick_index(),
        predecessor_x,
        predecessor_y,
    )
}

fn entry_lock_equivalent(left: EntryLock, right: EntryLock, retain_scoring_evidence: bool) -> bool {
    left.rotation == right.rotation
        && left.x == right.x
        && left.y == right.y
        && (!retain_scoring_evidence
            || evidence_class(left.evidence) == evidence_class(right.evidence))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_field_has_deterministic_grounded_locks() {
        let mut catalog = EntryCatalog::new(8, RuleProfileId::SrsPlus);
        let first = catalog.reachable_locks(StructureBoard::EMPTY, PieceKind::T, true, true);
        let second = catalog.reachable_locks(StructureBoard::EMPTY, PieceKind::T, true, true);
        assert!(!first.locks.is_empty());
        assert_eq!(first.locks, second.locks);
        assert!(first.locks.iter().all(|lock| lock.y == 0));
    }
}
