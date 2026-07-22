#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mvp3CapabilityId {
    CustomPieceSchema,
    MixedPieceSet,
    CustomBagProfile,
    CustomBoardWidth,
    Board128Runtime,
    WideBoardRuntime,
    GenericOperationTable,
    GenericExactCover,
    DlxSolver,
    AreaMultisetFeasibility,
    CustomRuleEditor,
    GenericGpuDescriptor,
    GpuBuildUpExpansion,
    CustomSkinEditor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mvp3CapabilityState {
    Unsupported,
    ConnectedApproximate,
    ConnectedExact,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mvp3Capability {
    id: Mvp3CapabilityId,
    state: Mvp3CapabilityState,
    disabled_reason: Option<&'static str>,
    runtime_transition_condition: &'static str,
    standard_fast_path_impact: &'static str,
}

impl Mvp3Capability {
    fn new(
        id: Mvp3CapabilityId,
        state: Mvp3CapabilityState,
        disabled_reason: Option<&'static str>,
        runtime_transition_condition: &'static str,
    ) -> Self {
        Self {
            id,
            state,
            disabled_reason,
            runtime_transition_condition,
            standard_fast_path_impact: "none",
        }
    }
}
impl Mvp3Capability {
    pub const fn id(&self) -> Mvp3CapabilityId {
        self.id
    }
}
impl Mvp3Capability {
    pub const fn state(&self) -> Mvp3CapabilityState {
        self.state
    }
}
impl Mvp3Capability {
    pub const fn disabled_reason(&self) -> Option<&'static str> {
        self.disabled_reason
    }
}
impl Mvp3Capability {
    pub const fn runtime_transition_condition(&self) -> &'static str {
        self.runtime_transition_condition
    }
}
impl Mvp3Capability {
    pub const fn standard_fast_path_impact(&self) -> &'static str {
        self.standard_fast_path_impact
    }
}
impl Mvp3Capability {
    pub const fn runtime_execution_allowed(&self) -> bool {
        matches!(
            self.state,
            Mvp3CapabilityState::ConnectedApproximate | Mvp3CapabilityState::ConnectedExact
        )
    }
}
impl Mvp3Capability {
    pub const fn is_exact_supported(&self) -> bool {
        matches!(self.state, Mvp3CapabilityState::ConnectedExact)
    }
}
impl Mvp3Capability {
    pub const fn state_name(&self) -> &'static str {
        self.state.as_str()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mvp3CapabilityReport {
    capabilities: Vec<Mvp3Capability>,
}

impl Mvp3CapabilityReport {
    pub fn current() -> Self {
        Self {
            capabilities: vec![
                Mvp3Capability::new(
                    Mvp3CapabilityId::CustomPieceSchema,
                    Mvp3CapabilityState::Unsupported,
                    Some("custom_piece_unsupported"),
                    "generic piece registry validates and maps to a separate generic runtime",
                ),
                Mvp3Capability::new(
                    Mvp3CapabilityId::MixedPieceSet,
                    Mvp3CapabilityState::Unsupported,
                    Some("mixed_piece_set_runtime_not_connected"),
                    "mixed piece sets compile to generic operation tables without standard fallback",
                ),
                Mvp3Capability::new(
                    Mvp3CapabilityId::CustomBagProfile,
                    Mvp3CapabilityState::Unsupported,
                    Some("custom_bag_profile_runtime_not_connected"),
                    "custom bag profile has typed supply provenance and generic cache identity",
                ),
                Mvp3Capability::new(
                    Mvp3CapabilityId::CustomBoardWidth,
                    Mvp3CapabilityState::Unsupported,
                    Some("custom_board_width_runtime_not_connected"),
                    "custom width board descriptors route to generic board runtime only",
                ),
                Mvp3Capability::new(
                    Mvp3CapabilityId::Board128Runtime,
                    Mvp3CapabilityState::Unsupported,
                    Some("board128_runtime_guarded"),
                    "Board128 runtime passes parity fixtures and does not alter Board64 fast path",
                ),
                Mvp3Capability::new(
                    Mvp3CapabilityId::WideBoardRuntime,
                    Mvp3CapabilityState::Unsupported,
                    Some("wide_board_runtime_guarded"),
                    "wide board runtime has separate descriptor, cache key, and fixtures",
                ),
                Mvp3Capability::new(
                    Mvp3CapabilityId::GenericOperationTable,
                    Mvp3CapabilityState::Unsupported,
                    Some("generic_operation_table_unsupported"),
                    "generic operation table builder exists outside standard tetromino operation table",
                ),
                Mvp3Capability::new(
                    Mvp3CapabilityId::GenericExactCover,
                    Mvp3CapabilityState::Unsupported,
                    Some("generic_exact_cover_runtime_not_connected"),
                    "generic exact-cover inputs validate and route to a separate generic solver",
                ),
                Mvp3Capability::new(
                    Mvp3CapabilityId::DlxSolver,
                    Mvp3CapabilityState::Unsupported,
                    Some("dlx_solver_not_connected"),
                    "DLX solver has typed input, exact fixtures, and no standard fast-path fallback",
                ),
                Mvp3Capability::new(
                    Mvp3CapabilityId::AreaMultisetFeasibility,
                    Mvp3CapabilityState::Unsupported,
                    Some("area_multiset_feasibility_guarded"),
                    "non-tetromino area multiset search is proven before execution",
                ),
                Mvp3Capability::new(
                    Mvp3CapabilityId::CustomRuleEditor,
                    Mvp3CapabilityState::Unsupported,
                    Some("custom_rule_editor_unsupported"),
                    "custom rule editor output imports as verified profiles before runtime execution",
                ),
                Mvp3Capability::new(
                    Mvp3CapabilityId::GenericGpuDescriptor,
                    Mvp3CapabilityState::Unsupported,
                    Some("generic_gpu_descriptor_not_connected"),
                    "generic GPU descriptor preserves piece, board, area, and rule identities",
                ),
                Mvp3Capability::new(
                    Mvp3CapabilityId::GpuBuildUpExpansion,
                    Mvp3CapabilityState::Unsupported,
                    Some("gpu_buildup_expansion_forbidden_until_exact_confirmed"),
                    "GPU BuildUp expansion has CPU exact confirmation and product parity fixtures",
                ),
                Mvp3Capability::new(
                    Mvp3CapabilityId::CustomSkinEditor,
                    Mvp3CapabilityState::Unsupported,
                    Some("custom_skin_editor_unsupported"),
                    "custom skin import writes sanitized atlas, manifest, and provenance",
                ),
            ],
        }
    }
}
impl Mvp3CapabilityReport {
    pub fn capabilities(&self) -> &[Mvp3Capability] {
        &self.capabilities
    }
}
impl Mvp3CapabilityReport {
    pub fn find(&self, id: Mvp3CapabilityId) -> Option<&Mvp3Capability> {
        self.capabilities
            .iter()
            .find(|capability| capability.id == id)
    }
}
impl Mvp3CapabilityReport {
    pub fn assert_runtime_execution_allowed(
        &self,
        id: Mvp3CapabilityId,
    ) -> Result<(), Mvp3CapabilityError> {
        let Some(capability) = self.find(id) else {
            return Err(Mvp3CapabilityError::UnsupportedFeature);
        };
        if capability.runtime_execution_allowed() {
            Ok(())
        } else {
            Err(Mvp3CapabilityError::RuntimeExecutionRequiresRuntimeConnected)
        }
    }
}
impl Mvp3CapabilityReport {
    pub fn assert_exact_claim_allowed(
        &self,
        id: Mvp3CapabilityId,
    ) -> Result<(), Mvp3CapabilityError> {
        let Some(capability) = self.find(id) else {
            return Err(Mvp3CapabilityError::UnsupportedFeature);
        };
        if capability.is_exact_supported() {
            Ok(())
        } else {
            Err(Mvp3CapabilityError::ExactClaimRequiresExactSupported)
        }
    }
}
impl Mvp3CapabilityReport {
    pub fn mvp3_capability_report_lists_all_generalization_features(&self) -> bool {
        const REQUIRED: [Mvp3CapabilityId; 14] = [
            Mvp3CapabilityId::CustomPieceSchema,
            Mvp3CapabilityId::MixedPieceSet,
            Mvp3CapabilityId::CustomBagProfile,
            Mvp3CapabilityId::CustomBoardWidth,
            Mvp3CapabilityId::Board128Runtime,
            Mvp3CapabilityId::WideBoardRuntime,
            Mvp3CapabilityId::GenericOperationTable,
            Mvp3CapabilityId::GenericExactCover,
            Mvp3CapabilityId::DlxSolver,
            Mvp3CapabilityId::AreaMultisetFeasibility,
            Mvp3CapabilityId::CustomRuleEditor,
            Mvp3CapabilityId::GenericGpuDescriptor,
            Mvp3CapabilityId::GpuBuildUpExpansion,
            Mvp3CapabilityId::CustomSkinEditor,
        ];

        self.capabilities.len() == REQUIRED.len()
            && REQUIRED
                .iter()
                .all(|required_id| self.find(*required_id).is_some())
    }
}
impl Mvp3CapabilityReport {
    pub fn unsupported_features_do_not_execute_runtime(&self) -> bool {
        self.capabilities
            .iter()
            .filter(|capability| capability.state == Mvp3CapabilityState::Unsupported)
            .all(|capability| !capability.runtime_execution_allowed())
    }
}
impl Mvp3CapabilityReport {
    pub fn unsupported_features_emit_disabled_reason(&self) -> bool {
        self.capabilities
            .iter()
            .filter(|capability| capability.state == Mvp3CapabilityState::Unsupported)
            .all(|capability| capability.disabled_reason.is_some())
    }
}
impl Mvp3CapabilityReport {
    pub fn standard_fast_path_unchanged(&self) -> bool {
        self.capabilities
            .iter()
            .all(|capability| capability.standard_fast_path_impact == "none")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mvp3CapabilityError {
    UnsupportedFeature,
    RuntimeExecutionRequiresRuntimeConnected,
    ExactClaimRequiresExactSupported,
}
