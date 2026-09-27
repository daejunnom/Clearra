//! Cross-stage search for ONE supplied pair of exact lock-time role sets.
//!
//! This module does not generate Build Probability candidates and does not
//! claim completeness over omitted candidate pairs. It deliberately has no
//! connection to the v1 selected-borrow request or its wire format.
//! Candidate production may ignore B2B; every actual combined edge is checked
//! here. Auto means no user-imposed early-count bound, not infinite supply.

use std::collections::HashSet;

use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
};
use clearra_replay::ScoringExecutionEdge;
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::{
    b2b_preservation::BackToBackPreservationPolicy,
    event::SpinDetector,
    profile::{SpinProfile, SpinProfileId},
};

use crate::{
    board::{place_and_clear, ForwardBoard},
    boundary_recovery::{compact_tag, BoundaryRecoveryStep},
    reachability::ReachabilityWorkspace,
    search::t_corner_counts,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CrossStageEarlyLimit {
    #[default]
    Auto,
    AtMost(usize),
}

impl CrossStageEarlyLimit {
    /// Each early placement consumes a distinct stage-one source token and
    /// fills a distinct stage-two role. This bound removes no legal mapping.
    pub fn effective_max(self, stage_one_supply: usize, stage_two_roles: usize) -> usize {
        let structural = stage_one_supply.min(stage_two_roles);
        match self {
            Self::Auto => structural,
            Self::AtMost(requested) => requested.min(structural),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossStageRole {
    pub piece: PieceKind,
    /// Coordinates at lock time in this candidate, NOT a final-board tiling
    /// mask. An adapter must supply all relevant line-clear realizations.
    pub lock_mask: Board256Mask,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossStageRecoveryQuery {
    pub initial_board: Board256Mask,
    /// Remaining initial/stage-one-role cells at the checkpoint, after clears.
    pub stage_one_target: Board256Mask,
    pub final_board: Board256Mask,
    pub height: u8,
    pub stage_one_supply: Vec<PieceKind>,
    pub stage_two_supply: Vec<PieceKind>,
    pub stage_one_roles: Vec<CrossStageRole>,
    pub stage_two_roles: Vec<CrossStageRole>,
    pub early_limit: CrossStageEarlyLimit,
    pub hold_enabled: bool,
    pub rule_profile: RuleProfileId,
    pub spin_profile: SpinProfileId,
    pub initial_b2b: bool,
    /// Applies to actual edges of BOTH stages, including deferred stage-one
    /// completions. It is not a stage-one candidate-generation constraint.
    pub preserve_b2b: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrossStageSearchError {
    InvalidHeight,
    BoardOutsideField,
    EmptyStage,
    InvalidRole,
    SizeOverflow,
    UnsupportedRuleProfile,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrossStagePairStatus {
    Normal,
    Recovery,
    NoPath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossStagePairReport {
    pub status: CrossStagePairStatus,
    pub requested_early_limit: CrossStageEarlyLimit,
    /// Physical upper bound intersected with the optional user quota.
    pub effective_max_early: usize,
    pub actual_early_placements: usize,
    pub normal_states: usize,
    pub recovery_states: usize,
    pub steps: Vec<BoundaryRecoveryStep>,
    /// True only after every reachable state in this pair's declared scope
    /// was exhausted. A witness proves existence, not all-path enumeration.
    pub pair_exhausted: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Token {
    source: usize,
    piece: PieceKind,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct State {
    board: ForwardBoard,
    stage_one_board: ForwardBoard,
    stage_two_board: ForwardBoard,
    active: Option<Token>,
    hold: Option<Token>,
    next_source: usize,
    filled_roles: Vec<u64>,
    filled_count: usize,
    checkpoint: bool,
    early_count: usize,
    b2b_active: bool,
}

struct Transition {
    state: State,
    step: BoundaryRecoveryStep,
}

struct Frame {
    children: std::vec::IntoIter<Transition>,
}

impl CrossStageRecoveryQuery {
    fn source_count(&self) -> Result<usize, CrossStageSearchError> {
        self.stage_one_supply
            .len()
            .checked_add(self.stage_two_supply.len())
            .ok_or(CrossStageSearchError::SizeOverflow)
    }

    fn role_count(&self) -> Result<usize, CrossStageSearchError> {
        self.stage_one_roles
            .len()
            .checked_add(self.stage_two_roles.len())
            .ok_or(CrossStageSearchError::SizeOverflow)
    }

    fn token(&self, index: usize) -> Option<Token> {
        let piece = if index < self.stage_one_supply.len() {
            self.stage_one_supply.get(index)
        } else {
            self.stage_two_supply
                .get(index - self.stage_one_supply.len())
        };
        piece.copied().map(|piece| Token {
            source: index,
            piece,
        })
    }

    pub(crate) fn validate(&self) -> Result<(), CrossStageSearchError> {
        if self.height == 0 || self.height > 25 {
            return Err(CrossStageSearchError::InvalidHeight);
        }
        self.source_count()?;
        self.role_count()?;
        if self.stage_one_supply.is_empty()
            || self.stage_two_supply.is_empty()
            || self.stage_one_roles.is_empty()
            || self.stage_two_roles.is_empty()
        {
            return Err(CrossStageSearchError::EmptyStage);
        }
        let cells = u16::from(self.height) * 10;
        if [self.initial_board, self.stage_one_target, self.final_board]
            .iter()
            .any(|mask| mask.fits_cell_count(cells) != Ok(true))
        {
            return Err(CrossStageSearchError::BoardOutsideField);
        }
        if self
            .stage_one_roles
            .iter()
            .chain(&self.stage_two_roles)
            .any(|role| {
                role.lock_mask.fits_cell_count(cells) != Ok(true)
                    || role
                        .lock_mask
                        .words()
                        .iter()
                        .map(|word| word.count_ones())
                        .sum::<u32>()
                        != 4
            })
        {
            return Err(CrossStageSearchError::InvalidRole);
        }
        Ok(())
    }

    /// A source may fill a role of either stage. Early use counts only a
    /// first-stage source filling a second-stage role before the checkpoint.
    /// Unused trailing supply is lookahead, not a required role. There is no
    /// source reset or extra hold at the input boundary.
    ///
    /// Complete EXISTENCE search within the supplied exact candidate pair.
    /// Normal paths take precedence. Auto explores all early-count choices
    /// in one recovery pass, not repeated 1, 2, ... capped invocations.
    /// No queue-length or early-count representation is narrowed to u8/u64.
    pub fn search(
        &self,
        control: &ExecutionControl,
    ) -> Result<CrossStagePairReport, CrossStageSearchError> {
        self.validate()?;
        let effective = self
            .early_limit
            .effective_max(self.stage_one_supply.len(), self.stage_two_roles.len());
        let (normal, normal_states) = self.run_pass(control, 0)?;
        if let Some((steps, early)) = normal {
            return Ok(self.report(
                CrossStagePairStatus::Normal,
                effective,
                early,
                normal_states,
                0,
                steps,
            ));
        }
        if effective == 0 {
            return Ok(self.report(
                CrossStagePairStatus::NoPath,
                effective,
                0,
                normal_states,
                0,
                Vec::new(),
            ));
        }
        let (recovery, recovery_states) = self.run_pass(control, effective)?;
        match recovery {
            Some((steps, early)) => Ok(self.report(
                CrossStagePairStatus::Recovery,
                effective,
                early,
                normal_states,
                recovery_states,
                steps,
            )),
            None => Ok(self.report(
                CrossStagePairStatus::NoPath,
                effective,
                0,
                normal_states,
                recovery_states,
                Vec::new(),
            )),
        }
    }

    pub(crate) fn report(
        &self,
        status: CrossStagePairStatus,
        effective: usize,
        actual: usize,
        normal_states: usize,
        recovery_states: usize,
        steps: Vec<BoundaryRecoveryStep>,
    ) -> CrossStagePairReport {
        CrossStagePairReport {
            status,
            requested_early_limit: self.early_limit,
            effective_max_early: effective,
            actual_early_placements: actual,
            normal_states,
            recovery_states,
            steps,
            pair_exhausted: status == CrossStagePairStatus::NoPath,
        }
    }

    pub(crate) fn run_pass(
        &self,
        control: &ExecutionControl,
        maximum: usize,
    ) -> Result<(Option<(Vec<BoundaryRecoveryStep>, usize)>, usize), CrossStageSearchError> {
        check_cancel(control)?;
        let mut reachability = ReachabilityWorkspace::new(self.height, self.rule_profile)
            .map_err(|_| CrossStageSearchError::UnsupportedRuleProfile)?;
        let (initial, _, _) =
            place_and_clear(10, self.height, ForwardBoard::from_mask(self.initial_board));
        let root = State {
            board: initial,
            stage_one_board: initial,
            stage_two_board: ForwardBoard::EMPTY,
            active: self.token(0),
            hold: None,
            next_source: 1,
            filled_roles: vec![0; self.role_count()?.div_ceil(64)],
            filled_count: 0,
            checkpoint: false,
            early_count: 0,
            b2b_active: self.initial_b2b,
        };
        let mut seen = HashSet::new();
        seen.insert(root.clone());
        let mut frames = vec![Frame {
            children: self
                .transitions(&root, maximum, &mut reachability, control)?
                .into_iter(),
        }];
        let mut path = Vec::new();
        while let Some(frame) = frames.last_mut() {
            check_cancel(control)?;
            let Some(transition) = frame.children.next() else {
                frames.pop();
                if !frames.is_empty() {
                    path.pop();
                }
                continue;
            };
            let next = transition.state;
            path.push(transition.step);
            if next.filled_count == self.role_count()? {
                if next.checkpoint
                    && next.board.words() == self.final_board.words()
                    && (maximum == 0 || next.early_count > 0)
                {
                    return Ok((Some((path, next.early_count)), seen.len()));
                }
                path.pop();
                continue;
            }
            // Identity contains role completion, both provenance masks, actual
            // supply/hold state and cumulative early use, never board alone.
            if !seen.insert(next.clone()) {
                path.pop();
                continue;
            }
            frames.push(Frame {
                children: self
                    .transitions(&next, maximum, &mut reachability, control)?
                    .into_iter(),
            });
        }
        Ok((None, seen.len()))
    }

    fn transitions(
        &self,
        state: &State,
        maximum: usize,
        reachability: &mut ReachabilityWorkspace,
        control: &ExecutionControl,
    ) -> Result<Vec<Transition>, CrossStageSearchError> {
        let mut choices = Vec::new();
        if let Some(active) = state.active {
            choices.push((active, state.hold, state.next_source, "none"));
            if self.hold_enabled {
                if let Some(held) = state.hold {
                    choices.push((held, Some(active), state.next_source, "swap"));
                } else if let Some(next) = self.token(state.next_source) {
                    choices.push((next, Some(active), state.next_source + 1, "store"));
                }
            }
        } else if let Some(held) = state.hold {
            // The last held required token does not disappear at finite supply end.
            choices.push((held, None, state.next_source, "release-held-at-terminal"));
        }
        let mut output = Vec::new();
        for (token, held, next_source, decision) in choices {
            check_cancel(control)?;
            let locks = reachability
                .reachable_locks(state.board, token.piece, true, true)
                .to_vec();
            for lock in locks {
                check_cancel(control)?;
                let placed = state.board.union_for_height(lock.mask, self.height);
                let (board_after, cleared_rows, cleared_lines) =
                    place_and_clear(10, self.height, placed);
                let (corners, front) = t_corner_counts(
                    state.board,
                    self.height,
                    token.piece,
                    lock.rotation,
                    lock.x,
                    lock.y,
                );
                let pc = board_after.is_empty() && cleared_lines > 0;
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
                let b2b = if cleared_lines == 0 {
                    state.b2b_active
                } else {
                    cleared_lines == 4 || pc || spin
                };
                // No source/target-stage exception exists in the combined path.
                if self.preserve_b2b && !BackToBackPreservationPolicy::new(profile).allows(edge) {
                    continue;
                }
                for (role_index, role) in self
                    .stage_one_roles
                    .iter()
                    .chain(&self.stage_two_roles)
                    .enumerate()
                {
                    if bit(&state.filled_roles, role_index)
                        || role.piece != token.piece
                        || role.lock_mask.words() != lock.mask.words()
                    {
                        continue;
                    }
                    let second = role_index >= self.stage_one_roles.len();
                    if maximum == 0 && second && !state.checkpoint {
                        continue;
                    }
                    let early =
                        second && !state.checkpoint && token.source < self.stage_one_supply.len();
                    if early && state.early_count >= maximum {
                        continue;
                    }
                    let first_tag = if second {
                        state.stage_one_board
                    } else {
                        state
                            .stage_one_board
                            .union_for_height(lock.mask, self.height)
                    };
                    let second_tag = if second {
                        state
                            .stage_two_board
                            .union_for_height(lock.mask, self.height)
                    } else {
                        state.stage_two_board
                    };
                    let first_tag = compact_tag(first_tag, cleared_rows, self.height);
                    let second_tag = compact_tag(second_tag, cleared_rows, self.height);
                    let mut filled = state.filled_roles.clone();
                    filled[role_index / 64] |= 1_u64 << (role_index % 64);
                    let checkpoint = state.checkpoint
                        || ((0..self.stage_one_roles.len()).all(|index| bit(&filled, index))
                            && first_tag.words() == self.stage_one_target.words());
                    let active = self.token(next_source);
                    let next = State {
                        board: board_after,
                        stage_one_board: first_tag,
                        stage_two_board: second_tag,
                        active,
                        hold: held,
                        next_source: next_source + usize::from(active.is_some()),
                        filled_roles: filled,
                        filled_count: state.filled_count + 1,
                        checkpoint,
                        early_count: state.early_count + usize::from(early),
                        b2b_active: b2b,
                    };
                    debug_assert_eq!(
                        next.board.words(),
                        next.stage_one_board.union(next.stage_two_board).words()
                    );
                    output.push(Transition {
                        state: next,
                        step: BoundaryRecoveryStep {
                            source_queue_index: token.source,
                            placement_role_index: role_index,
                            piece: token.piece,
                            rotation: lock.rotation,
                            x: lock.x,
                            y: lock.y,
                            hold_decision: decision,
                            placement_mask: lock.mask.words(),
                            cleared_row_mask: cleared_rows,
                            board_after: board_after.words(),
                            cleared_lines,
                            recognized_spin: spin,
                            b2b_active_after: b2b,
                            stage_one_complete_after: checkpoint,
                        },
                    });
                }
            }
        }
        Ok(output)
    }
}

fn bit(words: &[u64], index: usize) -> bool {
    words[index / 64] & (1_u64 << (index % 64)) != 0
}
fn check_cancel(control: &ExecutionControl) -> Result<(), CrossStageSearchError> {
    if control.is_cancelled() {
        Err(CrossStageSearchError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "cross_stage_recovery_tests.rs"]
mod tests;
