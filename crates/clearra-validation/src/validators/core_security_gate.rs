use clearra_core_ffi::{
    CBuildVariantViewError, CClrMemStatus, CoreAbiVersion, CoreLeakReport, FfiProblemError,
    CLEARRA_CORE_ABI_VERSION,
};

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

mod core_execution_stage {
    use crate::diagnostic::diagnostic_code::DiagnosticCode;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CoreExecutionStage {
        Packing,
        BuildUp,
    }

    impl CoreExecutionStage {
        pub(super) fn code(self) -> DiagnosticCode {
            match self {
                Self::Packing => DiagnosticCode::ECorePackingFailed,
                Self::BuildUp => DiagnosticCode::ECoreBuildUpFailed,
            }
        }
    }
    impl CoreExecutionStage {
        pub(super) fn label(self) -> &'static str {
            match self {
                Self::Packing => "packing",
                Self::BuildUp => "buildup",
            }
        }
    }
}
mod core_result_buffer_kind {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CoreResultBufferKind {
        PackingCandidateBuffer,
        BuildUpResultBuffer,
        CoverageRowBuffer,
    }

    impl CoreResultBufferKind {
        pub(super) fn label(self) -> &'static str {
            match self {
                Self::PackingCandidateBuffer => "packing-candidate-buffer",
                Self::BuildUpResultBuffer => "buildup-result-buffer",
                Self::CoverageRowBuffer => "coverage-row-buffer",
            }
        }
    }
}
mod gate {
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CoreSecurityGate;
}

pub use core_execution_stage::CoreExecutionStage;
pub use core_result_buffer_kind::CoreResultBufferKind;
pub use gate::CoreSecurityGate;

mod backend_fallback_used {
    use super::*;

    impl CoreSecurityGate {
        pub fn backend_fallback_used(
            requested_backend: &'static str,
            selected_backend: &'static str,
            reason: &'static str,
            location: &'static str,
        ) -> Diagnostic {
            Diagnostic::new(
                DiagnosticCode::WBackendFallbackUsed,
                "requested backend fell back to a different executor backend",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new(
                "requested_backend",
                requested_backend,
            ))
            .with_evidence(ValidationEvidence::new(
                "selected_backend",
                selected_backend,
            ))
            .with_evidence(ValidationEvidence::new("fallback_reason", reason))
        }
    }
}
mod build_up_variant_enumeration_truncated {
    use super::*;

    impl CoreSecurityGate {
        pub fn build_up_variant_enumeration_truncated(
            total_variant_count: usize,
            retained_variant_count: usize,
            truncation_reason: &'static str,
            location: &'static str,
        ) -> Diagnostic {
            Diagnostic::new(
                DiagnosticCode::EBuildUpVariantEnumerationTruncated,
                "BuildUp variant enumeration was truncated before a complete exact result was available",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new(
                "total_variant_count",
                total_variant_count.to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "retained_variant_count",
                retained_variant_count.to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "count_complete",
                "false",
            ))
            .with_evidence(ValidationEvidence::new(
                "truncation_reason",
                truncation_reason,
            ))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Do not use this partial BuildUp result as coverage, min-cover, all-solutions, or exact probability evidence.",
            ))
        }
    }
}
mod build_variant_view_error {
    use super::*;

