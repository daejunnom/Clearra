use super::{
    field::{available, joined, PreparedFields},
    RecoveryBuildError, RecoveryBuildFields,
};
use crate::{
    board::{place_and_clear, ForwardBoard},
    cross_stage_recovery::CrossStageEarlyLimit,
    reachability::ReachabilityWorkspace,
    search::t_corner_counts,
};
use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_replay::ScoringExecutionEdge;
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::{
    b2b_preservation::BackToBackPreservationPolicy,
    event::SpinDetector,
    profile::{SpinProfile, SpinProfileId},
};
use std::collections::HashSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryBuildFixedQuery {
    pub fields: RecoveryBuildFields,
    pub first_supply: Vec<PieceKind>,
    pub second_supply: Vec<PieceKind>,
    pub early_limit: CrossStageEarlyLimit,
    /// false preserves each supply's piece multiset; true allows a different
    /// type to repay a borrowed piece. Each actual piece keeps its real shape.
    pub allow_piece_exchange: bool,
    pub hold_enabled: bool,
    pub preserve_b2b: bool,
    pub initial_b2b: bool,
    pub rule_profile: RuleProfileId,
    pub spin_profile: SpinProfileId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryBuildStatus {
    Normal,
    Recovery,
    NoPath,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryBuildStep {
    pub source_index: usize,
    pub result_target: bool,
    pub piece: PieceKind,
    pub rotation: u8,
    pub x: i8,
    pub y: i8,
    pub hold_decision: &'static str,
    pub board_before: [u64; 4],
    pub placement: [u64; 4],
    pub board_after: [u64; 4],
    pub cleared_rows: u32,
    pub cleared_lines: u8,
    pub recognized_spin: bool,
    pub b2b_active: bool,
    pub middle_complete: bool,
    /// Persistent target cells, including cells already removed by line clears.
    /// One 10-bit occupancy mask per original logical row (not cell indices).
    pub logical_cells: Vec<u16>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryBuildFixedReport {
    pub middle_target: [u64; 4],
    pub stage_targets: Vec<[u64; 4]>,
    pub stage_source_lengths: Vec<u16>,
    pub stage_early_counts: Vec<u8>,
    pub status: RecoveryBuildStatus,
    pub states: usize,
    pub effective_max_early: usize,
    pub actual_early: usize,
    /// First-supply usage minus middle-target usage, in IJLOSTZ order.
    pub exchange_balance: [i16; 7],
    pub steps: Vec<RecoveryBuildStep>,
    pub terminal_board: [u64; 4],
    pub result_target: [u64; 4],
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Token {
    index: usize,
    piece: PieceKind,
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct State {
    board: ForwardBoard,
    middle: Vec<u16>,
    result: Vec<u16>,
    deleted: Vec<bool>,
    active: Option<Token>,
    hold: Option<Token>,
    next: usize,
    middle_count: usize,
    result_count: usize,
    first_used: usize,
    early_count: usize,
    exchange: [i16; 7],
    b2b: bool,
}
struct Transition {
    state: State,
    step: RecoveryBuildStep,
}
struct Frame {
    children: std::vec::IntoIter<Transition>,
}

pub(super) fn piece_index(piece: PieceKind) -> usize {
    match piece {
        PieceKind::I => 0,
        PieceKind::J => 1,
        PieceKind::L => 2,
        PieceKind::O => 3,
        PieceKind::S => 4,
        PieceKind::T => 5,
        PieceKind::Z => 6,
    }
}
fn cancelled(control: &ExecutionControl) -> Result<(), RecoveryBuildError> {
    if control.is_cancelled() {
        Err(RecoveryBuildError::Cancelled)
    } else {
        Ok(())
    }
}
impl RecoveryBuildFixedQuery {
    fn token(&self, index: usize) -> Option<Token> {
        self.first_supply
            .get(index)
            .or_else(|| {
                index
                    .checked_sub(self.first_supply.len())
                    .and_then(|i| self.second_supply.get(i))
            })
            .copied()
            .map(|piece| Token { index, piece })
    }
    pub fn search(
        &self,
        control: &ExecutionControl,
    ) -> Result<RecoveryBuildFixedReport, RecoveryBuildError> {
        self.search_with_filter(control, &|_, _, _| true)
    }
    // The filter is used only by independent regression fixtures to require a
    // specific colored drawing; the public search never filters candidates.
    pub(super) fn search_with_filter(
        &self,
        control: &ExecutionControl,
        accept: &impl Fn(PieceKind, bool, &[u16]) -> bool,
    ) -> Result<RecoveryBuildFixedReport, RecoveryBuildError> {
        cancelled(control)?;
        if self.first_supply.is_empty() || self.second_supply.is_empty() {
            return Err(RecoveryBuildError::EmptySupply);
        }
        self.first_supply
            .len()
            .checked_add(self.second_supply.len())
            .ok_or(RecoveryBuildError::CounterOverflow)?;
        let field = self.fields.prepare()?;
        let max = self
            .early_limit
            .effective_max(field.result_pieces, field.result_pieces);
        let mut states = 0_usize;
        for maximum in [0, max] {
            if maximum == 0 && states != 0 {
                break;
            }
            let (found, visited) = self.run_pass(&field, maximum, control, accept)?;
            states = states
                .checked_add(visited)
                .ok_or(RecoveryBuildError::CounterOverflow)?;
            if let Some((steps, terminal)) = found {
                return Ok(RecoveryBuildFixedReport {
                    middle_target: self.fields.middle.words(),
                    stage_targets: Vec::new(),
                    stage_source_lengths: Vec::new(),
                    stage_early_counts: Vec::new(),
                    status: if maximum == 0 {
                        RecoveryBuildStatus::Normal
                    } else {
                        RecoveryBuildStatus::Recovery
                    },
                    states,
                    effective_max_early: max,
                    actual_early: terminal.early_count,
                    exchange_balance: terminal.exchange,
                    steps,
                    terminal_board: field.terminal.words(),
                    result_target: self.fields.result.words(),
                });
            }
        }
        Ok(RecoveryBuildFixedReport {
            middle_target: self.fields.middle.words(),
            stage_targets: Vec::new(),
            stage_source_lengths: Vec::new(),
            stage_early_counts: Vec::new(),
            status: RecoveryBuildStatus::NoPath,
            states,
            effective_max_early: max,
            actual_early: 0,
            exchange_balance: [0; 7],
            steps: Vec::new(),
            terminal_board: field.terminal.words(),
            result_target: self.fields.result.words(),
        })
    }
    fn run_pass(
        &self,
        field: &PreparedFields,
        maximum: usize,
        control: &ExecutionControl,
        accept: &impl Fn(PieceKind, bool, &[u16]) -> bool,
    ) -> Result<(Option<(Vec<RecoveryBuildStep>, State)>, usize), RecoveryBuildError> {
        let mut reach = ReachabilityWorkspace::new(field.height, self.rule_profile)
            .map_err(|_| RecoveryBuildError::UnsupportedRuleProfile)?;
        let root = State {
            board: field.initial,
            middle: vec![0; field.middle.len()],
            result: vec![0; field.result.len()],
            deleted: field.initially_deleted.clone(),
            active: self.token(0),
            hold: None,
            next: 1,
            middle_count: 0,
            result_count: 0,
            first_used: 0,
            early_count: 0,
            exchange: [0; 7],
            b2b: self.initial_b2b,
        };
        let mut seen = HashSet::new();
        seen.try_reserve(1)
            .map_err(|_| RecoveryBuildError::MemoryUnavailable)?;
        seen.insert(root.clone());
        let mut frames = vec![Frame {
            children: self
                .transitions(&root, field, maximum, &mut reach, control, accept)?
                .into_iter(),
        }];
        let mut path = Vec::new();
        while let Some(frame) = frames.last_mut() {
            cancelled(control)?;
            let Some(transition) = frame.children.next() else {
                frames.pop();
                if !frames.is_empty() {
                    path.pop();
                }
                continue;
            };
            let next = transition.state;
            path.try_reserve(1)
                .map_err(|_| RecoveryBuildError::MemoryUnavailable)?;
            path.push(transition.step);
            if next.middle_count == field.middle_pieces && next.result_count == field.result_pieces
            {
                // Different-kind repayment changes stage TYPE counts, never
                // source consumption, total piece count, or the piece's shape.
                if next.board == field.terminal
                    && next.first_used == field.middle_pieces
                    && (self.allow_piece_exchange || next.exchange == [0; 7])
                    && (maximum == 0 || next.early_count > 0)
                {
                    return Ok((Some((path, next)), seen.len()));
                }
                path.pop();
                continue;
            }
            if seen.contains(&next) {
                path.pop();
                continue;
            }
            seen.try_reserve(1)
                .map_err(|_| RecoveryBuildError::MemoryUnavailable)?;
            seen.insert(next.clone());
            frames
                .try_reserve(1)
                .map_err(|_| RecoveryBuildError::MemoryUnavailable)?;
            frames.push(Frame {
                children: self
                    .transitions(&next, field, maximum, &mut reach, control, accept)?
                    .into_iter(),
            });
        }
        Ok((None, seen.len()))
    }
    fn transitions(
        &self,
        state: &State,
        field: &PreparedFields,
        maximum: usize,
        reach: &mut ReachabilityWorkspace,
        control: &ExecutionControl,
        accept: &impl Fn(PieceKind, bool, &[u16]) -> bool,
    ) -> Result<Vec<Transition>, RecoveryBuildError> {
        let mut choices = Vec::new();
        if let Some(active) = state.active {
            choices.push((active, state.hold, state.next, "none"));
            if self.hold_enabled {
                if let Some(held) = state.hold {
                    choices.push((held, Some(active), state.next, "swap"));
                } else if let Some(next) = self.token(state.next) {
                    choices.push((next, Some(active), state.next + 1, "store"));
                }
            }
        } else if let Some(held) = state.hold {
            choices.push((held, None, state.next, "release-held-at-terminal"));
        }
        let mut output = Vec::new();
        for (token, hold, next_index, decision) in choices {
            cancelled(control)?;
            let first_source = token.index < self.first_supply.len();
            if first_source && state.first_used == field.middle_pieces {
                continue;
            }
            for lock in reach
                .reachable_locks(state.board, token.piece, true, true)
                .to_vec()
            {
                cancelled(control)?;
                let Some(logical) = field.lift(lock.mask, &state.deleted) else {
                    continue;
                };
                let first = available(&logical, &field.middle, &state.middle);
                let second = available(&logical, &field.result, &state.result);
                if !first && !second {
                    continue;
                }
                let before_checkpoint = state.middle_count < field.middle_pieces;
                if second && maximum == 0 && before_checkpoint {
                    continue;
                }
                let early = second && before_checkpoint;
                if early && state.early_count == maximum {
                    continue;
                }
                if !accept(token.piece, second, &logical) {
                    continue;
                }
                let (board_after, cleared_rows, cleared_lines) = place_and_clear(
                    10,
                    field.height,
                    state.board.union_for_height(lock.mask, field.height),
                );
                let (corners, front) = t_corner_counts(
                    state.board,
                    field.height,
                    token.piece,
                    lock.rotation,
                    lock.x,
                    lock.y,
                );
                let pc = cleared_lines > 0 && board_after.is_empty();
                let edge = ScoringExecutionEdge::new(
                    0,
                    0,
                    token.piece,
                    lock.rotation,
                    lock.x,
                    lock.y,
                    cleared_lines,
                    corners,
                    front,
                    lock.evidence.scoring(lock.rotation, lock.immobile),
                )
                .with_perfect_clear(pc);
                let profile = SpinProfile::builtin(self.spin_profile);
                let spin = SpinDetector::detect_scoring_edge_with_profile(edge, profile).is_some();
                if self.preserve_b2b && !BackToBackPreservationPolicy::new(profile).allows(edge) {
                    continue;
                }
                let b2b = if cleared_lines == 0 {
                    state.b2b
                } else {
                    cleared_lines == 4 || pc || spin
                };
                let mut exchange = state.exchange;
                exchange[piece_index(token.piece)] += i16::from(first_source) - i16::from(first);
                let active = self.token(next_index);
                let next = State {
                    board: board_after,
                    middle: if first {
                        joined(&state.middle, &logical)
                    } else {
                        state.middle.clone()
                    },
                    result: if second {
                        joined(&state.result, &logical)
                    } else {
                        state.result.clone()
                    },
                    deleted: field.delete_rows(cleared_rows, &state.deleted),
                    active,
                    hold,
                    next: next_index + usize::from(active.is_some()),
                    middle_count: state.middle_count + usize::from(first),
                    result_count: state.result_count + usize::from(second),
                    first_used: state.first_used + usize::from(first_source),
                    early_count: state.early_count + usize::from(early),
                    exchange,
                    b2b,
                };
                let step = RecoveryBuildStep {
                    source_index: token.index,
                    result_target: second,
                    piece: token.piece,
                    rotation: lock.rotation.quarter_turns(),
                    x: lock.x,
                    y: lock.y,
                    hold_decision: decision,
                    board_before: state.board.words(),
                    placement: lock.mask.words(),
                    board_after: board_after.words(),
                    cleared_rows,
                    cleared_lines,
                    recognized_spin: spin,
                    b2b_active: b2b,
                    middle_complete: next.middle_count == field.middle_pieces,
                    logical_cells: logical,
                };
                output
                    .try_reserve(1)
                    .map_err(|_| RecoveryBuildError::MemoryUnavailable)?;
                output.push(Transition { state: next, step });
            }
        }
        Ok(output)
    }
}
