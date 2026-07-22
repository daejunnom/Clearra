use super::*;

mod case_gpu_backend_selection_defaults_to_explicit_cpu_fallback {
    use super::*;

    #[test]
    fn gpu_backend_selection_defaults_to_explicit_cpu_fallback() {
        let selection = GpuDeviceSelector::select_default();

        assert_eq!(selection.requested(), GpuBackendKind::NativeCompute);
        assert_eq!(selection.selected(), GpuBackendKind::Disabled);
        assert!(!selection.capability().is_available());
        assert!(selection.fallback_used());
        assert_eq!(
            selection.fallback_reason(),
            Some("native_gpu_backend_not_built")
        );
        assert_eq!(
            selection.gpu_failure_class(),
            Some(GpuExecutionFailureClass::Unavailable)
        );
        assert_eq!(
            selection.gpu_failure_stage(),
            Some(GpuExecutionFailureStage::CapabilityQuery)
        );
        assert_eq!(selection.fallback_backend(), Some(GpuFallbackBackend::Cpu));
        assert!(!selection.discarded_partial_gpu_result());
    }
}

mod case_native_gpu_backend_unavailable_reports_reason {
    use super::*;

    #[test]
    fn native_gpu_backend_unavailable_reports_reason() {
        let capability = GpuBackendCapability::for_kind(GpuBackendKind::NativeCompute);

        assert!(!capability.is_available());
        assert_eq!(
            capability.unavailable_reason(),
            Some("native_gpu_backend_not_built")
        );
        assert!(!capability.accepts_user_shader_path());
    }
}

mod case_gpu_backend_registry_excludes_unimplemented_apis {
    use super::*;

    #[test]
    fn gpu_backend_registry_excludes_unimplemented_apis() {
        let capability = GpuBackendCapability::for_kind(GpuBackendKind::NativeCompute);

        assert!(!capability.is_available());
        assert_eq!(capability.kind().as_str(), "native-gpu");
        assert_eq!(capability.contract_label(), "native-gpu-unavailable");
    }
}

mod case_gpu_backend_rejects_user_provided_shader_path {
    use super::*;

    #[test]
    fn gpu_backend_rejects_user_provided_shader_path() {
        assert_eq!(
            GpuDeviceSelector::reject_user_provided_shader_path(Some("kernel.spv")),
            Err(GpuBackendError::UserProvidedShaderPathRejected)
        );
        assert!(GpuDeviceSelector::reject_user_provided_shader_path(None).is_ok());
        assert!(GpuDeviceSelector::reject_user_provided_shader_path(Some("")).is_ok());
    }
}

mod case_gpu_backend_fallback_allowed_uses_cpu {
    use super::*;

    #[test]
    fn gpu_backend_fallback_allowed_uses_cpu() {
        let selection =
            GpuDeviceSelector::select(GpuBackendKind::NativeCompute, true).expect("fallback");

        assert_eq!(selection.requested(), GpuBackendKind::NativeCompute);
        assert_eq!(selection.selected(), GpuBackendKind::Disabled);
        assert!(selection.fallback_used());
        assert_eq!(
            selection.fallback_reason(),
            Some("native_gpu_backend_not_built")
        );
    }
}

mod case_gpu_backend_no_fallback_returns_error {
    use super::*;

    #[test]
    fn gpu_backend_no_fallback_returns_error() {
        let result = GpuDeviceSelector::select(GpuBackendKind::NativeCompute, false);

        assert_eq!(
            result,
            Err(GpuBackendError::BackendUnavailable {
                kind: GpuBackendKind::NativeCompute,
                reason: "native_gpu_backend_not_built",
            })
        );
    }
}

mod case_gpu_worker_unconfirmed_result_cannot_source_exact_probability {
    use super::*;

