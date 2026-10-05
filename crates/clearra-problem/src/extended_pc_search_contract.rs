use clearra_core_domain::board::extended_pc_state_masks::{
    ExtendedPcDeletedRowMask, ExtendedPcOperationBitSet, ExtendedPcStateMaskError,
};
use clearra_geometry::layout::standard_pc_layout::{
    StandardPcGeometryAlgorithm, StandardPcRuntimeCapability, StandardPcSearchContractKind,
    StandardPcStateLayoutContract, StandardPcStateLayoutError,
};
use clearra_pc_graph::request::{
    ExtendedPcScenarioBoard, ExtendedPcScenarioBoardError, ExtendedPcScenarioQuery,
    PcScenarioBoard, PcScenarioQuery, PcScenarioTargetFrame, PcScenarioTargetFrameError,
};

use crate::{
    BuildProbabilityField, BuildProbabilityFieldError, ProblemCompileError, ProblemCompiler,
    SearchProblem,
};

/// Typed bridge to the existing four-word ILC/BuildUp implementation. This is
/// execution input, not a declaration that every PC product reducer supports
/// extended identities. The original target frame remains bound to its Core
/// problem instead of replacing the user's board with a compact placeholder.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtendedPcExecutionProblem {
    problem: SearchProblem,
    field: BuildProbabilityField,
    target_frame: PcScenarioTargetFrame,
}

impl ExtendedPcExecutionProblem {
    pub fn problem(&self) -> &SearchProblem {
        &self.problem
    }

    pub const fn field(&self) -> BuildProbabilityField {
        self.field
    }

