use std::{
    fmt,
    time::{Duration, Instant},
};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    operation::operation::OperationId,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_rules::{kicks::KickTableProfileId, profile::rule_profile::RuleProfileId};

use crate::backend::DocumentReachabilityEngine;

use super::{
    build_order_language::{MAX_OPERATION_ORDER_OPERATIONS, MAX_OPERATION_ORDER_TIMEOUT_SECONDS},
    sequence_dependencies::OperationDocumentProblem,
};

/// One operation from the authoritative document after lossless coordinate
/// normalization and successful replay validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalizedOperationSequenceStep {
    pub operation_id: OperationId,
    pub piece: PieceKind,
    pub rotation: RotationState,
    pub centered_x: i16,
    pub centered_y: i16,
    pub normalized_x: i8,
    pub normalized_y: i8,
    pub board_before: u64,
    pub lock_mask: u64,
    pub board_after: u64,
    pub cleared_row_mask: u64,
    pub visited_state_count: usize,
    pub used_first_success_kick: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationSequenceReport {
    pub width: u8,
    pub height: u8,
    pub initial_board: u64,
    pub final_board: u64,
    pub steps: Vec<NormalizedOperationSequenceStep>,
    pub cleared_line_count: usize,
    pub trace_key: u64,
    pub rule_profile: RuleProfileId,
    pub kick_profile: KickTableProfileId,
}

