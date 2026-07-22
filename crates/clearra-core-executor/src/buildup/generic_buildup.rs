use clearra_core_ffi::{
    buildup_operation_set_runtime_status, buildup_runtime_status_for_board,
    problem::C_BUILDUP_STATUS_OK as C_BUILDUP_PROBLEM_STATUS_OK, CBoardDescriptor,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildUpCapability {
    ConnectedExact,
    Unsupported { reason: BuildUpUnsupportedReason },
}

impl BuildUpCapability {
    pub fn can_claim_solution(self) -> bool {
        matches!(self, Self::ConnectedExact)
    }

    pub fn unsupported_reason(self) -> Option<BuildUpUnsupportedReason> {
        match self {
            Self::ConnectedExact => None,
            Self::Unsupported { reason } => Some(reason),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildUpUnsupportedReason {
    BoardBackend,
    OperationCount,
}

pub fn buildup_capability_for_board(board: &CBoardDescriptor) -> BuildUpCapability {
    if buildup_runtime_status_for_board(board) == C_BUILDUP_PROBLEM_STATUS_OK {
        BuildUpCapability::ConnectedExact
    } else {
        BuildUpCapability::Unsupported {
            reason: BuildUpUnsupportedReason::BoardBackend,
        }
    }
}

pub fn buildup_capability_for_operation_count(operation_count: u32) -> BuildUpCapability {
    if buildup_operation_set_runtime_status(operation_count) == C_BUILDUP_PROBLEM_STATUS_OK {
        BuildUpCapability::ConnectedExact
    } else {
        BuildUpCapability::Unsupported {
            reason: BuildUpUnsupportedReason::OperationCount,
        }
    }
}

#[cfg(test)]
#[path = "generic_buildup_tests.rs"]
mod tests;