    impl CoreSecurityGate {
        pub fn build_variant_view_error(
            error: CBuildVariantViewError,
            location: &'static str,
        ) -> Diagnostic {
            match error {
                CBuildVariantViewError::KickEvidenceCountExceeded { count, max } => {
                    Diagnostic::new(
                        DiagnosticCode::ECoreFfiBufferBounds,
                        "C BuildVariant kick evidence count exceeded the Rust FFI boundary limit",
                    )
                    .with_location(EvidenceLocation::new(location))
                    .with_evidence(ValidationEvidence::new(
                        "kick_evidence_count",
                        count.to_string(),
                    ))
                    .with_evidence(ValidationEvidence::new(
                        "kick_evidence_limit",
                        max.to_string(),
                    ))
                    .with_suggested_next_step(SuggestedNextStep::new(
                        "Reject the native view before reading the pointer/count buffer.",
                    ))
                }
                CBuildVariantViewError::MissingKickEvidencePointer { count } => Diagnostic::new(
                    DiagnosticCode::ECoreInvalidNativeView,
                    "C BuildVariant kick evidence count was nonzero but the pointer was null",
                )
                .with_location(EvidenceLocation::new(location))
                .with_evidence(ValidationEvidence::new(
                    "kick_evidence_count",
                    count.to_string(),
                ))
                .with_suggested_next_step(SuggestedNextStep::new(
                    "Reject malformed native views instead of fabricating exact kick evidence.",
                )),
                CBuildVariantViewError::OperationOrderCountExceeded { count, max }
                | CBuildVariantViewError::TraceStepCountExceeded { count, max } => Diagnostic::new(
                    DiagnosticCode::ECoreFfiBufferBounds,
                    "C BuildVariant trace count exceeded the Rust FFI boundary limit",
                )
                .with_location(EvidenceLocation::new(location))
                .with_evidence(ValidationEvidence::new("trace_count", count.to_string()))
                .with_evidence(ValidationEvidence::new("trace_limit", max.to_string()))
                .with_suggested_next_step(SuggestedNextStep::new(
                    "Reject the native trace view before reading its pointer/count buffers.",
                )),
                CBuildVariantViewError::MissingOperationOrderPointer { count }
                | CBuildVariantViewError::MissingTraceStepsPointer { count } => Diagnostic::new(
                    DiagnosticCode::ECoreInvalidNativeView,
                    "C BuildVariant trace count was nonzero but its pointer was null",
                )
                .with_location(EvidenceLocation::new(location))
                .with_evidence(ValidationEvidence::new("trace_count", count.to_string()))
                .with_suggested_next_step(SuggestedNextStep::new(
                    "Reject malformed native trace views before replay materialization.",
                )),
                CBuildVariantViewError::TraceCountMismatch {
                    operation_count,
                    trace_count,
                } => Diagnostic::new(
                    DiagnosticCode::ECoreInvalidNativeView,
                    "C BuildVariant operation-order and trace-step counts did not match",
                )
                .with_location(EvidenceLocation::new(location))
                .with_evidence(ValidationEvidence::new(
                    "operation_order_count",
                    operation_count.to_string(),
                ))
                .with_evidence(ValidationEvidence::new(
                    "trace_step_count",
                    trace_count.to_string(),
                )),
                CBuildVariantViewError::OperationOrderMismatch { step_index, .. } => {
                    Diagnostic::new(
                        DiagnosticCode::ECoreInvalidNativeView,
                        "C BuildVariant operation order did not match its trace steps",
                    )
                    .with_location(EvidenceLocation::new(location))
                    .with_evidence(ValidationEvidence::new(
                        "trace_step_index",
                        step_index.to_string(),
                    ))
                }
                CBuildVariantViewError::KickEvidenceIndexOutOfRange {
                    step_index,
                    evidence_index,
                    evidence_count,
                } => Diagnostic::new(
                    DiagnosticCode::ECoreInvalidNativeView,
                    "C BuildVariant trace step referenced kick evidence outside the owned buffer",
                )
                .with_location(EvidenceLocation::new(location))
                .with_evidence(ValidationEvidence::new(
                    "trace_step_index",
                    step_index.to_string(),
                ))
                .with_evidence(ValidationEvidence::new(
                    "kick_evidence_index",
                    evidence_index.to_string(),
                ))
                .with_evidence(ValidationEvidence::new(
                    "kick_evidence_count",
                    evidence_count.to_string(),
                )),
            }
        }
    }
}
mod buildup_enumeration_truncated {
    use super::*;

    impl CoreSecurityGate {
        pub fn buildup_enumeration_truncated(
            accepted_variant_count: usize,
            variant_limit: usize,
            location: &'static str,
        ) -> Diagnostic {
            Diagnostic::new(
                DiagnosticCode::WBuildUpEnumerationTruncated,
                "C BuildUp variant enumeration stopped at the configured budget",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new(
                "accepted_variant_count",
                accepted_variant_count.to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "variant_limit",
                variant_limit.to_string(),
            ))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Report probability_complete=false or increase the BuildUp enumerate variant limit.",
            ))
        }
    }
}
mod c_status {
    use super::*;

    impl CoreSecurityGate {
        pub fn c_status(status: CClrMemStatus, location: &'static str) -> Option<Diagnostic> {
            if status == CClrMemStatus::Ok {
                return None;
            }

            Some(
                Diagnostic::new(
                    DiagnosticCode::ECMemoryScopeInvalid,
                    "C memory scope returned an invalid status",
                )
                .with_location(EvidenceLocation::new(location))
                .with_evidence(ValidationEvidence::new("c_status", format!("{status:?}")))
                .with_suggested_next_step(SuggestedNextStep::new(
                    "Treat C memory scope failures as hard errors; do not continue with possibly invalid buffers.",
                )),
            )
        }
    }
}
mod core_abi_version_mismatch {
    use super::*;

