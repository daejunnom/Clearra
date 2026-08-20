mod backend_constants {
    pub const C_BACKEND_AUTO: u32 = 0;
    pub const C_BACKEND_CPU: u32 = 5;
    pub const C_BACKEND_GPU: u32 = 6;
    pub const C_BACKEND_HYBRID: u32 = 7;
    pub const C_BACKEND_FALLBACK_ALLOW: u8 = 1;
    pub const C_BACKEND_FALLBACK_DENY: u8 = 2;
    pub const C_GPU_PIECE_SOURCE_UNKNOWN: u8 = 0;
    pub const C_GPU_PIECE_SOURCE_FIXED_SEQUENCE: u8 = 1;
    pub const C_GPU_PIECE_SOURCE_BAG_ALIGNED_PATTERN: u8 = 2;
    pub const C_GPU_PIECE_SOURCE_OBSERVED_WINDOW: u8 = 3;
}
mod backend_request {
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CBackendRequest {
        pub requested_backend: u32,
        pub workers: u16,
        pub deterministic: u8,
        pub reserved_flags: u8,
        pub fallback_policy: u8,
        pub gpu_device_kind: u8,
        pub gpu_device_index: u8,
        pub reserved: u8,
    }
}
mod bag_window {
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CBagWindow {
        pub start: u16,
        pub len: u16,
        pub boundary_known: u8,
        pub reserved: [u8; 3],
    }
}
mod board_descriptor {
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CBoardDescriptor {
        pub width: u16,
        pub visible_height: u16,
        pub search_height: u16,
        pub reserved: u16,
        pub initial_mask: u64,
        pub initial_mask_hi: u64,
        pub backend_kind: u32,
        pub cell_count: u32,
    }
}
mod buildup_operation {
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CBuildUpOperation {
        pub piece: u8,
        pub rotation: u8,
        pub x: i8,
        pub y: i8,
        pub operation_id: u16,
        pub required_deleted_row_mask: u16,
        pub mask: u64,
    }
}
mod buildup_operation_set {
    use super::{CBuildUpOperation, C_BUILDUP_MAX_OPERATIONS};

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CBuildUpOperationSet {
        pub operation_count: u16,
        pub geometry_variant_domains: u16,
        pub representative_order_hint: [u16; C_BUILDUP_MAX_OPERATIONS],
        pub reserved_tail: [u16; 3],
        pub operations: [CBuildUpOperation; C_BUILDUP_MAX_OPERATIONS],
    }

    impl Default for CBuildUpOperationSet {
        fn default() -> Self {
            Self {
                operation_count: 0,
                geometry_variant_domains: 0,
                representative_order_hint: [0; C_BUILDUP_MAX_OPERATIONS],
                reserved_tail: [0; 3],
                operations: [CBuildUpOperation::default(); C_BUILDUP_MAX_OPERATIONS],
            }
        }
    }
}
pub mod buildup_problem_builder;
mod checkpoint_spec {
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CCheckpointSpec {
        pub label_count: u16,
        pub checkpoint_count: u16,
        pub partition_count: u16,
        pub reserved: u16,
    }
}
pub mod dlx_build_up_bridge;
pub mod dlx_buildup_bridge;
pub mod ffi_problem_error;
pub mod generic_buildup;
mod hold_state {
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CHoldState {
        pub enabled: u8,
        pub empty: u8,
        pub piece: u8,
        pub reserved: u8,
    }
}
mod kick_offset_descriptor {
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CKickOffsetDescriptor {
        pub dx: i8,
        pub dy: i8,
    }
}
mod kick_sequence_descriptor {
    use super::{CKickOffsetDescriptor, C_RULE_MAX_KICK_OFFSETS};

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CKickSequenceDescriptor {
        pub offsets: [CKickOffsetDescriptor; C_RULE_MAX_KICK_OFFSETS],
        pub count: u8,
        pub reserved: [u8; 3],
    }