impl OperationSequenceReport {
    /// Stable operation-only representation. It is deliberately independent
    /// of the source container (CTK3 versus Fumen) while retaining every
    /// concrete operation coordinate in document order.
    pub fn canonical_trace(&self) -> String {
        self.steps
            .iter()
            .map(|step| {
                format!(
                    "{}:{}:{}:{}:{}",
                    step.operation_id.0,
                    step.piece.as_ascii(),
                    step.rotation.quarter_turns(),
                    step.centered_x,
                    step.centered_y,
                )
            })
            .collect::<Vec<_>>()
            .join(";")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationSequenceError {
    InvalidInput(&'static str),
    Cancelled,
    TimedOut { timeout_seconds: u16 },
    Incomplete { reason: &'static str },
}

impl fmt::Display for OperationSequenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for OperationSequenceError {}

pub struct OperationSequenceAnalyzer;

impl OperationSequenceAnalyzer {
    pub fn analyze(
        problem: &OperationDocumentProblem,
        control: &ExecutionControl,
    ) -> Result<OperationSequenceReport, OperationSequenceError> {
        validate_problem(problem)?;
        let started = Instant::now();
        let deadline = Duration::from_secs(u64::from(problem.timeout_seconds));
        let mut reachability =
            DocumentReachabilityEngine::new(problem.width, problem.height, problem.kick_profile)
                .ok_or(OperationSequenceError::InvalidInput(
                    "unsupported kick profile or Board64 dimensions",
                ))?;
        let document_boards =
            problem
                .document_boards
                .as_ref()
                .ok_or(OperationSequenceError::InvalidInput(
                    "operation sequence requires every document page board",
                ))?;
        let mut board = problem.initial_board;
        let mut steps = Vec::new();
        steps.try_reserve(problem.operations.len()).map_err(|_| {
            OperationSequenceError::Incomplete {
                reason: "operation sequence allocation failed",
            }
        })?;
        let mut cleared_line_count = 0_usize;
        let mut trace_key = 0xcbf29ce484222325_u64;
        mix(&mut trace_key, u64::from(problem.width));
        mix(&mut trace_key, u64::from(problem.height));
        mix(&mut trace_key, problem.initial_board);

        for (operation_index, operation) in problem.operations.iter().copied().enumerate() {
            poll(control, started, deadline, problem.timeout_seconds)?;
            if document_boards[operation_index] != board {
                return Err(OperationSequenceError::InvalidInput(
                    "document page board does not match concrete operation replay",
                ));
            }
            let result = reachability.analyze_lock(
                board,
                operation.piece,
                operation.rotation,
                operation.x,
                operation.y,
            );
            if !result.valid_target {
                return Err(OperationSequenceError::InvalidInput(
                    "document contains an out-of-bounds concrete operation",
                ));
            }
            if !result.reachable {
                return Err(OperationSequenceError::InvalidInput(
                    "document operation trace is not reachable under the selected kick profile",
                ));
            }
            let (centered_x, centered_y) =
                operation
                    .centered_coordinates()
                    .ok_or(OperationSequenceError::InvalidInput(
                        "document operation coordinates cannot be normalized losslessly",
                    ))?;
            let occupied = board | result.lock_mask;
            let (board_after, cleared_row_mask) =
                clear_full_rows(occupied, problem.width, problem.height);
            cleared_line_count = cleared_line_count
                .checked_add(cleared_row_mask.count_ones() as usize)
                .ok_or(OperationSequenceError::Incomplete {
                    reason: "cleared line count overflow",
                })?;
            let step = NormalizedOperationSequenceStep {
                operation_id: operation.operation_id,
                piece: operation.piece,
                rotation: operation.rotation,
                centered_x,
                centered_y,
                normalized_x: operation.x,
                normalized_y: operation.y,
                board_before: board,
                lock_mask: result.lock_mask,
                board_after,
                cleared_row_mask,
                visited_state_count: result.visited_state_count,
                used_first_success_kick: result.first_success_kick_evidence,
            };
            mix_step(&mut trace_key, step);
            steps.push(step);
            board = board_after;
            control.report_progress(
                "operation_sequence_replay",
                (operation_index + 1) as u64,
                Some(problem.operations.len() as u64),
            );
        }
        poll(control, started, deadline, problem.timeout_seconds)?;

        Ok(OperationSequenceReport {
            width: problem.width,
            height: problem.height,
            initial_board: problem.initial_board,
            final_board: board,
            steps,
            cleared_line_count,
            trace_key,
            rule_profile: problem.rule_profile,
            kick_profile: problem.kick_profile,
        })
    }
}

fn validate_problem(problem: &OperationDocumentProblem) -> Result<(), OperationSequenceError> {
    let cells = usize::from(problem.width)
        .checked_mul(usize::from(problem.height))
        .ok_or(OperationSequenceError::InvalidInput(
            "board dimensions overflow",
        ))?;
    if problem.width == 0 || problem.height == 0 || cells > 64 {
        return Err(OperationSequenceError::InvalidInput(
            "operation sequence requires a non-empty Board64 document",
        ));
    }
    if problem.operations.is_empty() {
        return Err(OperationSequenceError::InvalidInput(
            "document must contain a concrete operation trace",
        ));
    }
    let Some(document_boards) = problem.document_boards.as_ref() else {
        return Err(OperationSequenceError::InvalidInput(
            "operation sequence requires operation-preserving document pages",
        ));
    };
    if document_boards.len() != problem.operations.len() {
        return Err(OperationSequenceError::InvalidInput(
            "document board/page count differs from concrete operation count",
        ));
    }
    if problem.operations.len() > MAX_OPERATION_ORDER_OPERATIONS {
        return Err(OperationSequenceError::InvalidInput(
            "operation sequence exceeds the 4096 operation limit",
        ));
    }
    if !(1..=MAX_OPERATION_ORDER_TIMEOUT_SECONDS).contains(&problem.timeout_seconds) {
        return Err(OperationSequenceError::InvalidInput(
            "operation sequence timeout-seconds must be in 1..=900",
        ));
    }
    if cells < 64 && problem.initial_board >> cells != 0 {
        return Err(OperationSequenceError::InvalidInput(
            "initial board contains cells outside document dimensions",
        ));
    }
    if matches!(problem.rule_profile, RuleProfileId::Custom) {
        return Err(OperationSequenceError::InvalidInput(
            "custom rule profiles require an explicit connected runtime profile",
        ));
    }
    let mut ids: Vec<_> = problem
        .operations
        .iter()
        .map(|operation| operation.operation_id)
        .collect();
    ids.sort_unstable();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(OperationSequenceError::InvalidInput(
            "operation ids must be unique",
        ));
    }
    Ok(())
}

fn clear_full_rows(board: u64, width: u8, height: u8) -> (u64, u64) {
    let complete_row = row_mask(width);
    let mut compacted = 0_u64;
    let mut destination = 0_usize;
    let mut cleared = 0_u64;
    for source in 0..usize::from(height) {
        let row = (board >> (source * usize::from(width))) & complete_row;
        if row == complete_row {
            cleared |= 1_u64 << source;
        } else {
            compacted |= row << (destination * usize::from(width));
            destination += 1;
        }
    }
    (compacted, cleared)
}

fn row_mask(width: u8) -> u64 {
    if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    }
}

fn poll(
    control: &ExecutionControl,
    started: Instant,
    deadline: Duration,
    timeout_seconds: u16,
) -> Result<(), OperationSequenceError> {
    if control.is_cancelled() {
        return Err(OperationSequenceError::Cancelled);
    }
    if started.elapsed() >= deadline {
        return Err(OperationSequenceError::TimedOut { timeout_seconds });
    }
    Ok(())
}

fn mix(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(0x100000001b3);
}

fn mix_step(hash: &mut u64, step: NormalizedOperationSequenceStep) {
    mix(hash, u64::from(step.operation_id.0));
    mix(hash, step.piece.as_ascii() as u64);
    mix(hash, u64::from(step.rotation.quarter_turns()));
    mix(hash, step.centered_x as i64 as u64);
    mix(hash, step.centered_y as i64 as u64);
    mix(hash, step.board_before);
    mix(hash, step.lock_mask);
    mix(hash, step.board_after);
    mix(hash, step.cleared_row_mask);
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        operation::operation::OperationId,
        piece::{piece_kind::PieceKind, rotation::RotationState},
    };

    use super::*;
    use crate::order_language::sequence_dependencies::ConcreteDocumentOperation;

    fn one_o_problem() -> OperationDocumentProblem {
        let operation = ConcreteDocumentOperation::from_centered(
            OperationId(0),
            PieceKind::O,
            RotationState::Zero,
            0,
            0,
        )
        .unwrap();
        let mut problem = OperationDocumentProblem::canonical(10, 4, 0, vec![operation]);
        problem.document_boards = Some(vec![0]);
        problem
    }

    #[test]
    fn normalizes_and_replays_authoritative_trace_without_reordering() {
        let report =
            OperationSequenceAnalyzer::analyze(&one_o_problem(), &ExecutionControl::default())
                .unwrap();
        assert_eq!(report.canonical_trace(), "0:O:0:0:0");
        assert_eq!(report.steps.len(), 1);
        assert_eq!(report.steps[0].board_before, 0);
        assert_eq!(report.steps[0].lock_mask, 0b11 | (0b11 << 10));
        assert_eq!(report.final_board, report.steps[0].lock_mask);
    }

    #[test]
    fn rejects_incomplete_or_divergent_document_pages() {
        let mut missing = one_o_problem();
        missing.document_boards = None;
        assert!(matches!(
            OperationSequenceAnalyzer::analyze(&missing, &ExecutionControl::default()),
            Err(OperationSequenceError::InvalidInput(_))
        ));

        let mut divergent = one_o_problem();
        divergent.document_boards = Some(vec![1]);
        assert!(matches!(
            OperationSequenceAnalyzer::analyze(&divergent, &ExecutionControl::default()),
            Err(OperationSequenceError::InvalidInput(_))
        ));
    }

    #[test]
    fn cancelled_replay_never_publishes_partial_result() {
        let token = clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new();
        token.handle().cancel();
        assert_eq!(
            OperationSequenceAnalyzer::analyze(&one_o_problem(), &ExecutionControl::new(token),),
            Err(OperationSequenceError::Cancelled),
        );
    }
}