    #[test]
    fn gpu_worker_unconfirmed_result_cannot_source_exact_probability() {
        let failure = GpuExecutionFailure::transient_before_commit(
            GpuExecutionFailureStage::Readback,
            GpuPartialResultDisposition::RetainedIncomplete,
        )
        .expect("pre-commit stage")
        .resolve(BackendFallbackPolicy::Allow);
        let result = GpuWorkerResult::from_failure(
            7,
            3,
            failure,
            ticket(99),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );

        assert!(!result.can_source_exact_probability());
        assert!(result.cpu_confirm_required());
        assert_eq!(result.trust_state().as_str(), "gpu-computed-unconfirmed");
        assert!(!failure.fallback_used());
        assert_eq!(
            failure.class(),
            GpuExecutionFailureClass::TransientBeforeCommit
        );
    }
}

mod case_gpu_worker_cpu_confirmed_result_can_source_exact_probability {
    use super::*;

    #[test]
    fn gpu_worker_cpu_confirmed_result_can_source_exact_probability() {
        let result = GpuWorkerResult::new(
            7,
            3,
            GpuTrustState::GpuComputedCpuConfirmed,
            false,
            None,
            ticket(99),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );

        assert!(result.can_source_exact_probability());
    }
}

mod case_gpu_unconfirmed_result_reduces_to_prefilter_only {
    use super::*;

    #[test]
    fn gpu_unconfirmed_result_reduces_to_prefilter_only() {
        let result = GpuWorkerResult::new(
            7,
            3,
            GpuTrustState::GpuComputedUnconfirmed,
            true,
            None,
            ticket(99),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );

        let reduction = GpuWorkerResultReducer::reduce(result);

        match reduction {
            GpuWorkerReduction::PrefilterOnly {
                cpu_confirm_required,
                report,
                ..
            } => {
                assert!(cpu_confirm_required);
                assert_eq!(
                    report.exactness_gate(),
                    GpuWorkerExactnessGate::PrefilterOnly
                );
                assert!(!report.can_source_exact_probability());
                assert!(!report.can_accept_build_variant());
            }
            other => panic!("expected prefilter-only reduction, got {other:?}"),
        }
    }
}

mod case_gpu_cpu_confirmed_result_reduces_to_exact_candidate_source {
    use super::*;

    #[test]
    fn gpu_cpu_confirmed_result_reduces_to_exact_candidate_source() {
        let result = GpuWorkerResult::new(
            7,
            3,
            GpuTrustState::GpuComputedCpuConfirmed,
            false,
            None,
            ticket(99),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );

        let reduction = GpuWorkerResultReducer::reduce(result);

        match reduction {
            GpuWorkerReduction::ExactCandidateSource { report, .. } => {
                assert_eq!(
                    report.exactness_gate(),
                    GpuWorkerExactnessGate::ExactCandidateSource
                );
                assert!(report.can_source_exact_probability());
                assert!(report.can_accept_build_variant());
            }
            other => panic!("expected exact candidate reduction, got {other:?}"),
        }
    }
}

mod case_gpu_deterministic_reference_result_reduces_to_exact_candidate_source {
    use super::*;

    #[test]
    fn gpu_deterministic_reference_result_reduces_to_exact_candidate_source() {
        let result = GpuWorkerResult::new(
            7,
            3,
            GpuTrustState::DeterministicReferenceMatched,
            false,
            None,
            ticket(99),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );

        let reduction = GpuWorkerResultReducer::reduce(result);

        assert!(matches!(
            reduction,
            GpuWorkerReduction::ExactCandidateSource { .. }
        ));
    }
}

mod case_gpu_fallback_result_reduces_to_backend_fallback_report {
    use super::*;