    impl Default for CKickSequenceDescriptor {
        fn default() -> Self {
            Self {
                offsets: [CKickOffsetDescriptor::default(); C_RULE_MAX_KICK_OFFSETS],
                count: 0,
                reserved: [0; 3],
            }
        }
    }
}
mod kick_transition_descriptor {
    use super::CKickSequenceDescriptor;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CKickTransitionDescriptor {
        pub piece: u8,
        pub from_rotation: u8,
        pub to_rotation: u8,
        pub reserved: u8,
        pub sequence: CKickSequenceDescriptor,
    }
}
mod packing_backend_descriptor_builder;
mod packing_board_descriptor_builder;
mod packing_budget_descriptor_builder;
mod packing_checkpoint_descriptor_builder;
mod packing_goal_descriptor_builder;
pub mod packing_problem_builder;
mod packing_problem_builder_error;
mod packing_rule_descriptor_builder;
mod packing_supply_descriptor_builder;
mod piece_constants {
    pub const C_PIECE_MULTISET_WINDOW_CAPACITY: usize = 15;
    pub const C_PIECE_NONE: u8 = 0;
    pub const C_PIECE_I: u8 = 1;
    pub const C_PIECE_O: u8 = 2;
    pub const C_PIECE_T: u8 = 3;
    pub const C_PIECE_S: u8 = 4;
    pub const C_PIECE_Z: u8 = 5;
    pub const C_PIECE_J: u8 = 6;
    pub const C_PIECE_L: u8 = 7;
    pub const C_PIECE_KIND_STORAGE_LEN: usize = C_PIECE_L as usize + 1;
    pub const C_PIECE_MULTISET_FAMILY_CAPACITY: usize = 256;
    pub const C_PIECE_SET_STANDARD_TETROMINOES: u32 = 1;
}
mod piece_multiset_window {
    use super::C_PIECE_KIND_STORAGE_LEN;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CPieceMultisetWindow {
        pub counts: [u8; C_PIECE_KIND_STORAGE_LEN],
        pub total_count: u8,
        pub exact_count: u8,
        pub reserved: [u8; 6],
    }

    impl Default for CPieceMultisetWindow {
        fn default() -> Self {
            Self {
                counts: [0; C_PIECE_KIND_STORAGE_LEN],
                total_count: 0,
                exact_count: 0,
                reserved: [0; 6],
            }
        }
    }
}
mod piece_multiset_family {
    use super::{CPieceMultisetWindow, C_PIECE_MULTISET_FAMILY_CAPACITY};

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CPieceMultisetFamily {
        pub members: [CPieceMultisetWindow; C_PIECE_MULTISET_FAMILY_CAPACITY],
        pub count: u16,
        pub complete: u8,
        pub reserved: [u8; 5],
    }

    impl Default for CPieceMultisetFamily {
        fn default() -> Self {
            Self {
                members: [CPieceMultisetWindow::default(); C_PIECE_MULTISET_FAMILY_CAPACITY],
                count: 0,
                complete: 0,
                reserved: [0; 5],
            }
        }
    }
}
mod piece_window_descriptor {
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CPieceWindowDescriptor {
        pub max_pieces: u16,
        pub exact_pieces: u16,
        pub has_exact_pieces: u8,
        pub reserved: [u8; 3],
    }
}
mod problem_budget {
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CProblemBudget {
        pub max_nodes: u64,
        pub max_seconds: u32,
        pub max_results: u32,
        pub max_patterns: u32,
        pub max_frontier_states: u32,
        pub max_memory_mib: u32,
        pub has_max_memory_mib: u8,
        pub reserved: [u8; 7],
    }
}
mod problem_descriptors;
mod queue_view {
    use super::C_QUEUE_VIEW_CAPACITY;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CQueueView {
        pub mode: u8,
        pub truncated: u8,
        pub len: u16,
        pub stored_len: u16,
        pub reserved: u16,
        pub provenance_id: u32,
        pub pieces: [u8; C_QUEUE_VIEW_CAPACITY],
    }

