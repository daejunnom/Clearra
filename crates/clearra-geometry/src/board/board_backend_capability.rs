use clearra_core_domain::board::board_size::BoardSize;

use crate::layout::board_backend::{backend_kind_for_size, BoardBackendKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardRuntimeUnsupportedReason {
    None,
    BoardWidthOutOfScope,
    BoardBackendNotConnected,
    WideBoardRuntimeNotConnected,
}

impl BoardRuntimeUnsupportedReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::BoardWidthOutOfScope => "board_width_out_of_scope",
            Self::BoardBackendNotConnected => "board_backend_not_connected",
            Self::WideBoardRuntimeNotConnected => "wide_board_runtime_not_connected",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardBackendCapability {
    backend_kind: BoardBackendKind,
    descriptor_supported: bool,
    basic_ops_supported: bool,
    operation_mask_supported: bool,
    runtime_connected: bool,
    packing_supported: bool,
    unsupported_reason: BoardRuntimeUnsupportedReason,
}

impl BoardBackendCapability {
    pub const fn new(
        backend_kind: BoardBackendKind,
        descriptor_supported: bool,
        basic_ops_supported: bool,
        operation_mask_supported: bool,
        runtime_connected: bool,
        packing_supported: bool,
        unsupported_reason: BoardRuntimeUnsupportedReason,
    ) -> Self {
        Self {
            backend_kind,
            descriptor_supported,
            basic_ops_supported,
            operation_mask_supported,
            runtime_connected,
            packing_supported,
            unsupported_reason,
        }
    }
}
impl BoardBackendCapability {
    pub fn backend_kind(self) -> BoardBackendKind {
        self.backend_kind
    }
}
impl BoardBackendCapability {
    pub fn descriptor_supported(self) -> bool {
        self.descriptor_supported
    }
}
impl BoardBackendCapability {
    pub fn basic_ops_supported(self) -> bool {
        self.basic_ops_supported
    }
}
impl BoardBackendCapability {
    pub fn operation_mask_supported(self) -> bool {
        self.operation_mask_supported
    }
}
impl BoardBackendCapability {
    pub fn runtime_connected(self) -> bool {
        self.runtime_connected
    }
}
impl BoardBackendCapability {
    pub fn packing_supported(self) -> bool {
        self.packing_supported
    }
}
impl BoardBackendCapability {
    pub fn unsupported_reason(self) -> BoardRuntimeUnsupportedReason {
        self.unsupported_reason
    }
}

pub fn board_backend_capability_for_kind(kind: BoardBackendKind) -> BoardBackendCapability {
    match kind {
        BoardBackendKind::Board64 => BoardBackendCapability::new(
            kind,
            true,
            true,
            true,
            true,
            true,
            BoardRuntimeUnsupportedReason::None,
        ),
        BoardBackendKind::Board128 => BoardBackendCapability::new(
            kind,
            true,
            true,
            true,
            false,
            false,
            BoardRuntimeUnsupportedReason::BoardBackendNotConnected,
        ),
        BoardBackendKind::Board256 => BoardBackendCapability::new(
            kind,
            true,
            true,
            true,
            false,
            false,
            BoardRuntimeUnsupportedReason::BoardBackendNotConnected,
        ),
        BoardBackendKind::Wide => BoardBackendCapability::new(
            kind,
            true,
            false,
            false,
            false,
            false,
            BoardRuntimeUnsupportedReason::WideBoardRuntimeNotConnected,
        ),
    }
}

pub fn board_backend_capability_for_size(size: BoardSize) -> BoardBackendCapability {
    board_backend_capability_for_kind(backend_kind_for_size(size))
}

#[cfg(test)]
#[path = "board_backend_capability_tests.rs"]
mod tests;
