#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mvp2CapabilityId {
    RuleKickExpansion,
    ScoringPostProcessing,
    SpinTarget,
    SetupRawMetricsV2,
    BuildEditorSchema,
    RendererPng,
    RendererGif,
    GpuPackingStrengthening,
    HybridScheduler,
}

impl Mvp2CapabilityId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RuleKickExpansion => "RuleKickExpansion",
            Self::ScoringPostProcessing => "ScoringPostProcessing",
            Self::SpinTarget => "SpinTarget",
            Self::SetupRawMetricsV2 => "SetupRawMetricsV2",
            Self::BuildEditorSchema => "BuildEditorSchema",
            Self::RendererPng => "RendererPng",
            Self::RendererGif => "RendererGif",
            Self::GpuPackingStrengthening => "GpuPackingStrengthening",
            Self::HybridScheduler => "HybridScheduler",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mvp2CapabilityState {
    Unsupported,
    ConnectedApproximate,
    ConnectedExact,
}

impl Mvp2CapabilityState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "Unsupported",
            Self::ConnectedApproximate => "ConnectedApproximate",
            Self::ConnectedExact => "ConnectedExact",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mvp2Capability {
    id: Mvp2CapabilityId,
    state: Mvp2CapabilityState,
    disabled_reason: Option<&'static str>,
    exact_transition_condition: &'static str,
}

impl Mvp2Capability {
    pub fn new(
        id: Mvp2CapabilityId,
        state: Mvp2CapabilityState,
        disabled_reason: Option<&'static str>,
        exact_transition_condition: &'static str,
    ) -> Self {
        Self {
            id,
            state,
            disabled_reason,
            exact_transition_condition,
        }
    }
}
impl Mvp2Capability {
    pub const fn id(&self) -> Mvp2CapabilityId {
        self.id
    }
}
impl Mvp2Capability {
    pub const fn state(&self) -> Mvp2CapabilityState {
        self.state
    }
}
impl Mvp2Capability {
    pub const fn disabled_reason(&self) -> Option<&'static str> {
        self.disabled_reason
    }
}
impl Mvp2Capability {
    pub const fn exact_transition_condition(&self) -> &'static str {
        self.exact_transition_condition
    }
}
impl Mvp2Capability {
    pub const fn is_exact(&self) -> bool {
        matches!(self.state, Mvp2CapabilityState::ConnectedExact)
    }
}
impl Mvp2Capability {
    pub const fn state_name(&self) -> &'static str {
        self.state.as_str()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mvp2CapabilityReport {
    capabilities: Vec<Mvp2Capability>,
}

impl Mvp2CapabilityReport {
    pub fn current() -> Self {
        Self {
            capabilities: vec![
                Mvp2Capability::new(
                    Mvp2CapabilityId::RuleKickExpansion,
                    Mvp2CapabilityState::Unsupported,
                    Some("rule_kick_expansion_not_connected"),
                    "verified imported kick profiles and C compact descriptors pass exact fixtures",
                ),
                Mvp2Capability::new(
                    Mvp2CapabilityId::ScoringPostProcessing,
                    Mvp2CapabilityState::ConnectedApproximate,
                    None,
                    "profile-specific score/attack rules and exact spin evidence are implemented",
                ),
                Mvp2Capability::new(
                    Mvp2CapabilityId::SpinTarget,
                    Mvp2CapabilityState::Unsupported,
                    Some("spin_target_not_connected"),
                    "classifier capability and trace-complete KickEvidence are available",
                ),
                Mvp2Capability::new(
                    Mvp2CapabilityId::SetupRawMetricsV2,
                    Mvp2CapabilityState::Unsupported,
                    Some("setup_raw_metrics_v2_not_connected"),
                    "raw metrics are backed by C BuildUp/Coverage rows and stable exports",
                ),
                Mvp2Capability::new(
                    Mvp2CapabilityId::BuildEditorSchema,
                    Mvp2CapabilityState::Unsupported,
                    Some("build_editor_not_connected"),
                    "schema-driven editor emits validated BuildTemplate assignments",
                ),
                Mvp2Capability::new(
                    Mvp2CapabilityId::RendererPng,
                    Mvp2CapabilityState::ConnectedExact,
                    None,
                    "exact PNG renderer is connected and render_exact=true is proven",
                ),
                Mvp2Capability::new(
                    Mvp2CapabilityId::RendererGif,
                    Mvp2CapabilityState::ConnectedExact,
                    None,
                    "exact GIF renderer is connected and frame semantics are proven",
                ),
                Mvp2Capability::new(
                    Mvp2CapabilityId::GpuPackingStrengthening,
                    Mvp2CapabilityState::Unsupported,
                    Some("gpu_packing_not_connected"),
                    "GPU candidates pass CPU exact confirmation and product parity fixtures",
                ),
                Mvp2Capability::new(
                    Mvp2CapabilityId::HybridScheduler,
                    Mvp2CapabilityState::Unsupported,
                    Some("hybrid_scheduler_not_connected"),
                    "hybrid result equals CPU reference with clean memory/backpressure reports",
                ),
            ],
        }
    }
}
impl Mvp2CapabilityReport {
    pub fn capabilities(&self) -> &[Mvp2Capability] {
        &self.capabilities
    }
}
impl Mvp2CapabilityReport {
    pub fn find(&self, id: Mvp2CapabilityId) -> Option<&Mvp2Capability> {
        self.capabilities
            .iter()
            .find(|capability| capability.id == id)
    }
}
impl Mvp2CapabilityReport {
    pub fn assert_exact_claim_allowed(
        &self,
        id: Mvp2CapabilityId,
    ) -> Result<(), Mvp2CapabilityError> {
        let Some(capability) = self.find(id) else {
            return Err(Mvp2CapabilityError::UnsupportedFeature);
        };
        if capability.is_exact() {
            Ok(())
        } else {
            Err(Mvp2CapabilityError::ExactClaimRequiresCapabilityExact)
        }
    }
}
impl Mvp2CapabilityReport {
    pub fn mvp2_capability_report_lists_all_mvp2_features(&self) -> bool {
        const REQUIRED: [Mvp2CapabilityId; 9] = [
            Mvp2CapabilityId::RuleKickExpansion,
            Mvp2CapabilityId::ScoringPostProcessing,
            Mvp2CapabilityId::SpinTarget,
            Mvp2CapabilityId::SetupRawMetricsV2,
            Mvp2CapabilityId::BuildEditorSchema,
            Mvp2CapabilityId::RendererPng,
            Mvp2CapabilityId::RendererGif,
            Mvp2CapabilityId::GpuPackingStrengthening,
            Mvp2CapabilityId::HybridScheduler,
        ];

        self.capabilities.len() == REQUIRED.len()
            && REQUIRED
                .iter()
                .all(|required_id| self.find(*required_id).is_some())
    }
}
impl Mvp2CapabilityReport {
    pub fn mvp2_exact_claims_require_capability_exact(&self) -> bool {
        self.capabilities.iter().all(|capability| {
            let result = self.assert_exact_claim_allowed(capability.id());
            if capability.is_exact() {
                result.is_ok()
            } else {
                result == Err(Mvp2CapabilityError::ExactClaimRequiresCapabilityExact)
            }
        })
    }
}
impl Mvp2CapabilityReport {
    pub fn mvp2_unsupported_features_emit_disabled_reason(&self) -> bool {
        self.capabilities.iter().all(|capability| {
            capability.state != Mvp2CapabilityState::Unsupported
                || capability.disabled_reason.is_some()
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mvp2CapabilityError {
    UnsupportedFeature,
    ExactClaimRequiresCapabilityExact,
}