    impl Default for CQueueView {
        fn default() -> Self {
            Self {
                mode: 0,
                truncated: 0,
                len: 0,
                stored_len: 0,
                reserved: 0,
                provenance_id: 0,
                pieces: [0; C_QUEUE_VIEW_CAPACITY],
            }
        }
    }
}
mod rule_constants {
    pub const C_RULE_SRS_PLUS: u32 = 1;
    pub const C_RULE_SRS: u32 = 2;
    pub const C_RULE_SRS_X: u32 = 3;
    pub const C_RULE_ASC: u32 = 4;
    pub const C_RULE_ARS: u32 = 5;
    pub const C_RULE_NO_KICK: u32 = 6;
    pub const C_RULE_JSTRIS_180: u32 = 7;
    pub const C_RULE_CUSTOM: u32 = 255;
    pub const C_KICK_SRS_90: u32 = 1;
    pub const C_KICK_NO_KICK: u32 = 2;
    pub const C_KICK_SRS_PLUS_180: u32 = 3;
    pub const C_KICK_SRS_X: u32 = 4;
    pub const C_KICK_ASC: u32 = 5;
    pub const C_KICK_ARS: u32 = 6;
    pub const C_KICK_IMPORTED: u32 = 7;
    pub const C_KICK_JSTRIS_180: u32 = 8;
    pub const C_KICK_CUSTOM: u32 = 255;
    pub const C_SPAWN_STANDARD_10: u32 = 1;
    pub const C_SPAWN_ARIKA: u32 = 2;
    pub const C_SPAWN_CUSTOM: u32 = 255;
    pub const C_RULE_MAX_KICK_OFFSETS: usize = 6;
    pub const C_RULE_MAX_KICK_TRANSITIONS: usize = 84;
}
mod rule_profile_descriptor {
    use super::{CKickTransitionDescriptor, C_RULE_MAX_KICK_TRANSITIONS};

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CRuleProfileDescriptor {
        pub piece_set_profile_id: u32,
        pub bag_profile_id: u32,
        pub rule_profile_id: u32,
        pub kick_profile_id: u32,
        pub spawn_profile_id: u32,
        pub has_verified_kick_profile: u8,
        pub verified_supports_180: u8,
        pub verified_transition_count: u16,
        pub verified_transitions: [CKickTransitionDescriptor; C_RULE_MAX_KICK_TRANSITIONS],
    }

