use clearra_core_domain::board::extended_pc_state_masks::{
    ExtendedPcDeletedRowMask, ExtendedPcOperationBitSet, ExtendedPcStateMaskError,
};
use clearra_geometry::layout::standard_pc_layout::{
    StandardPcGeometryAlgorithm, StandardPcRuntimeCapability, StandardPcSearchContractKind,
    StandardPcStateLayoutContract, StandardPcStateLayoutError,
};
use clearra_pc_graph::request::{ExtendedPcScenarioBoard, ExtendedPcScenarioQuery};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtendedPcSearchContract {
    query: ExtendedPcScenarioQuery,
    state_layout: StandardPcStateLayoutContract,
}

impl ExtendedPcSearchContract {
    pub fn compile(query: ExtendedPcScenarioQuery) -> Result<Self, ExtendedPcSearchContractError> {
        let state_layout =
            StandardPcStateLayoutContract::compile(query.initial_board().visible_height())
                .map_err(ExtendedPcSearchContractError::StateLayout)?;
        if state_layout.contract_kind() != StandardPcSearchContractKind::ExtendedBoardWords {
            return Err(ExtendedPcSearchContractError::CompactBoardContractRequired);
        }
        Ok(Self {
            query,
            state_layout,
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
}
