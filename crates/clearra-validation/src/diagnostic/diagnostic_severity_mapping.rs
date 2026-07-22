use super::{diagnostic_code::DiagnosticCode, diagnostic_severity::DiagnosticSeverity};

impl DiagnosticCode {
    pub fn default_severity(self) -> DiagnosticSeverity {
        match self {
            Self::ESupplyInvalidDuplicate
            | Self::ESupplyInvalidBagBoundary
            | Self::EBoardUnsupportedMvp
            | Self::ECustomBoardUnsupportedMvp
            | Self::EBoardWidthOutOfScope
            | Self::EBoardBackendNotConnected
            | Self::EWideBoardRuntimeNotConnected
            | Self::EPieceSetUnsupportedMvp
            | Self::ECustomPieceUnsupportedMvp
            | Self::ECustomBagUnsupportedMvp
            | Self::ERuleUnsupportedMvp
            | Self::ECustomRuleInvalid
            | Self::EPcTargetUnsupportedMvp
            | Self::EPcQueryInvalid
            | Self::EBackendGpuFeatureDisabled
            | Self::EBackendGpuDeviceNotFound
            | Self::EBackendFrontierBudgetRequired
            | Self::EBackendGpuUnavailable
            | Self::EGpuWorkerTrustMismatch
            | Self::EGpuWorkerMemoryTicketMissing
            | Self::EGpuWorkerMissingMemoryTicket
            | Self::EGpuBufferFenceMissing
            | Self::EGpuFenceEpochMissing
            | Self::EGpuUnconfirmedProbabilitySource
            | Self::ECoreAbiVersionMismatch
            | Self::ENativeCoreUnavailable
            | Self::ECorePackingFailed
            | Self::ECoreBuildUpFailed
            | Self::ECoreMemoryContextDoubleRelease
            | Self::ECoreMemoryScopeInvalid
            | Self::ECoreMemoryLeakDetected
            | Self::ECoreFfiBufferBounds
            | Self::ECoreInvalidNativeView
            | Self::ECMemoryScopeInvalid
            | Self::ECMemoryLeakDetected
            | Self::EPackingCandidateUsedAsSolution
            | Self::EObjectiveUnsupportedMvp
            | Self::ESetupQueryInvalid
            | Self::EBuildQueryInvalid
            | Self::EScoreProfileInvalid
            | Self::ESpinTargetUnsupported
            | Self::ESpinProfileUnverified
            | Self::ESpinKickEvidenceMissing
            | Self::ESpinClassifierIncompatible
            | Self::EScoreProfileSpinPolicyIncompatible
            | Self::EScoreMatrixCapacityExceeded
            | Self::ESpinCoverageCapacityExceeded
            | Self::ESpinCoverageUniverseMismatch
            | Self::ECoverageCapacityExceeded
            | Self::EBuildUpVariantEnumerationTruncated
            | Self::EKickEvidenceBufferExhausted
            | Self::EAreaInfeasible
            | Self::EGuiFormInvalid
            | Self::EGuiFilePathUnsafe
            | Self::EGuiRenderUnsupported
            | Self::ERenderRuntimeSvgForbidden
            | Self::ERenderAssetProvenanceMissing
            | Self::EGuiSubprocessForbidden
            | Self::EFrontendTypedRequestRequired
            | Self::EExternalPcSourceRegistryInvalid
            | Self::EExternalPcSourceMissingRetrievedAt
            | Self::EExternalPcSourceRequiresHumanVerification => DiagnosticSeverity::Error,
            Self::WSupplyAmbiguousObservedWindow
            | Self::WFastPathTwoLineDisabled
            | Self::WMinimumCoverGreedyFallback
            | Self::WPcObjectiveFirstSolutionFallback
            | Self::WPcBackendFallback
            | Self::WBackendFallbackUsed
            | Self::WGpuBackendFallback
            | Self::WGpuDeviceUnavailable
            | Self::WHybridBackpressureActive
            | Self::WGpuResultCpuConfirmRequired
            | Self::WGpuBufferReleaseDeferred
            | Self::WPendingReleaseQueueNotDrained
            | Self::WMemoryPressureHigh
            | Self::WRuleSrsPlusExtensionsDisabled
            | Self::WObjectivePatternWeightModelNotMaterialized
            | Self::WSpinClassificationEstimated
            | Self::WSpinTargetProbabilityIncomplete
            | Self::WScoreExpectationSampleOnly
            | Self::WSpecialSpinDescriptorOnly
            | Self::WBuildUpEnumerationTruncated
            | Self::WObservedQueueProbabilityIncomplete
            | Self::WTraceRetentionTruncated
            | Self::WGuiBackendFallbackRequired
            | Self::WGuiSettingsLoadFailed
            | Self::WGuiSettingsSaveFailed => DiagnosticSeverity::Warning,
            Self::ISupplyBoundaryCompatible
            | Self::ISupplyFixedSequenceAccepted
            | Self::IFastPathTwoLineEnabled
            | Self::IBoardMvpSupported
            | Self::IPieceSetMvpSupported
            | Self::IRuleMvpSupported
            | Self::ICustomRuleVerified
            | Self::IPcTargetMvpSupported
            | Self::IPcQueryMvpSupported
            | Self::IObjectiveMvpSupported
            | Self::ISetupQueryMvpSupported
            | Self::IBuildQueryMvpSupported
            | Self::IScoreProfileMvp2Supported
            | Self::IAreaNecessaryConditionPassed => DiagnosticSeverity::Info,
        }
    }
}
