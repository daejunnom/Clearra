use super::{diagnostic_code::DiagnosticCode, diagnostic_severity::DiagnosticSeverity};

mod case_backend_codes_are_stable {
    use super::*;

    #[test]
    fn backend_codes_are_stable() {
        assert_eq!(
            DiagnosticCode::EBackendGpuFeatureDisabled.as_str(),
            "E_BACKEND_GPU_FEATURE_DISABLED"
        );
        for code in [
            DiagnosticCode::EBackendGpuDeviceNotFound,
            DiagnosticCode::EBackendFrontierBudgetRequired,
        ] {
            assert_eq!(code.default_severity(), DiagnosticSeverity::Error);
        }
        assert_eq!(
            DiagnosticCode::EBackendGpuUnavailable.as_str(),
            "E_BACKEND_GPU_UNAVAILABLE"
        );
        for code in [
            DiagnosticCode::WBackendFallbackUsed,
            DiagnosticCode::WGpuResultCpuConfirmRequired,
        ] {
            assert_eq!(code.default_severity(), DiagnosticSeverity::Warning);
        }
    }
}
mod case_board_and_custom_codes_are_stable {
    use super::*;

    #[test]
    fn board_and_custom_codes_are_stable() {
        for code in [
            DiagnosticCode::EBoardUnsupportedMvp,
            DiagnosticCode::ECustomBoardUnsupportedMvp,
            DiagnosticCode::EBoardBackendNotConnected,
            DiagnosticCode::ECustomPieceUnsupportedMvp,
            DiagnosticCode::ECustomBagUnsupportedMvp,
        ] {
            assert_eq!(code.default_severity(), DiagnosticSeverity::Error);
        }
        assert_eq!(
            DiagnosticCode::EBoardWidthOutOfScope.as_str(),
            "E_BOARD_WIDTH_OUT_OF_SCOPE"
        );
        assert_eq!(
            DiagnosticCode::EWideBoardRuntimeNotConnected.as_str(),
            "E_WIDE_BOARD_RUNTIME_NOT_CONNECTED"
        );
        assert_eq!(
            DiagnosticCode::IBuildQueryMvpSupported.default_severity(),
            DiagnosticSeverity::Info
        );
    }
}
mod case_core_memory_and_gpu_codes_are_stable {
    use super::*;

    #[test]
    fn core_memory_and_gpu_codes_are_stable() {
        assert_eq!(
            DiagnosticCode::ECoreAbiVersionMismatch.as_str(),
            "E_CORE_ABI_VERSION_MISMATCH"
        );
        assert_eq!(
            DiagnosticCode::ECorePackingFailed.default_severity(),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            DiagnosticCode::ECoreBuildUpFailed.as_str(),
            "E_CORE_BUILDUP_FAILED"
        );
        assert_eq!(
            DiagnosticCode::ECoreMemoryContextDoubleRelease.as_str(),
            "E_CORE_MEMORY_CONTEXT_DOUBLE_RELEASE"
        );
        assert_eq!(
            DiagnosticCode::ECoreMemoryScopeInvalid.as_str(),
            "E_CORE_MEMORY_SCOPE_INVALID"
        );
        assert_eq!(
            DiagnosticCode::ECoreMemoryLeakDetected.default_severity(),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            DiagnosticCode::ECoreFfiBufferBounds.as_str(),
            "E_CORE_FFI_BUFFER_BOUNDS"
        );
        for code in [
            DiagnosticCode::ECoreInvalidNativeView,
            DiagnosticCode::ECMemoryScopeInvalid,
            DiagnosticCode::EPackingCandidateUsedAsSolution,
            DiagnosticCode::EGpuUnconfirmedProbabilitySource,
        ] {
            assert_eq!(code.default_severity(), DiagnosticSeverity::Error);
        }
        assert_eq!(
            DiagnosticCode::ECMemoryLeakDetected.as_str(),
            "E_C_MEMORY_LEAK_DETECTED"
        );
        assert_eq!(
            DiagnosticCode::EGpuWorkerMissingMemoryTicket.as_str(),
            "E_GPU_WORKER_MISSING_MEMORY_TICKET"
        );
        assert_eq!(
            DiagnosticCode::EGpuFenceEpochMissing.as_str(),
            "E_GPU_FENCE_EPOCH_MISSING"
        );
    }
}
mod case_external_pc_codes_are_stable {
    use super::*;