    impl CoreSecurityGate {
        pub fn core_abi_version_mismatch(
            actual: CoreAbiVersion,
            location: &'static str,
        ) -> DiagnosticReport {
            let mut report = DiagnosticReport::new();
            if !actual.is_compatible_with(CLEARRA_CORE_ABI_VERSION) {
                report.push(
                    Diagnostic::new(
                        DiagnosticCode::ECoreAbiVersionMismatch,
                        "C core ABI version does not match the Rust FFI contract",
                    )
                    .with_location(EvidenceLocation::new(location))
                    .with_evidence(ValidationEvidence::new(
                        "expected_abi_version",
                        CLEARRA_CORE_ABI_VERSION.to_string(),
                    ))
                    .with_evidence(ValidationEvidence::new(
                        "actual_abi_version",
                        actual.value().to_string(),
                    ))
                    .with_suggested_next_step(SuggestedNextStep::new(
                        "Rebuild core-c and clearra-core-ffi from the same workspace revision.",
                    )),
                );
            }
            report
        }
    }
}
mod coverage_capacity_exceeded {
    use super::*;

    impl CoreSecurityGate {
        pub fn coverage_capacity_exceeded(
            row_count: usize,
            row_limit: usize,
            location: &'static str,
        ) -> Diagnostic {
            Diagnostic::new(
                DiagnosticCode::ECoverageCapacityExceeded,
                "coverage row matrix exceeded its configured memory budget",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new("row_count", row_count.to_string()))
            .with_evidence(ValidationEvidence::new("row_limit", row_limit.to_string()))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Reject the partial coverage result; do not report an empty or complete coverage set.",
            ))
        }
    }
}
mod ffi_problem_error {
    use super::*;

    impl CoreSecurityGate {
        pub fn ffi_problem_error(
            stage: CoreExecutionStage,
            error: FfiProblemError,
            location: &'static str,
        ) -> Diagnostic {
            Diagnostic::new(
                stage.code(),
                format!(
                    "C core {} problem descriptor failed: {error:?}",
                    stage.label()
                ),
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new("stage", stage.label()))
            .with_evidence(ValidationEvidence::new("ffi_error", format!("{error:?}")))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Reject the compiled problem before handing it to core-c.",
            ))
        }
    }
}
mod gpu_result_cpu_confirm_required {
    use super::*;

    impl CoreSecurityGate {
        pub fn gpu_result_cpu_confirm_required(
            reason: &'static str,
            location: &'static str,
        ) -> Diagnostic {
            Diagnostic::new(
                DiagnosticCode::WGpuResultCpuConfirmRequired,
                "GPU packing result requires CPU exact confirmation before it can be trusted",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new("reason", reason))
        }
    }
}
mod gpu_unavailable {
    use super::*;

    impl CoreSecurityGate {
        pub fn gpu_unavailable(reason: &'static str, location: &'static str) -> Diagnostic {
            Diagnostic::new(
                DiagnosticCode::EBackendGpuUnavailable,
                "requested GPU backend is unavailable",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new("reason", reason))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Use a CPU backend, enable the GPU feature, or explicitly allow backend fallback.",
            ))
        }
    }
}
mod invalid_c_result_buffer {
    use super::*;

    impl CoreSecurityGate {
        pub fn invalid_c_result_buffer(
            kind: CoreResultBufferKind,
            reason: &'static str,
            location: &'static str,
        ) -> Diagnostic {
            Diagnostic::new(
                DiagnosticCode::ECoreBuildUpFailed,
                "invalid C result buffer was rejected before coverage/objective reduction",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new("buffer_kind", kind.label()))
            .with_evidence(ValidationEvidence::new("reason", reason))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Reject malformed core-c buffers instead of truncating or repairing them.",
            ))
        }
    }
}
mod kick_evidence_buffer_exhausted {
    use super::*;

    impl CoreSecurityGate {
        pub fn kick_evidence_buffer_exhausted(
            evidence_count: usize,
            evidence_limit: usize,
            location: &'static str,
        ) -> Diagnostic {
            Diagnostic::new(
                DiagnosticCode::EKickEvidenceBufferExhausted,
                "C BuildVariant kick evidence buffer was exhausted",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new(
                "kick_evidence_count",
                evidence_count.to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "kick_evidence_limit",
                evidence_limit.to_string(),
            ))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Treat the variant as incomplete; do not estimate exact spin classification without full kick evidence.",
            ))
        }
    }
}
mod memory_leak_report {
    use super::*;