    #[test]
    fn gpu_fallback_result_reduces_to_backend_fallback_report() {
        let failure = GpuExecutionFailure::unavailable(
            GpuExecutionFailureStage::CapabilityQuery,
            SearchBackendFallbackReason::GpuFeatureDisabled,
        )
        .resolve(BackendFallbackPolicy::Allow);
        let result = GpuWorkerResult::from_failure(
            7,
            0,
            failure,
            ticket(99),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );

        let reduction = GpuWorkerResultReducer::reduce(result);

        match reduction {
            GpuWorkerReduction::Fallback { reason, report, .. } => {
                assert_eq!(reason, SearchBackendFallbackReason::GpuFeatureDisabled);
                assert_eq!(
                    report.fallback_reason(),
                    Some(SearchBackendFallbackReason::GpuFeatureDisabled)
                );
                assert_eq!(
                    report.exactness_gate(),
                    GpuWorkerExactnessGate::BackendFallback
                );
                assert!(!report.can_source_exact_probability());
                assert_eq!(
                    report.gpu_failure_class(),
                    Some(GpuExecutionFailureClass::Unavailable)
                );
                assert_eq!(
                    report.gpu_failure_stage(),
                    Some(GpuExecutionFailureStage::CapabilityQuery)
                );
                assert!(report.fallback_used());
                assert!(!report.discarded_partial_gpu_result());
            }
            other => panic!("expected fallback reduction, got {other:?}"),
        }
    }
}

mod case_gpu_mismatch_result_is_rejected {
    use super::*;

    #[test]
    fn gpu_mismatch_result_is_rejected() {
        let failure =
            GpuExecutionFailure::trust_mismatch(GpuExecutionFailureStage::CpuReferenceConfirm)
                .resolve(BackendFallbackPolicy::Allow);
        let result = GpuWorkerResult::from_failure(
            7,
            3,
            failure,
            ticket(99),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );

        let reduction = GpuWorkerResultReducer::reduce(result);

        match reduction {
            GpuWorkerReduction::RejectedMismatch { report, .. } => {
                assert_eq!(
                    report.exactness_gate(),
                    GpuWorkerExactnessGate::RejectedMismatch
                );
                assert!(!report.can_source_exact_probability());
                assert!(!report.can_accept_build_variant());
                assert_eq!(
                    report.gpu_failure_class(),
                    Some(GpuExecutionFailureClass::TrustMismatch)
                );
                assert!(!report.fallback_used());
            }
            other => panic!("expected rejected mismatch reduction, got {other:?}"),
        }
    }
}

mod case_gpu_confirmed_candidate_can_enter_cpu_buildup_queue {
    use super::*;

    #[test]
    fn gpu_confirmed_candidate_can_enter_cpu_buildup_queue() {
        let result = GpuWorkerResult::new(
            7,
            3,
            GpuTrustState::GpuComputedCpuConfirmed,
            false,
            None,
            ticket(99),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );
        let reduction = GpuWorkerResultReducer::reduce(result);

        let decision = GpuCpuConfirmBridge::route_reduction(&reduction).expect("bridge decision");

        assert!(decision.cpu_confirmed());
        assert!(decision.can_enter_cpu_buildup_queue());
        assert!(!decision.can_create_coverage_row());
        assert!(!decision.candidate_is_solution());
        assert_eq!(
            decision.exactness_gate(),
            GpuWorkerExactnessGate::ExactCandidateSource
        );
    }
}

mod case_gpu_unconfirmed_candidate_cannot_enter_cpu_buildup_queue {
    use super::*;

    #[test]
    fn gpu_unconfirmed_candidate_cannot_enter_cpu_buildup_queue() {
        let result = GpuWorkerResult::new(
            7,
            3,
            GpuTrustState::GpuComputedUnconfirmed,
            true,
            None,
            ticket(99),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );
        let reduction = GpuWorkerResultReducer::reduce(result);

        let decision = GpuCpuConfirmBridge::route_reduction(&reduction);

        assert_eq!(
            decision,
            Err(GpuCpuConfirmBridgeError::UnconfirmedCandidate)
        );
    }
}

mod case_gpu_unconfirmed_candidate_cannot_create_coverage_row {
    use super::*;

    #[test]
    fn gpu_unconfirmed_candidate_cannot_create_coverage_row() {
        let result = GpuWorkerResult::new(
            7,
            3,
            GpuTrustState::GpuComputedUnconfirmed,
            true,
            None,
            ticket(99),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );
        let reduction = GpuWorkerResultReducer::reduce(result);

        let decision = GpuCpuConfirmBridge::route_reduction(&reduction);

        assert_eq!(
            decision,
            Err(GpuCpuConfirmBridgeError::UnconfirmedCandidate)
        );
    }
}