    #[test]
    fn external_pc_codes_are_stable() {
        assert_eq!(
            DiagnosticCode::EExternalPcSourceRegistryInvalid.as_str(),
            "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID"
        );
        assert_eq!(
            DiagnosticCode::EExternalPcSourceMissingRetrievedAt.as_str(),
            "E_EXTERNAL_PC_SOURCE_MISSING_RETRIEVED_AT"
        );
        assert_eq!(
            DiagnosticCode::EExternalPcSourceRequiresHumanVerification.default_severity(),
            DiagnosticSeverity::Error
        );
    }
}
mod case_gui_render_and_frontend_codes_are_stable {
    use super::*;

    #[test]
    fn gui_render_and_frontend_codes_are_stable() {
        assert_eq!(
            DiagnosticCode::EGuiFormInvalid.as_str(),
            "E_GUI_FORM_INVALID"
        );
        assert_eq!(
            DiagnosticCode::EGuiFilePathUnsafe.default_severity(),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            DiagnosticCode::EGuiRenderUnsupported.as_str(),
            "E_GUI_RENDER_UNSUPPORTED"
        );
        assert_eq!(
            DiagnosticCode::ERenderRuntimeSvgForbidden.as_str(),
            "E_RENDER_RUNTIME_SVG_FORBIDDEN"
        );
        assert_eq!(
            DiagnosticCode::ERenderAssetProvenanceMissing.default_severity(),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            DiagnosticCode::EGuiSubprocessForbidden.as_str(),
            "E_GUI_SUBPROCESS_FORBIDDEN"
        );
        assert_eq!(
            DiagnosticCode::EFrontendTypedRequestRequired.as_str(),
            "E_FRONTEND_TYPED_REQUEST_REQUIRED"
        );
        assert_eq!(
            DiagnosticCode::WGuiBackendFallbackRequired.default_severity(),
            DiagnosticSeverity::Warning
        );
    }
}
mod case_query_and_rule_codes_are_stable {
    use super::*;

    #[test]
    fn query_and_rule_codes_are_stable() {
        for code in [
            DiagnosticCode::EPcQueryInvalid,
            DiagnosticCode::ECustomRuleInvalid,
            DiagnosticCode::ESetupQueryInvalid,
        ] {
            assert_eq!(code.default_severity(), DiagnosticSeverity::Error);
        }
        assert_eq!(
            DiagnosticCode::WRuleSrsPlusExtensionsDisabled.default_severity(),
            DiagnosticSeverity::Warning
        );
        assert_eq!(
            DiagnosticCode::WObjectivePatternWeightModelNotMaterialized.as_str(),
            "W_OBJECTIVE_PATTERN_WEIGHT_MODEL_NOT_MATERIALIZED"
        );
        assert_eq!(
            DiagnosticCode::WObjectivePatternWeightModelNotMaterialized.default_severity(),
            DiagnosticSeverity::Warning
        );
        for code in [
            DiagnosticCode::ICustomRuleVerified,
            DiagnosticCode::IPcQueryMvpSupported,
            DiagnosticCode::ISetupQueryMvpSupported,
        ] {
            assert_eq!(code.default_severity(), DiagnosticSeverity::Info);
        }
    }
}
mod case_spin_score_and_coverage_codes_are_stable {
    use super::*;