    impl CoreSecurityGate {
        pub fn memory_leak_report(
            report: CoreLeakReport,
            location: &'static str,
        ) -> DiagnosticReport {
            let mut diagnostics = DiagnosticReport::new();
            if report.is_zero() {
                return diagnostics;
            }

            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::ECMemoryLeakDetected,
                    "C memory leak report is not clean after core execution",
                )
                .with_location(EvidenceLocation::new(location))
                .with_evidence(ValidationEvidence::new(
                    "live_search_scopes",
                    report.live_search_scopes.to_string(),
                ))
                .with_evidence(ValidationEvidence::new(
                    "live_batch_scopes",
                    report.live_batch_scopes.to_string(),
                ))
                .with_suggested_next_step(SuggestedNextStep::new(
                    "Release or abort all core-c scopes before accepting executor output.",
                )),
            );
            diagnostics
        }
    }
}
mod observed_queue_probability_incomplete {
    use super::*;

    impl CoreSecurityGate {
        pub fn observed_queue_probability_incomplete(
            materialized_probability_mass: &'static str,
            truncation_reason: &'static str,
            location: &'static str,
        ) -> Diagnostic {
            Diagnostic::new(
                DiagnosticCode::WObservedQueueProbabilityIncomplete,
                "observed queue expansion was truncated and probability mass is incomplete",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new(
                "materialized_probability_mass",
                materialized_probability_mass,
            ))
            .with_evidence(ValidationEvidence::new("renormalized", "false"))
            .with_evidence(ValidationEvidence::new(
                "probability_complete",
                "false",
            ))
            .with_evidence(ValidationEvidence::new(
                "truncation_reason",
                truncation_reason,
            ))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Show the incomplete probability mass; do not renormalize truncated observed queue coverage to 100%.",
            ))
        }
    }
}
mod packing_candidate_used_as_solution {
    use super::*;

    impl CoreSecurityGate {
        pub fn packing_candidate_used_as_solution(location: &'static str) -> Diagnostic {
            Diagnostic::new(
                DiagnosticCode::EPackingCandidateUsedAsSolution,
                "PackingCandidate was used as a solution before BuildUp verification",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new(
                "required_stage",
                "C BuildUp verification",
            ))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Promote only BuildUp-verified variants to solution/output contracts.",
            ))
        }
    }
}
mod score_matrix_capacity_exceeded {
    use super::*;

    impl CoreSecurityGate {
        pub fn score_matrix_capacity_exceeded(
            row_count: usize,
            row_limit: usize,
            location: &'static str,
        ) -> Diagnostic {
            Diagnostic::new(
                DiagnosticCode::EScoreMatrixCapacityExceeded,
                "score-cell matrix exceeded its configured memory budget",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new("row_count", row_count.to_string()))
            .with_evidence(ValidationEvidence::new("row_limit", row_limit.to_string()))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Increase the score-cell matrix budget or reduce the requested score objective surface.",
            ))
        }
    }
}
mod spin_coverage_capacity_exceeded {
    use super::*;

    impl CoreSecurityGate {
        pub fn spin_coverage_capacity_exceeded(
            row_count: usize,
            row_limit: usize,
            location: &'static str,
        ) -> Diagnostic {
            Diagnostic::new(
                DiagnosticCode::ESpinCoverageCapacityExceeded,
                "spin coverage matrix exceeded its configured memory budget",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new("row_count", row_count.to_string()))
            .with_evidence(ValidationEvidence::new("row_limit", row_limit.to_string()))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Increase the spin coverage matrix budget or narrow the SpinTarget query.",
            ))
        }
    }
}
mod trace_retention_truncated {
    use super::*;

    impl CoreSecurityGate {
        pub fn trace_retention_truncated(
            retained_trace_count: usize,
            total_solution_count: usize,
            trace_retention_reason: &'static str,
            location: &'static str,
        ) -> Diagnostic {
            Diagnostic::new(
                DiagnosticCode::WTraceRetentionTruncated,
                "solution count is complete but only a bounded trace sample was retained",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new(
                "retained_trace_count",
                retained_trace_count.to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "total_solution_count",
                total_solution_count.to_string(),
            ))
            .with_evidence(ValidationEvidence::new("trace_retention_truncated", "true"))
            .with_evidence(ValidationEvidence::new(
                "trace_retention_reason",
                trace_retention_reason,
            ))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Keep retained trace count separate from total solution count in output.",
            ))
        }
    }
}

#[cfg(test)]
#[path = "core_security_gate_tests.rs"]
mod tests;