    impl Default for CRuleProfileDescriptor {
        fn default() -> Self {
            Self {
                piece_set_profile_id: 0,
                bag_profile_id: 0,
                rule_profile_id: 0,
                kick_profile_id: 0,
                spawn_profile_id: 0,
                has_verified_kick_profile: 0,
                verified_supports_180: 0,
                verified_transition_count: 0,
                verified_transitions: [CKickTransitionDescriptor::default();
                    C_RULE_MAX_KICK_TRANSITIONS],
            }
        }
    }
}
mod search_constants {
    pub const C_GOAL_CLEAR_TO_EMPTY: u32 = 1;
    pub const C_COUNT_FIRST_SOLUTION: u32 = 1;
    pub const C_COUNT_ALL: u32 = 2;
    pub const C_COUNT_UNIQUE: u32 = 3;
    pub const C_OBJECTIVE_ALL: u32 = 1;
    pub const C_OBJECTIVE_UNIQUE: u32 = 2;
    pub const C_OBJECTIVE_MIN_COVER: u32 = 3;
    pub const C_BUILDUP_MAX_OPERATIONS: usize = 15;
    pub const C_BUILDUP_FLAG_HOLD_ENABLED: u32 = 1;
    pub const C_BUILDUP_SOURCE_CONCRETE_PATTERN: u32 = 1;
    pub const C_BUILDUP_SOURCE_STANDARD_BAG_AUTOMATON: u32 = 2;
    pub const C_BUILDUP_TERMINAL_PROJECTION_POLICY_VERSION: u16 = 1;
    pub const C_BUILDUP_TERMINAL_PROJECTION_DISABLED: u8 = 0;
    pub const C_BUILDUP_TERMINAL_PROJECTION_RELEASE_FINITE_HELD: u8 = 1;
    pub const C_LINE_CLEAR_POLICY_STANDARD: u32 = 1;
    pub const C_PACKING_MAX_PIECES: usize = 15;
}
mod supply_constants {
    pub const C_QUEUE_VIEW_CAPACITY: usize = 64;
    pub const C_PIECE_SOURCE_PATTERN_READER_CAPACITY: usize = 64;
    pub const C_QUEUE_FIXED_SEQUENCE: u8 = 1;
    pub const C_QUEUE_BAG_ALIGNED_PATTERN: u8 = 2;
    pub const C_QUEUE_OBSERVED: u8 = 3;
    pub const C_SUPPLY_PROVENANCE_FIXED_SEQUENCE: u32 = 4097;
    pub const C_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN: u32 = 8193;
    pub const C_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED: u32 = 12289;
    pub const C_SUPPLY_PROFILE_UNSUPPORTED: u32 = 0;
    pub const C_SUPPLY_PROFILE_STANDARD_7_BAG: u32 = 1;
    pub const C_SUPPLY_PROFILE_FIXED_SEQUENCE: u32 = 2;
    pub const C_SUPPLY_PROFILE_OBSERVED_STANDARD_7_BAG: u32 = 3;
    pub const C_SUPPLY_BOUNDARY_NOT_EVALUATED: u8 = 0;
    pub const C_SUPPLY_BOUNDARY_FIXED: u8 = 1;
    pub const C_SUPPLY_BOUNDARY_OBSERVED_COMPATIBLE: u8 = 2;
    pub const C_SUPPLY_BOUNDARY_OBSERVED_AMBIGUOUS: u8 = 3;
    pub const C_SUPPLY_BOUNDARY_DUPLICATE_REJECTED: u8 = 4;
    pub const C_BAG_STANDARD_7_BAG: u32 = 1;
}
mod supply_identity_descriptor {
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CSupplyIdentityDescriptor {
        pub supply_provenance_id: u32,
        pub bag_profile_id: u32,
        pub piece_set_id: u32,
        pub observed_window_id: u32,
        pub bag_boundary_evidence: u8,
        pub duplicate_witness: u8,
        pub ambiguity_report: u8,
        pub reserved: u8,
    }
}

pub use backend_constants::*;
pub use backend_request::CBackendRequest;
pub use bag_window::CBagWindow;
pub use board_descriptor::CBoardDescriptor;
pub use buildup_operation::CBuildUpOperation;
pub use buildup_operation_set::CBuildUpOperationSet;
pub use buildup_problem_builder::{CBuildUpProblemBuilder, CBuildUpProblemTemplate};
pub use checkpoint_spec::CCheckpointSpec;
pub use dlx_buildup_bridge::{
    DlxBuildUpBridge, DlxBuildUpBridgeError, DlxBuildUpOperationCandidate,
};
pub use ffi_problem_error::FfiProblemError;
pub use generic_buildup::{
    buildup_operation_set_runtime_status, buildup_runtime_status_for_board, C_BUILDUP_STATUS_OK,
    C_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE,
};
pub use hold_state::CHoldState;
pub use kick_offset_descriptor::CKickOffsetDescriptor;
pub use kick_sequence_descriptor::CKickSequenceDescriptor;
pub use kick_transition_descriptor::CKickTransitionDescriptor;
pub use packing_problem_builder::CPackingProblemBuilder;
pub use piece_constants::*;
pub use piece_multiset_family::CPieceMultisetFamily;
pub use piece_multiset_window::CPieceMultisetWindow;
pub use piece_window_descriptor::CPieceWindowDescriptor;
pub use problem_budget::CProblemBudget;
pub use problem_descriptors::{CBuildUpProblem, CPackingProblem};
pub use queue_view::CQueueView;
pub use rule_constants::*;
pub use rule_profile_descriptor::CRuleProfileDescriptor;
pub use search_constants::*;
pub use supply_constants::*;
pub use supply_identity_descriptor::CSupplyIdentityDescriptor;
