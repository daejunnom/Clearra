//! Two independently compiled Build domains, with an exact combined-board
//! verifier only at the moving boundary. Geometry cache keys never contain a
//! queue. Conversely, no isolated reachability or spin failure removes repair.
use super::super::{
    field::PreparedFields, RecoveryBuildError as Error, RecoveryBuildFields, RecoveryBuildQuery,
    RecoveryBuildStep,
};
use super::source::{cancelled, PIECES};
use crate::{
    board::{place_and_clear, ForwardBoard},
    reachability::{ReachabilityWorkspace, ReachableLock},
    search::t_corner_counts,
};
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask as Mask, execution_cancellation::ExecutionControl,
};
use clearra_core_executor::backend::{BuildStageDomain, BuildStageDomainError};
use clearra_problem::BuildProbabilityField;
use clearra_replay::ScoringExecutionEdge;
use clearra_scoring::{
    b2b_preservation::BackToBackPreservationPolicy, event::SpinDetector, profile::SpinProfile,
};
use std::{collections::HashMap, sync::Arc};

pub(super) struct Stage {
    pub fields: RecoveryBuildFields,
    pub prepared: PreparedFields,
    pub middle_domain: BuildStageDomain,
    pub result_domain: BuildStageDomain,
    pub result_to_logical: Vec<u8>,
    logical_to_result: Vec<Option<u8>>,
}
fn as_mask(board: ForwardBoard) -> Mask {
    Mask::from_words(board.words())
}
fn field(height: u8, base: Mask, target: Mask) -> Result<BuildProbabilityField, Error> {
    BuildProbabilityField::from_words_preserving_height(height, base.words(), target.words())
        .map_err(|_| Error::BoardOutsideField)
}
fn domain_error(error: BuildStageDomainError) -> Error {
    match error {
        BuildStageDomainError::Cancelled => Error::Cancelled,
        BuildStageDomainError::Allocation => Error::MemoryUnavailable,
        BuildStageDomainError::InvalidField => Error::PatternDomainUnavailable,
    }
}
fn occupied_height(mask: Mask) -> u8 {
    (0..24)
        .rev()
        .find(|&y| (0..10).any(|x| mask.contains_index(u16::from(y) * 10 + x)))
        .map_or(1, |y| y + 1)
}
impl Stage {
    fn compile(fields: RecoveryBuildFields, control: &ExecutionControl) -> Result<Self, Error> {
        let prepared = fields.prepare()?;
        let first = ForwardBoard::from_mask(fields.initial.union(fields.middle));
        let first_full = (0..fields.height).fold(0_u32, |m, y| {
            m | if first.row_bits(10, y) == 1023 {
                1 << y
            } else {
                0
            }
        });
        let mut result_to_logical = Vec::new();
        let mut logical_to_result = vec![None; prepared.middle.len()];
        let mut logical = 0_usize;
        for physical in 0..fields.height {
            while logical < 32 && first_full & (1 << logical) != 0 {
                logical += 1;
            }
            result_to_logical.push(logical as u8);
            logical_to_result[logical] = Some(physical);
            logical += 1;
        }
        // The first-stage catalog may use every logical row that the complete
        // TWO-stage target can clear. Using the isolated middle-only clear-row
        // set here would incorrectly reject repair enabled by an early block.
        let h1 = occupied_height(fields.middle);
        let mut context = fields.initial;
        for y in 0..h1 {
            for x in 0..10 {
                if prepared.result[usize::from(y)] & (1 << x) != 0 {
                    context = context.union(
                        Mask::singleton(u16::from(y) * 10 + x)
                            .map_err(|_| Error::BoardOutsideField)?,
                    );
                }
            }
        }
        let clip1 = Mask::all_cells(u16::from(h1) * 10).map_err(|_| Error::BoardOutsideField)?;
        context = Mask::from_words(core::array::from_fn(|w| {
            context.words()[w] & clip1.words()[w]
        }));
        let middle_domain = BuildStageDomain::compile(field(h1, context, fields.middle)?, control)
            .map_err(domain_error)?;
        let (base2, _, _) = place_and_clear(10, fields.height, first);
        let h2 = occupied_height(fields.result);
        let clip2 = Mask::all_cells(u16::from(h2) * 10).map_err(|_| Error::BoardOutsideField)?;
        let base2 = Mask::from_words(core::array::from_fn(|w| {
            base2.words()[w] & clip2.words()[w]
        }));
        let result_domain = BuildStageDomain::compile(field(h2, base2, fields.result)?, control)
            .map_err(domain_error)?;
        Ok(Self {
            fields,
            prepared,
            middle_domain,
            result_domain,
            result_to_logical,
            logical_to_result,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct Position {
    pub stage: u8,
    pub board: ForwardBoard,
    pub middle: Mask,
    pub result: Mask,
    pub deleted: u64,
    pub b2b: bool,
}
impl Position {
    pub fn middle_count(self) -> usize {
        self.middle.count_ones() as usize / 4
    }
    pub fn result_count(self) -> usize {
        self.result.count_ones() as usize / 4
    }
}
#[derive(Clone, Copy)]
pub(super) struct Edge {
    pub next: u32,
    pub result: bool,
    pub lock: ReachableLock,
    pub cleared_rows: u32,
    pub cleared_lines: u8,
    pub spin: bool,
}
struct Entry {
    position: Position,
    edges: [Option<Arc<[Edge]>>; 7],
}
pub(in crate::recovery_build) struct Geometry {
    pub stages: Vec<Stage>,
    pub roots: Vec<u32>,
    entries: Vec<Entry>,
    unique: HashMap<Position, u32>,
    reach: ReachabilityWorkspace,
    physical_locks: HashMap<(ForwardBoard, usize), Arc<[ReachableLock]>>,
    physical_lock_count: usize,
    profile: SpinProfile,
    preserve: bool,
    pub lock_queries: u128,
    pub cache_hits: u128,
}
impl Geometry {
    pub fn new(query: &RecoveryBuildQuery, control: &ExecutionControl) -> Result<Self, Error> {
        cancelled(control)?;
        query.validate()?;
        let mut fields = vec![query.fields.clone()];
        let (base2, _, _) = place_and_clear(
            10,
            query.fields.height,
            ForwardBoard::from_mask(query.fields.initial.union(query.fields.middle)),
        );
        // Exactly the existing Build mirror applicability rule, without a new
        // user switch. Each target is actually verified on the original board;
        // we do not presume that a kick table or a held piece is mirror-invariant.
        let target = field(query.fields.height, as_mask(base2), query.fields.result)?
            .with_horizontal_mirror_included(true);
        if target.includes_applicable_horizontal_mirror() {
            let mirrored = target.mirrored_horizontally().target();
            if mirrored != query.fields.result {
                let mut other = query.fields.clone();
                other.result = mirrored;
                fields.push(other);
            }
        }
        let stages = fields
            .into_iter()
            .map(|f| Stage::compile(f, control))
            .collect::<Result<Vec<_>, _>>()?;
        let mut result = Self {
            stages,
            roots: Vec::new(),
            entries: Vec::new(),
            unique: HashMap::new(),
            reach: ReachabilityWorkspace::new(query.fields.height, query.rule_profile)
                .map_err(|_| Error::UnsupportedRuleProfile)?,
            physical_locks: HashMap::new(),
            physical_lock_count: 0,
            profile: SpinProfile::builtin(query.spin_profile),
            preserve: query.preserve_b2b,
            lock_queries: 0,
            cache_hits: 0,
        };
        for i in 0..result.stages.len() {
            let p = &result.stages[i].prepared;
            let root = Position {
                stage: i as u8,
                board: p.initial,
                middle: Mask::EMPTY,
                result: Mask::EMPTY,
                deleted: p
                    .initially_deleted
                    .iter()
                    .enumerate()
                    .fold(0, |m, (y, &gone)| m | if gone { 1 << y } else { 0 }),
                b2b: query.initial_b2b,
            };
            let root = result.intern(root)?;
            result.roots.push(root);
        }
        Ok(result)
    }
    fn intern(&mut self, position: Position) -> Result<u32, Error> {
        if let Some(&id) = self.unique.get(&position) {
            return Ok(id);
        }
        let id = u32::try_from(self.entries.len()).map_err(|_| Error::CounterOverflow)?;
        self.entries
            .try_reserve(1)
            .map_err(|_| Error::MemoryUnavailable)?;
        self.unique
            .try_reserve(1)
            .map_err(|_| Error::MemoryUnavailable)?;
        self.entries.push(Entry {
            position,
            edges: core::array::from_fn(|_| None),
        });
        self.unique.insert(position, id);
        Ok(id)
    }
    pub fn position(&self, id: u32) -> Position {
        self.entries[id as usize].position
    }
    pub fn feasible(
        &mut self,
        id: u32,
        middle_caps: [u8; 7],
        result_caps: [u8; 7],
        control: &ExecutionControl,
    ) -> Result<bool, Error> {
        let pos = self.position(id);
        let stage = &mut self.stages[usize::from(pos.stage)];
        Ok(stage
            .middle_domain
            .can_complete(
                stage.fields.middle.without(pos.middle),
                middle_caps,
                control,
            )
            .map_err(domain_error)?
            && stage
                .result_domain
                .can_complete(
                    stage.fields.result.without(pos.result),
                    result_caps,
                    control,
                )
                .map_err(domain_error)?)
    }
    /// Height, rotation profile and reachability flags are immutable for
    /// this query-owned Geometry. Physical lock evidence depends on
    /// the full actual board and piece, not stage ownership, B2B,
    /// logical row labels or the queue. Those conditions are still
    /// checked below for EVERY logical edge. Keep every returned lock
    /// and its original first-success/immobility evidence unchanged.
    fn physical_locks(
        &mut self,
        board: ForwardBoard,
        piece: usize,
    ) -> Result<Arc<[ReachableLock]>, Error> {
        if let Some(locks) = self.physical_locks.get(&(board, piece)) {
            return Ok(Arc::clone(locks));
        }
        let locks: Arc<[ReachableLock]> = self
            .reach
            .reachable_locks(board, PIECES[piece], true, true)
            .to_vec()
            .into();
        // This is an evictable memo, never a search limit. A miss or
        // eviction recomputes the complete exact reachability result.
        const MAX_ENTRIES: usize = 4096;
        const MAX_LOCKS: usize = 8 * 1024 * 1024 / size_of::<ReachableLock>();
        if self.physical_locks.len() >= MAX_ENTRIES
            || self.physical_lock_count + locks.len() > MAX_LOCKS
        {
            self.physical_locks.clear();
            self.physical_lock_count = 0;
        }
        if locks.len() <= MAX_LOCKS {
            self.physical_locks
                .try_reserve(1)
                .map_err(|_| Error::MemoryUnavailable)?;
            self.physical_lock_count += locks.len();
            self.physical_locks
                .insert((board, piece), Arc::clone(&locks));
        }
        Ok(locks)
    }
    pub fn edges(
        &mut self,
        id: u32,
        piece: usize,
        control: &ExecutionControl,
    ) -> Result<Arc<[Edge]>, Error> {
        if let Some(edges) = &self.entries[id as usize].edges[piece] {
            self.cache_hits += 1;
            return Ok(Arc::clone(edges));
        }
        cancelled(control)?;
        let pos = self.position(id);
        let height = self.stages[usize::from(pos.stage)].fields.height;
        let logical_height = self.stages[usize::from(pos.stage)].prepared.middle.len();
        let map = (0..logical_height)
            .filter(|&row| pos.deleted & (1 << row) == 0)
            .collect::<Vec<_>>();
        self.lock_queries += 1;
        let locks = self.physical_locks(pos.board, piece)?;
        let mut output = Vec::new();
        for &lock in locks.iter() {
            cancelled(control)?;
            let stage = &self.stages[usize::from(pos.stage)];
            let mut middle = Mask::EMPTY;
            let mut result = Mask::EMPTY;
            let mut valid_first = true;
            let mut valid_second = true;
            for y in 0..height {
                let bits = lock.mask.row_bits(10, y);
                if bits == 0 {
                    continue;
                }
                let Some(&logical) = map.get(usize::from(y)) else {
                    valid_first = false;
                    valid_second = false;
                    break;
                };
                for x in 0..10_u16 {
                    if bits & (1 << x) == 0 {
                        continue;
                    }
                    if logical < usize::from(height) {
                        let cell = logical as u16 * 10 + x;
                        if stage.fields.middle.contains_index(cell)
                            && !pos.middle.contains_index(cell)
                        {
                            middle = middle.union(
                                Mask::singleton(cell).map_err(|_| Error::BoardOutsideField)?,
                            );
                        } else {
                            valid_first = false;
                        }
                    } else {
                        valid_first = false;
                    }
                    if let Some(row) = stage.logical_to_result[logical] {
                        let cell = u16::from(row) * 10 + x;
                        if stage.fields.result.contains_index(cell)
                            && !pos.result.contains_index(cell)
                        {
                            result = result.union(
                                Mask::singleton(cell).map_err(|_| Error::BoardOutsideField)?,
                            );
                        } else {
                            valid_second = false;
                        }
                    } else {
                        valid_second = false;
                    }
                }
            }
            if !valid_first && !valid_second {
                continue;
            }
            let (board, cleared_rows, cleared_lines) =
                place_and_clear(10, height, pos.board.union_for_height(lock.mask, height));
            let (corners, front) = t_corner_counts(
                pos.board,
                height,
                PIECES[piece],
                lock.rotation,
                lock.x,
                lock.y,
            );
            let pc = cleared_lines > 0 && board.is_empty();
            let scoring = ScoringExecutionEdge::new(
                0,
                0,
                PIECES[piece],
                lock.rotation,
                lock.x,
                lock.y,
                cleared_lines,
                corners,
                front,
                lock.evidence.scoring(lock.rotation, lock.immobile),
            )
            .with_perfect_clear(pc);
            let spin =
                SpinDetector::detect_scoring_edge_with_profile(scoring, self.profile).is_some();
            if self.preserve && !BackToBackPreservationPolicy::new(self.profile).allows(scoring) {
                continue;
            }
            let mut deleted = pos.deleted;
            for (physical, &logical) in map.iter().enumerate() {
                if physical < 32 && cleared_rows & (1 << physical) != 0 {
                    deleted |= 1 << logical;
                }
            }
            let next = Position {
                stage: pos.stage,
                board,
                middle: if valid_first {
                    pos.middle.union(middle)
                } else {
                    pos.middle
                },
                result: if valid_second {
                    pos.result.union(result)
                } else {
                    pos.result
                },
                deleted,
                b2b: if cleared_lines == 0 {
                    pos.b2b
                } else {
                    cleared_lines == 4 || pc || spin
                },
            };
            let next = self.intern(next)?;
            output
                .try_reserve(1)
                .map_err(|_| Error::MemoryUnavailable)?;
            output.push(Edge {
                next,
                result: valid_second,
                lock,
                cleared_rows,
                cleared_lines,
                spin,
            });
        }
        let edges: Arc<[Edge]> = output.into();
        self.entries[id as usize].edges[piece] = Some(Arc::clone(&edges));
        Ok(edges)
    }
    pub fn step(
        &self,
        from: u32,
        edge: Edge,
        source_index: usize,
        decision: &'static str,
        piece: usize,
    ) -> RecoveryBuildStep {
        let before = self.position(from);
        let after = self.position(edge.next);
        let stage = &self.stages[usize::from(before.stage)];
        let mask = if edge.result {
            after.result.without(before.result)
        } else {
            after.middle.without(before.middle)
        };
        let mut logical_cells = vec![0_u16; stage.prepared.middle.len()];
        for y in 0..stage.fields.height {
            let logical = if edge.result {
                stage.result_to_logical[usize::from(y)]
            } else {
                y
            };
            logical_cells[usize::from(logical)] = ForwardBoard::from_mask(mask).row_bits(10, y);
        }
        RecoveryBuildStep {
            source_index,
            result_target: edge.result,
            piece: PIECES[piece],
            rotation: edge.lock.rotation.quarter_turns(),
            x: edge.lock.x,
            y: edge.lock.y,
            hold_decision: decision,
            board_before: before.board.words(),
            placement: edge.lock.mask.words(),
            board_after: after.board.words(),
            cleared_rows: edge.cleared_rows,
            cleared_lines: edge.cleared_lines,
            recognized_spin: edge.spin,
            b2b_active: after.b2b,
            middle_complete: after.middle_count() == stage.prepared.middle_pieces,
            logical_cells,
        }
    }
}

#[cfg(test)]
mod physical_lock_tests {
    use super::*;
    use clearra_rules::profile::rule_profile::RuleProfileId;

    #[test]
    fn recovery_build_physical_cache_preserves_every_lock_and_evidence() {
        let control = ExecutionControl::default();
        for height in [4_u8, 10] {
            let query = RecoveryBuildQuery {
                fields: RecoveryBuildFields {
                    height,
                    initial: Mask::EMPTY,
                    middle: Mask::from_words([15, 0, 0, 0]),
                    result: Mask::from_words([0xc030, 0, 0, 0]),
                },
                first_supply: "I".into(),
                second_supply: "O".into(),
                early_limit: crate::CrossStageEarlyLimit::Auto,
                allow_piece_exchange: true,
                hold_enabled: true,
                preserve_b2b: false,
                initial_b2b: true,
                rule_profile: RuleProfileId::SrsPlus,
                spin_profile: clearra_scoring::profile::SpinProfileId::AllSpinPlus,
            };
            let mut cached = Geometry::new(&query, &control).unwrap();
            let mut direct = ReachabilityWorkspace::new(height, query.rule_profile).unwrap();
            let mut seed = 0x9215_2740_0119_ab17_u64;
            for _ in 0..48 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let board = ForwardBoard::from_words([seed & ((1_u64 << 40) - 1), 0, 0, 0]);
                for piece in 0..7 {
                    let expected = direct
                        .reachable_locks(board, PIECES[piece], true, true)
                        .to_vec();
                    let first = cached.physical_locks(board, piece).unwrap();
                    let second = cached.physical_locks(board, piece).unwrap();
                    assert_eq!(first.as_ref(), expected.as_slice());
                    assert_eq!(second.as_ref(), expected.as_slice());
                    assert!(Arc::ptr_eq(&first, &second));
                }
            }
            // Force the optional cache's capacity boundary. Previously
            // returned evidence stays valid and the next miss computes
            // its complete result instead of rejecting a search state.
            cached.physical_lock_count = usize::MAX / 2;
            let expected = direct
                .reachable_locks(ForwardBoard::EMPTY, PIECES[0], true, true)
                .to_vec();
            let actual = cached.physical_locks(ForwardBoard::EMPTY, 0).unwrap();
            assert_eq!(actual.as_ref(), expected.as_slice());
            assert_eq!(cached.physical_locks.len(), 1);
            assert_eq!(cached.physical_lock_count, actual.len());
        }
    }
}