    #[test]
    fn spin_score_and_coverage_codes_are_stable() {
        assert_eq!(
            DiagnosticCode::EScoreProfileInvalid.default_severity(),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            DiagnosticCode::IScoreProfileMvp2Supported.default_severity(),
            DiagnosticSeverity::Info
        );
        assert_eq!(
            DiagnosticCode::ESpinTargetUnsupported.as_str(),
            "E_SPIN_TARGET_UNSUPPORTED"
        );
        for code in [
            DiagnosticCode::ESpinProfileUnverified,
            DiagnosticCode::ESpinKickEvidenceMissing,
            DiagnosticCode::ESpinClassifierIncompatible,
            DiagnosticCode::EScoreMatrixCapacityExceeded,
            DiagnosticCode::EBuildUpVariantEnumerationTruncated,
            DiagnosticCode::EKickEvidenceBufferExhausted,
        ] {
            assert_eq!(code.default_severity(), DiagnosticSeverity::Error);
        }
        assert_eq!(
            DiagnosticCode::EScoreProfileSpinPolicyIncompatible.as_str(),
            "E_SCORE_PROFILE_SPIN_POLICY_INCOMPATIBLE"
        );
        assert_eq!(
            DiagnosticCode::ESpinCoverageCapacityExceeded.as_str(),
            "E_SPIN_COVERAGE_CAPACITY_EXCEEDED"
        );
        assert_eq!(
            DiagnosticCode::ESpinCoverageUniverseMismatch.as_str(),
            "E_SPIN_COVERAGE_UNIVERSE_MISMATCH"
        );
        assert_eq!(
            DiagnosticCode::ESpinCoverageUniverseMismatch.default_severity(),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            DiagnosticCode::ECoverageCapacityExceeded.as_str(),
            "E_COVERAGE_CAPACITY_EXCEEDED"
        );
        for code in [
            DiagnosticCode::WSpinClassificationEstimated,
            DiagnosticCode::WSpinTargetProbabilityIncomplete,
            DiagnosticCode::WScoreExpectationSampleOnly,
            DiagnosticCode::WSpecialSpinDescriptorOnly,
            DiagnosticCode::WTraceRetentionTruncated,
        ] {
            assert_eq!(code.default_severity(), DiagnosticSeverity::Warning);
        }
        assert_eq!(
            DiagnosticCode::WSpinTargetProbabilityIncomplete.as_str(),
            "W_SPIN_TARGET_PROBABILITY_INCOMPLETE"
        );
        assert_eq!(
            DiagnosticCode::WSpecialSpinDescriptorOnly.as_str(),
            "W_SPECIAL_SPIN_DESCRIPTOR_ONLY"
        );
        assert_eq!(
            DiagnosticCode::WBuildUpEnumerationTruncated.as_str(),
            "W_BUILDUP_ENUMERATION_TRUNCATED"
        );
        assert_eq!(
            DiagnosticCode::WObservedQueueProbabilityIncomplete.as_str(),
            "W_OBSERVED_QUEUE_PROBABILITY_INCOMPLETE"
        );
    }
}
mod case_supply_and_fast_path_codes_are_stable {
    use super::*;

    #[test]
    fn supply_and_fast_path_codes_are_stable() {
        assert_eq!(
            DiagnosticCode::ESupplyInvalidDuplicate.default_severity(),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            DiagnosticCode::WSupplyAmbiguousObservedWindow.default_severity(),
            DiagnosticSeverity::Warning
        );
        assert_eq!(
            DiagnosticCode::ISupplyBoundaryCompatible.default_severity(),
            DiagnosticSeverity::Info
        );
        assert_eq!(
            DiagnosticCode::ISupplyFixedSequenceAccepted.default_severity(),
            DiagnosticSeverity::Info
        );
        assert_eq!(
            DiagnosticCode::WFastPathTwoLineDisabled.default_severity(),
            DiagnosticSeverity::Warning
        );
        assert_eq!(
            DiagnosticCode::IFastPathTwoLineEnabled.default_severity(),
            DiagnosticSeverity::Info
        );
        assert_eq!(
            DiagnosticCode::WMinimumCoverGreedyFallback.default_severity(),
            DiagnosticSeverity::Warning
        );
        assert_eq!(
            DiagnosticCode::WPcObjectiveFirstSolutionFallback.default_severity(),
            DiagnosticSeverity::Warning
        );
        assert_eq!(
            DiagnosticCode::WPcBackendFallback.default_severity(),
            DiagnosticSeverity::Warning
        );
    }
}

mod case_diagnostic_namespace_groups_contract_codes {
    use super::*;

    #[test]
    fn diagnostic_namespace_groups_contract_codes() {
        use super::super::diagnostic_namespace::DiagnosticNamespace;

        assert_eq!(
            DiagnosticCode::EBackendGpuUnavailable.namespace(),
            DiagnosticNamespace::Backend
        );
        assert_eq!(
            DiagnosticCode::ESpinCoverageUniverseMismatch.namespace(),
            DiagnosticNamespace::Spin
        );
        assert_eq!(
            DiagnosticCode::ESupplyInvalidDuplicate.namespace(),
            DiagnosticNamespace::Supply
        );
        assert_eq!(
            DiagnosticCode::EGuiFormInvalid.namespace(),
            DiagnosticNamespace::Gui
        );
    }
}