mod case_gpu_candidate_cannot_create_coverage_row_before_buildup {
    use super::*;

    #[test]
    fn gpu_candidate_cannot_create_coverage_row_before_buildup() {
        let result = GpuWorkerResult::new(
            7,
            3,
            GpuTrustState::GpuComputedCpuConfirmed,
            false,
            None,
            ticket(99),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );
        let reduction = GpuWorkerResultReducer::reduce(result);

        let decision = GpuCpuConfirmBridge::route_reduction(&reduction).expect("bridge decision");

        assert!(decision.cpu_confirmed());
        assert!(decision.can_enter_cpu_buildup_queue());
        assert!(!decision.can_create_coverage_row());
        assert!(!decision.candidate_is_solution());
    }
}

mod case_gpu_request_does_not_materialize_fake_result_without_backend {
    use super::*;

    #[test]
    fn gpu_request_does_not_materialize_fake_result_without_backend() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(7))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");
        let request = GpuWorkerRequest::new(
            11,
            descriptor,
            5,
            GpuMemoryTicket::new(42, GpuFenceEpoch::new(3), 4096),
            true,
        )
        .expect("GPU request");

        assert_eq!(request.batch(), descriptor);
        assert_eq!(request.request_id(), 11);
        assert_eq!(request.candidate_count_hint(), 5);
        assert_eq!(request.memory_ticket().id(), 42);
    }
}

mod case_gpu_worker_result_requires_memory_ticket {
    use super::*;

    #[test]
    fn gpu_worker_result_requires_memory_ticket() {
        let result = GpuWorkerResult::new(
            7,
            3,
            GpuTrustState::DeterministicReferenceMatched,
            false,
            None,
            ticket(99),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );

        assert_eq!(result.memory_ticket_id(), 99);
        assert_ne!(result.memory_ticket_id(), 0);
        assert_ne!(result.fence_epoch(), 0);
        assert_ne!(result.scope_epoch(), 0);
        assert_ne!(result.byte_budget(), 0);
        assert!(validate_gpu_worker_result(&result).is_ok());
    }
}

mod case_gpu_unconfirmed_probability_rejected {
    use super::*;

    #[test]
    fn gpu_unconfirmed_probability_rejected() {
        let result = GpuWorkerResult::new(
            7,
            3,
            GpuTrustState::GpuComputedUnconfirmed,
            true,
            None,
            ticket(99),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );

        assert!(!result.can_source_exact_probability());
        assert!(
            !GpuWorkerExactnessGate::for_trust_state(result.trust_state())
                .can_source_exact_probability()
        );
        assert!(matches!(
            GpuWorkerResultReducer::reduce(result),
            GpuWorkerReduction::PrefilterOnly {
                cpu_confirm_required: true,
                ..
            }
        ));
    }
}

mod case_gpu_worker_fallback_result_carries_reason {
    use super::*;

    #[test]
    fn gpu_worker_fallback_result_carries_reason() {
        let failure = GpuExecutionFailure::resource_incomplete(
            GpuExecutionFailureStage::Readback,
            GpuPartialResultDisposition::Discarded,
        )
        .expect("resource failure includes a partial result")
        .resolve(BackendFallbackPolicy::Allow);
        let result = GpuWorkerResult::from_failure(
            5,
            0,
            failure,
            ticket(12),
            GpuWorkerBackpressure::idle("gpu-worker-v0.1"),
        );

        assert!(!result.can_source_exact_probability());
        assert_eq!(
            result
                .fallback_reason()
                .map(SearchBackendFallbackReason::as_str),
            Some("gpu_resource_incomplete")
        );
        let report = match GpuWorkerResultReducer::reduce(result) {
            GpuWorkerReduction::Fallback { report, .. } => report,
            other => panic!("expected resource fallback, got {other:?}"),
        };
        assert_eq!(
            report.gpu_failure_class(),
            Some(GpuExecutionFailureClass::ResourceIncomplete)
        );
        assert!(report.discarded_partial_gpu_result());
    }
}
