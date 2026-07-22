use clearra_core_domain::board::standard_pc_board::{
    StandardPcBoardError, StandardPcBoardStorageKind, STANDARD_PC_BOARD_WIDTH,
    STANDARD_PC_COMPACT_MAX_LINES, STANDARD_PC_MAX_LINES,
};

use super::board_backend::BoardBackendKind;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StandardPcGeometryAlgorithm {
    InverseLockClearSkeletonExactCover,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StandardPcSearchContractKind {
    CompactBoard64,
    ExtendedBoardWords,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StandardPcRuntimeCapability {
    ConnectedExact,
    Unsupported(StandardPcRuntimeUnsupportedReason),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StandardPcRuntimeUnsupportedReason {
    ExtendedSearchStagesNotConnected,
}

impl StandardPcRuntimeUnsupportedReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExtendedSearchStagesNotConnected => "extended_search_stages_not_connected",
        }
    }
}

impl StandardPcRuntimeCapability {
    pub const fn connected_exact(self) -> bool {
        matches!(self, Self::ConnectedExact)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StandardPcStateLayoutContract {
    target_lines: u8,
    contract_kind: StandardPcSearchContractKind,
    storage_kind: StandardPcBoardStorageKind,
    backend_kind: BoardBackendKind,
    algorithm: StandardPcGeometryAlgorithm,
    cpu_board_word_count: u8,
    gpu_board_word_count: u8,
    deleted_row_word_count: u8,
    operation_bitset_word_count: u8,
    maximum_placement_count: u8,
}

impl StandardPcStateLayoutContract {
    pub fn compile(target_lines: u8) -> Result<Self, StandardPcStateLayoutError> {
        let storage_kind = StandardPcBoardStorageKind::for_lines(target_lines)
            .map_err(StandardPcStateLayoutError::Board)?;
        let cell_count = u16::from(target_lines) * STANDARD_PC_BOARD_WIDTH;
        let maximum_placement_count = u8::try_from(cell_count / 4)
            .map_err(|_| StandardPcStateLayoutError::PlacementCountOverflow)?;
        let contract_kind = if target_lines <= STANDARD_PC_COMPACT_MAX_LINES {
            StandardPcSearchContractKind::CompactBoard64
        } else {
            StandardPcSearchContractKind::ExtendedBoardWords
        };
        let backend_kind = match storage_kind {
            StandardPcBoardStorageKind::Board64 => BoardBackendKind::Board64,
            StandardPcBoardStorageKind::Board128 => BoardBackendKind::Board128,
            StandardPcBoardStorageKind::Board256 => BoardBackendKind::Board256,
        };
        Ok(Self {
            target_lines,
            contract_kind,
            storage_kind,
            backend_kind,
            algorithm: StandardPcGeometryAlgorithm::InverseLockClearSkeletonExactCover,
            cpu_board_word_count: storage_kind.cpu_word_count(),
            gpu_board_word_count: u8::try_from(cell_count.div_ceil(u32::BITS as u16))
                .expect("a 24-line standard board needs at most eight GPU words"),
            deleted_row_word_count: u8::try_from(
                u16::from(target_lines).div_ceil(u32::BITS as u16),
            )
            .expect("a 24-line deleted-row mask needs one GPU word"),
            operation_bitset_word_count: u8::try_from(
                u16::from(maximum_placement_count).div_ceil(u64::BITS as u16),
            )
            .expect("a 24-line standard board needs one operation bitset word"),
            maximum_placement_count,
        })
    }

    pub const fn target_lines(self) -> u8 {
        self.target_lines
    }

    pub const fn contract_kind(self) -> StandardPcSearchContractKind {
        self.contract_kind
    }

    pub const fn storage_kind(self) -> StandardPcBoardStorageKind {
        self.storage_kind
    }

    pub const fn backend_kind(self) -> BoardBackendKind {
        self.backend_kind
    }

    pub const fn algorithm(self) -> StandardPcGeometryAlgorithm {
        self.algorithm
    }

    pub const fn cpu_board_word_count(self) -> u8 {
        self.cpu_board_word_count
    }

    pub const fn gpu_board_word_count(self) -> u8 {
        self.gpu_board_word_count
    }

    pub const fn deleted_row_word_count(self) -> u8 {
        self.deleted_row_word_count
    }

    pub const fn operation_bitset_word_count(self) -> u8 {
        self.operation_bitset_word_count
    }

    pub const fn maximum_placement_count(self) -> u8 {
        self.maximum_placement_count
    }

    pub const fn uses_legacy_board64_fast_path(self) -> bool {
        matches!(
            self.contract_kind,
            StandardPcSearchContractKind::CompactBoard64
        )
    }

    pub const fn runtime_capability(self) -> StandardPcRuntimeCapability {
        match self.contract_kind {
            StandardPcSearchContractKind::CompactBoard64 => {
                StandardPcRuntimeCapability::ConnectedExact
            }
            StandardPcSearchContractKind::ExtendedBoardWords => {
                StandardPcRuntimeCapability::Unsupported(
                    StandardPcRuntimeUnsupportedReason::ExtendedSearchStagesNotConnected,
                )
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardPcStateLayoutError {
    Board(StandardPcBoardError),
    PlacementCountOverflow,
}

pub const fn standard_pc_max_lines() -> u8 {
    STANDARD_PC_MAX_LINES
}
