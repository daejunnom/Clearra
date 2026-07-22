use super::mvp3_capability_registry::{Mvp3CapabilityId, Mvp3CapabilityState};

impl Mvp3CapabilityId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CustomPieceSchema => "CustomPieceSchema",
            Self::MixedPieceSet => "MixedPieceSet",
            Self::CustomBagProfile => "CustomBagProfile",
            Self::CustomBoardWidth => "CustomBoardWidth",
            Self::Board128Runtime => "Board128Runtime",
            Self::WideBoardRuntime => "WideBoardRuntime",
            Self::GenericOperationTable => "GenericOperationTable",
            Self::GenericExactCover => "GenericExactCover",
            Self::DlxSolver => "DlxSolver",
            Self::AreaMultisetFeasibility => "AreaMultisetFeasibility",
            Self::CustomRuleEditor => "CustomRuleEditor",
            Self::GenericGpuDescriptor => "GenericGpuDescriptor",
            Self::GpuBuildUpExpansion => "GpuBuildUpExpansion",
            Self::CustomSkinEditor => "CustomSkinEditor",
        }
    }
}

impl Mvp3CapabilityState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "Unsupported",
            Self::ConnectedApproximate => "ConnectedApproximate",
            Self::ConnectedExact => "ConnectedExact",
        }
    }
}
