use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_geometry::{
    layout::board64_layout::Board64Layout, placement::placement_mask::PlacementMask,
};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

use crate::{
    board::board64_state::{Board64State, Board64StateError},
    replay::replay_engine::BuildVariantOperation,
    trace::{
        BoardAfterStep, HoldDecision, LineClearEvent, PieceDecision, PlacementStep, SolutionTrace,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionTraceBuilder {
    layout: Board64Layout,
    initial_board: Board64State,
    operations: Vec<BuildVariantOperation>,
    representative_order: Vec<usize>,
    hold_decisions: Vec<HoldDecision>,
}

impl SolutionTraceBuilder {
    pub fn new(
        layout: Board64Layout,
        initial_occupied: u64,
        operations: Vec<BuildVariantOperation>,
        representative_order: Vec<usize>,
    ) -> Result<Self, SolutionTraceBuilderError> {
        let initial_board = Board64State::new(layout, initial_occupied)
            .map_err(SolutionTraceBuilderError::InvalidInitialBoard)?;
        if operations.len() != representative_order.len() {
            return Err(
                SolutionTraceBuilderError::RepresentativeOrderLengthMismatch {
                    operation_count: operations.len(),
                    order_count: representative_order.len(),
                },
            );
        }
        validate_representative_order(operations.len(), &representative_order)?;
        let hold_decisions = vec![HoldDecision::None; operations.len()];
        Ok(Self {
            layout,
            initial_board,
            operations,
            representative_order,
            hold_decisions,
        })
    }
}
impl SolutionTraceBuilder {
    pub fn with_hold_decisions(mut self, hold_decisions: Vec<HoldDecision>) -> Self {
        self.hold_decisions = hold_decisions;
        self
    }
}
impl SolutionTraceBuilder {
    pub fn build(&self) -> Result<SolutionTrace, SolutionTraceBuilderError> {
        if self.hold_decisions.len() != self.operations.len() {
            return Err(SolutionTraceBuilderError::HoldDecisionLengthMismatch {
                operation_count: self.operations.len(),
                decision_count: self.hold_decisions.len(),
            });
        }
        let registry = standard_tetromino_registry();
        let mut board = self.initial_board;
        let mut steps = Vec::with_capacity(self.operations.len());

        for (step_index, operation_index) in self.representative_order.iter().copied().enumerate() {
            let operation = self.operations.get(operation_index).ok_or(
                SolutionTraceBuilderError::RepresentativeOrderIndexOutOfRange {
                    index: operation_index,
                    operation_count: self.operations.len(),
                },
            )?;
            let definition = registry
                .get(operation.piece())
                .ok_or(SolutionTraceBuilderError::UnknownPiece(operation.piece()))?;
            let placement = PlacementMask::new(
                self.layout,
                definition,
                operation.rotation(),
                operation.x(),
                operation.y(),
            )
            .map_err(|_| SolutionTraceBuilderError::PlacementOutOfBounds { operation_index })?;

            if let Some(expected_mask) = operation.expected_mask() {
                if expected_mask != placement.mask() {
                    return Err(SolutionTraceBuilderError::OperationMaskMismatch {
                        operation_index,
                        expected_mask,
                        rebuilt_mask: placement.mask(),
                    });
                }
            }

            if board.occupied() & placement.mask() != 0 {
                return Err(SolutionTraceBuilderError::PlacementCollision {
                    operation_index,
                    occupied: board.occupied(),
                    placement_mask: placement.mask(),
                });
            }

            let after_placement_mask = board.occupied() | placement.mask();
            let after_placement = Board64State::new(self.layout, after_placement_mask)
                .map_err(SolutionTraceBuilderError::InvalidIntermediateBoard)?;
            let clear_result = clear_complete_lines(self.layout, after_placement_mask);
            if let Some(expected_mask) = operation.expected_cleared_row_mask() {
                if expected_mask != clear_result.cleared_row_mask {
                    return Err(SolutionTraceBuilderError::ClearedRowMaskMismatch {
                        operation_index,
                        expected_mask,
                        rebuilt_mask: clear_result.cleared_row_mask,
                    });
                }
            }
            let after_line_clear = Board64State::new(self.layout, clear_result.occupied)
                .map_err(SolutionTraceBuilderError::InvalidIntermediateBoard)?;
            let piece_decision = PieceDecision::new(
                operation.piece(),
                step_index,
                step_index + 1,
                None,
                None,
                self.hold_decisions[step_index],
            );
            let mut placement_step = PlacementStep::new(
                step_index,
                piece_decision,
                placement,
                board,
                BoardAfterStep::new(after_placement, after_line_clear),
                LineClearEvent::new(clear_result.cleared_lines),
            );
            if let Some(operation_id) = operation.operation_id() {
                placement_step = placement_step.with_operation_id(operation_id);
            }
            steps.push(placement_step);
            board = after_line_clear;
        }

        Ok(SolutionTrace::new(steps))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolutionTraceBuilderError {
    InvalidInitialBoard(Board64StateError),
    InvalidIntermediateBoard(Board64StateError),
    RepresentativeOrderLengthMismatch {
        operation_count: usize,
        order_count: usize,
    },
    RepresentativeOrderIndexOutOfRange {
        index: usize,
        operation_count: usize,
    },
    RepresentativeOrderDuplicate {
        index: usize,
    },
    HoldDecisionLengthMismatch {
        operation_count: usize,
        decision_count: usize,
    },
    UnknownPiece(PieceKind),
    PlacementOutOfBounds {
        operation_index: usize,
    },
    OperationMaskMismatch {
        operation_index: usize,
        expected_mask: u64,
        rebuilt_mask: u64,
    },
    ClearedRowMaskMismatch {
        operation_index: usize,
        expected_mask: u16,
        rebuilt_mask: u16,
    },
    PlacementCollision {
        operation_index: usize,
        occupied: u64,
        placement_mask: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LineClearResult {
    occupied: u64,
    cleared_lines: u8,
    cleared_row_mask: u16,
}

fn validate_representative_order(
    operation_count: usize,
    representative_order: &[usize],
) -> Result<(), SolutionTraceBuilderError> {
    let mut seen = vec![false; operation_count];
    for index in representative_order {
        if *index >= operation_count {
            return Err(
                SolutionTraceBuilderError::RepresentativeOrderIndexOutOfRange {
                    index: *index,
                    operation_count,
                },
            );
        }
        if seen[*index] {
            return Err(SolutionTraceBuilderError::RepresentativeOrderDuplicate { index: *index });
        }
        seen[*index] = true;
    }
    Ok(())
}

fn clear_complete_lines(layout: Board64Layout, occupied: u64) -> LineClearResult {
    let width = usize::from(layout.width());
    let height = usize::from(layout.height());
    let mut compacted = 0_u64;
    let mut dest_y = 0_usize;
    let mut cleared_lines = 0_u8;
    let mut cleared_row_mask = 0_u16;

    for source_y in 0..height {
        let row_mask = row_mask(width, source_y);
        if occupied & row_mask == row_mask {
            cleared_lines += 1;
            if source_y < u16::BITS as usize {
                cleared_row_mask |= 1_u16 << source_y;
            }
            continue;
        }

        let row = (occupied & row_mask) >> (source_y * width);
        compacted |= row << (dest_y * width);
        dest_y += 1;
    }

    LineClearResult {
        occupied: compacted,
        cleared_lines,
        cleared_row_mask,
    }
}

fn row_mask(width: usize, y: usize) -> u64 {
    let start = y * width;
    ((1_u64 << width) - 1) << start
}

#[cfg(test)]
#[path = "solution_trace_builder_tests.rs"]
mod tests;