    pub fn target_frame(&self) -> &PcScenarioTargetFrame {
        &self.target_frame
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtendedPcSearchContract {
    query: ExtendedPcScenarioQuery,
    state_layout: StandardPcStateLayoutContract,
    target_frame: PcScenarioTargetFrame,
}

impl ExtendedPcSearchContract {
    pub fn compile(query: ExtendedPcScenarioQuery) -> Result<Self, ExtendedPcSearchContractError> {
        let state_layout =
            StandardPcStateLayoutContract::compile(query.initial_board().visible_height())
                .map_err(ExtendedPcSearchContractError::StateLayout)?;
        if state_layout.contract_kind() != StandardPcSearchContractKind::ExtendedBoardWords {
            return Err(ExtendedPcSearchContractError::CompactBoardContractRequired);
        }
        let board = query.initial_board();
        let target_frame = PcScenarioBoard::standard_10_from_words(
            u16::from(board.visible_height()),
            board.occupied_words(),
        )
        .map_err(ExtendedPcSearchContractError::TargetFrame)?
        .to_standard_target_frame(state_layout.target_lines())
        .map_err(ExtendedPcSearchContractError::TargetFrame)?;
        Ok(Self {
            query,
            state_layout,
            target_frame,
        })
    }

    /// Binds the shared PC input to its explicit extended target. Input-field
    /// height is not a substitute for target height: initial full rows may be
    /// cleared, and a short initial field may be searched inside a taller PC.
    /// Every non-board query policy is moved unchanged, without a compact
    /// placeholder or a second normalization that loses the initial clear.
    ///
    /// This is a compiler boundary only. It does not enable public extended
    /// reducers, certify CountAll multiplicity, or redirect the 1..=6 fast path.
    pub fn compile_standard_query(
        query: PcScenarioQuery,
        target_lines: u8,
    ) -> Result<Self, ExtendedPcSearchContractError> {
        let state_layout = StandardPcStateLayoutContract::compile(target_lines)
            .map_err(ExtendedPcSearchContractError::StateLayout)?;
        if state_layout.contract_kind() != StandardPcSearchContractKind::ExtendedBoardWords {
            return Err(ExtendedPcSearchContractError::CompactBoardContractRequired);
        }
        let target_frame = query
            .initial_board()
            .to_standard_target_frame(target_lines)
            .map_err(ExtendedPcSearchContractError::TargetFrame)?;
        let board = ExtendedPcScenarioBoard::standard_10_from_words(
            target_lines,
            target_frame.normalized_board().occupied_words(),
        )
        .map_err(ExtendedPcSearchContractError::ExtendedBoard)?;
        Ok(Self {
            query: query.map_initial_board(|_| board),
            state_layout,
            target_frame,
        })
    }

    pub fn query(&self) -> &ExtendedPcScenarioQuery {
        &self.query
    }

    pub fn board(&self) -> ExtendedPcScenarioBoard {
        *self.query.initial_board()
    }

    pub const fn state_layout(&self) -> StandardPcStateLayoutContract {
        self.state_layout
    }

    pub const fn algorithm(&self) -> StandardPcGeometryAlgorithm {
        self.state_layout.algorithm()
    }

    pub const fn runtime_capability(&self) -> StandardPcRuntimeCapability {
        self.state_layout.runtime_capability()
    }

    pub fn execution_problem(
        &self,
    ) -> Result<ExtendedPcExecutionProblem, ExtendedPcSearchContractError> {
        let target_frame = self.target_frame.clone();
        let required_pieces = target_frame.required_pieces();
        let maximum_pieces = self.query.piece_window().max_pieces();
        if maximum_pieces < required_pieces {
            return Err(ExtendedPcSearchContractError::PieceWindowTooShort {
                maximum_pieces,
                required_pieces,
            });
        }
        if self
            .query
            .exact_pieces()
            .is_some_and(|exact| exact != required_pieces)
        {
            return Err(ExtendedPcSearchContractError::ExactPieceCountMismatch {
                requested: self
                    .query
                    .exact_pieces()
                    .expect("checked exact piece count"),
                required_pieces,
            });
        }
        let normalized = target_frame.normalized_board().clone();
        let base = clearra_core_domain::board::standard_pc_board::Board256Mask::from_words(
            normalized.occupied_words(),
        );
        let full = clearra_core_domain::board::standard_pc_board::Board256Mask::all_cells(
            u16::from(self.board().visible_height()) * 10,
        )
        .expect("extended board constructor already validated the target domain");
        let field = BuildProbabilityField::from_words_preserving_height(
            self.board().visible_height(),
            base.words(),
            full.without(base).words(),
        )
        .map_err(ExtendedPcSearchContractError::Field)?;
        let query = self
            .query
            .clone()
            .map_initial_board(|_| normalized)
            .with_exact_pieces(Some(required_pieces));
        let problem = ProblemCompiler::compile_scenario_pc(&query)
            .map_err(ExtendedPcSearchContractError::Problem)?;
        Ok(ExtendedPcExecutionProblem {
            problem,
            field,
            target_frame,
        })
    }

    pub fn deleted_rows(
        &self,
        bits: u32,
    ) -> Result<ExtendedPcDeletedRowMask, ExtendedPcStateMaskError> {
        ExtendedPcDeletedRowMask::from_bits(self.state_layout.target_lines(), bits)
    }

    pub fn remaining_operations(
        &self,
        operation_count: u8,
        bits: u64,
    ) -> Result<ExtendedPcOperationBitSet, ExtendedPcStateMaskError> {
        let maximum = self.state_layout.maximum_placement_count();
        if operation_count > maximum {
            return Err(ExtendedPcStateMaskError::TooManyOperations {
                operation_count,
                maximum,
            });
        }
        ExtendedPcOperationBitSet::from_bits(operation_count, bits)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtendedPcSearchContractError {
    CompactBoardContractRequired,
    StateLayout(StandardPcStateLayoutError),
    TargetFrame(PcScenarioTargetFrameError),
    ExtendedBoard(ExtendedPcScenarioBoardError),
    Field(BuildProbabilityFieldError),
    Problem(ProblemCompileError),
    PieceWindowTooShort {
        maximum_pieces: usize,
        required_pieces: usize,
    },
    ExactPieceCountMismatch {
        requested: usize,
        required_pieces: usize,
    },
}
