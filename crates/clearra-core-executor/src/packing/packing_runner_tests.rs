#![cfg_attr(not(feature = "native-c-core"), allow(dead_code, unused_imports))]

use std::cell::Cell;

use clearra_core_domain::resource::ResourceReport;
use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
#[cfg(not(feature = "native-c-core"))]
use clearra_core_ffi::NativeCoreError;
use clearra_core_ffi::{
    problem::CPackingProblem, supply::C_PIECE_SOURCE_FIXED_QUEUE, CBuildUpProblemBuilder,
    CNativeBuildUpEnumerationLimits, CoreCNative, FfiProblemError,
};
use clearra_pc_graph::request::{
    GpuDeviceSelection, OpeningPcSearchQuery, PcExecutionPolicy, PcQueueInput,
    RequestedSearchBackend,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::{
    kicks::{KickTableProfile, KickTableProfileId, SrsKicks, VerifiedKickTableProfile},
    profile::{
        builtin_rules::srs_x,
        rule_profile::{RuleProfile, RuleProfileId},
    },
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::{
    backend::{
        BackendTrustReport, CapabilityQueryError, GpuExecutionFailure, GpuExecutionFailureClass,
        GpuExecutionFailureStage, GpuPartialResultDisposition, GpuSearchCapability,
        GpuUnavailableReason, PackingBackendOutcome, SearchBackendCapabilityProvider,
        SearchBackendExecutor, SearchBackendExecutorResolver, SelectedSearchBackend,
    },
    buildup::BuildUpRunner,
    packing::{packing_metrics::PackingExecutionSource, PackingRunner, PackingRunnerError},
};

#[cfg(feature = "native-c-core")]
#[test]
fn packing_runner_builds_c_packing_problem_and_candidate_buffer() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

    let result = PackingRunner::run(&problem).expect("packing");

    assert_eq!(
        result.compact_problem().problem_kind,
        CPackingProblem::OPENING_PC
    );
    assert_eq!(
        result.compact_problem().piece_source.source_kind,
        C_PIECE_SOURCE_FIXED_QUEUE
    );
    assert_eq!(result.candidate_count(), 4);
    assert!(result.count_complete());
    assert_eq!(result.truncation_reason(), None);
    assert_eq!(
        result.candidate_at(0).expect("candidate").operation_count,
        5
    );
    assert!(result
        .candidates()
        .all(|candidate| candidate.geometry_variant_domains != 0));
    assert_eq!(
        result.execution_source(),
        PackingExecutionSource::NativeCpuPacking
    );
    assert_eq!(
        result.backend_report().selected_backend().as_str(),
        "cpu-geometry-exact-cover"
    );
    assert_eq!(
        result.gpu_packing_report().backend_scope(),
        "native-gpu-packing"
    );
    assert!(result.gpu_packing_report().hash_exact_confirm_required());
    assert!(!result.gpu_packing_report().larger_batch_planner());
    assert!(!result.gpu_packing_report().dominance_prefilter());
    assert!(!result.gpu_packing_report().shape_union_mask());
    assert!(!result.gpu_packing_report().readback_compression());
    assert!(!result.gpu_packing_report().cpu_reference_confirmed());
    assert!(!result.gpu_packing_report().cpu_reference_match());
    assert!(!result.gpu_packing_report().deterministic_result());
    assert!(!result.hybrid_scheduler_report().enabled());
    assert!(!result
        .hybrid_scheduler_report()
        .gpu_readback_cpu_buildup_overlap());
    assert!(!result.hybrid_scheduler_report().memory_epoch_managed());
    assert!(result.memory_report().memory_leak_report_clean());
    assert!(result.memory_report().transient_scope_release_complete());
    assert_eq!(
        result.memory_report().leak_check_state(),
        crate::packing::PackingMemoryLeakCheckState::OwnershipReleaseConfirmed
    );
}

#[cfg(feature = "native-c-core")]
#[test]
fn opening_4l_packing_is_complete_before_buildup() {
    let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

    let result = PackingRunner::run(&problem).expect("packing");
    eprintln!(
        "4L packing candidates={} peak_frontier={} peak_cpu_bytes={}",
        result.candidate_count(),
        result.resource_report().peak_frontier_states,
        result.resource_report().peak_cpu_bytes,
    );

    assert!(result.count_complete());
    assert!(result.candidate_count() > 0);

    let candidate = result.candidate_at(0).expect("4L candidate");
    let buildup = CBuildUpProblemBuilder::from_packing_candidate(&problem, &candidate, 0, 0)
        .expect("4L BuildUp problem");
    let started = std::time::Instant::now();
    let outcome = CoreCNative::enumerate_buildup_variants(
        &buildup,
        &CNativeBuildUpEnumerationLimits {
            max_variants: 1,
            preserve_hold_branches: 1,
            prefer_highest_t_spin_trace: 0,
            reserved: [0; 6],
        },
    )
    .expect("4L first-candidate BuildUp");
    eprintln!(
        "4L first buildup elapsed_ms={} status={} variants={} states={} memo_probes={} memo_hits={} memo_insertions={} memo_saturation={} memo_capacity={} memo_max_probe={}",
        started.elapsed().as_millis(),
        outcome.status,
        outcome.buffer.total_variant_count,
        outcome.buffer.search_metrics.expanded_state_count,
        outcome.buffer.search_metrics.memo_probes,
        outcome.buffer.search_metrics.memo_hits,
        outcome.buffer.search_metrics.memo_insertions,
        outcome.buffer.search_metrics.memo_saturation_skips,
        outcome.buffer.search_metrics.memo_capacity,
        outcome.buffer.search_metrics.memo_max_probe_length,
    );
}

#[cfg(feature = "native-c-core")]
#[test]
#[ignore = "manual BuildUp long-tail performance profile"]
fn profile_opening_4l_buildup_candidate_distribution() {
    let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
    let packing = PackingRunner::run(&problem).expect("packing");
    let product_started = std::time::Instant::now();
    let product_buildup = BuildUpRunner::run(&problem, &packing).expect("parallel product BuildUp");
    eprintln!(
        "4L product BuildUp elapsed_ms={} variants={} traces={} complete={}",
        product_started.elapsed().as_millis(),
        product_buildup.total_solution_count(),
        product_buildup.unique_trace_count(),
        product_buildup.count_complete(),
    );
    let limits = CNativeBuildUpEnumerationLimits {
        max_variants: 1,
        preserve_hold_branches: 1,
        prefer_highest_t_spin_trace: 0,
        reserved: [0; 6],
    };
    let profile_started = std::time::Instant::now();
    let mut total_states = 0u64;
    let mut max_states = 0u64;
    let mut max_state_candidate = 0u64;

    for (index, candidate) in packing.candidates().enumerate() {
        let buildup = CBuildUpProblemBuilder::from_packing_candidate(&problem, &candidate, 0, 0)
            .expect("4L BuildUp problem");
        let started = std::time::Instant::now();
        let outcome = CoreCNative::enumerate_buildup_variants(&buildup, &limits)
            .expect("4L candidate BuildUp");
        let elapsed = started.elapsed();
        let metrics = outcome.buffer.search_metrics;
        total_states = total_states.saturating_add(metrics.expanded_state_count);
        if metrics.expanded_state_count > max_states {
            max_states = metrics.expanded_state_count;
            max_state_candidate = candidate.candidate_id;
        }
        if elapsed.as_millis() >= 10 || metrics.memo_saturation_skips > 0 {
            eprintln!(
                "4L slow candidate index={} id={} elapsed_ms={} status={} variants={} states={} probes={} hits={} insertions={} saturation={} max_probe={}",
                index,
                candidate.candidate_id,
                elapsed.as_millis(),
                outcome.status,
                outcome.buffer.total_variant_count,
                metrics.expanded_state_count,
                metrics.memo_probes,
                metrics.memo_hits,
                metrics.memo_insertions,
                metrics.memo_saturation_skips,
                metrics.memo_max_probe_length,
            );
        }
        if (index + 1) % 100 == 0 {
            eprintln!(
                "4L progress candidates={} elapsed_ms={} total_states={}",
                index + 1,
                profile_started.elapsed().as_millis(),
                total_states,
            );
        }
    }

    eprintln!(
        "4L BuildUp profile candidates={} elapsed_ms={} total_states={} max_states={} max_state_candidate={}",
        packing.candidate_count(),
        profile_started.elapsed().as_millis(),
        total_states,
        max_states,
        max_state_candidate,
    );
}

#[cfg(feature = "native-c-core")]
#[test]
#[ignore = "manual 6L packing resource profile"]
fn profile_opening_6l_packing_resources() {
    let query = OpeningPcSearchQuery::new(PcTarget::six_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
    #[cfg(feature = "search-stage-profiling")]
    let native_profile = clearra_core_ffi::NativeSearchProfileSession::start()
        .expect("native stage profiling must be enabled");
    let started = std::time::Instant::now();
    let packing = PackingRunner::run(&problem).expect("6L packing");
    eprintln!(
        "6L packing elapsed_ms={} candidates={} complete={} reason={:?} peak_frontier={} peak_candidates={} peak_hash_buckets={} peak_cpu_bytes={} retained_candidate_bytes={} candidate_metadata_bytes={} operation_reference_bytes={} operation_dictionary_entries={} operation_references={}",
        started.elapsed().as_millis(),
        packing.candidate_count(),
        packing.count_complete(),
        packing.truncation_reason(),
        packing.resource_report().peak_frontier_states,
        packing.resource_report().peak_candidate_rows,
        packing.resource_report().peak_hash_buckets,
        packing.resource_report().peak_cpu_bytes,
        packing.memory_report().retained_candidate_bytes(),
        packing.retained_candidate_metadata_bytes(),
        packing.retained_operation_reference_bytes(),
        packing.retained_operation_dictionary_entries(),
        packing.retained_operation_references(),
    );
    #[cfg(feature = "search-stage-profiling")]
    for stage in native_profile
        .finish()
        .into_iter()
        .filter(|stage| stage.invocation_count != 0 || stage.work_item_count != 0)
    {
        eprintln!(
            "6L stage={} duration_us={} invocations={} work_items={}",
            stage.name,
            stage.duration_ns / 1_000,
            stage.invocation_count,
            stage.work_item_count,
        );
    }
}

#[cfg(feature = "native-c-core")]
#[test]
fn setup_preset_builds_c_packing_candidate_buffer() {
    let query = clearra_problem::query::SetupSearchQuery::default()
        .with_queue(clearra_problem::query::SetupQueueInput::fixed_sequence(
            FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ]),
        ))
        .with_piece_budget(clearra_problem::query::PieceBudget::standard_7_bag(5));
    let problem = ProblemCompiler::compile_setup(&query).expect("setup problem");

    let result = PackingRunner::run(&problem).expect("packing");

    assert_eq!(
        result.compact_problem().problem_kind,
        CPackingProblem::SETUP
    );
    assert_eq!(
        result.execution_source(),
        PackingExecutionSource::NativeCpuPacking
    );
    assert_eq!(
        usize::from(result.compact_problem().piece_multiset_window.total_count),
        5
    );
    assert!(result.candidate_count() > 0);
    if let Some(candidate) = result.candidate_at(0) {
        assert!(candidate.operation_count > 0);
    }
}

#[cfg(feature = "native-c-core")]
#[test]
fn build_preset_builds_c_packing_candidate_buffer() {
    let query = clearra_problem::query::BuildQuery::coverage_bridge(
        clearra_problem::query::BuildTemplateBridge::new(
            "template-a",
            clearra_core_domain::board::board_size::BoardSize::new(10, 4).expect("board"),
            3,
        ),
        4,
        clearra_problem::query::BuildProblemLimits::new(12, 4),
    );
    let problem = ProblemCompiler::compile_build(&query).expect("build problem");

    let result = PackingRunner::run(&problem).expect("packing");

    assert_eq!(
        result.compact_problem().problem_kind,
        CPackingProblem::BUILD
    );
    assert_eq!(
        result.execution_source(),
        PackingExecutionSource::NativeCpuPacking
    );
    assert!(usize::from(result.compact_problem().piece_multiset_window.total_count) >= 3);
    assert_eq!(result.candidate_count(), 0);
    if let Some(candidate) = result.candidate_at(0) {
        assert_eq!(candidate.operation_count, 3);
    }
}

#[cfg(feature = "native-c-core")]
#[test]
fn verified_imported_kick_profile_reaches_c_packing_descriptor() {
    let verified = verified_imported_srs_x_profile();
    let query = opening_query_with_rule(srs_x(), Some(verified));
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

    let result = PackingRunner::run(&problem).expect("packing");

    assert_eq!(result.compact_problem().rule.has_verified_kick_profile, 1);
    assert_eq!(result.compact_problem().rule.verified_transition_count, 80);
}

#[test]
fn unverified_extension_rule_is_rejected_before_candidate_generation() {
    let query = opening_query_with_rule(srs_x(), None);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

    let result = PackingRunner::run(&problem);

    assert_eq!(
        result,
        Err(PackingRunnerError::Ffi(
            FfiProblemError::UnverifiedRuleProfileRejected {
                rule_profile_id: clearra_core_ffi::problem::C_RULE_SRS_X
            }
        ))
    );
}

#[cfg(feature = "native-c-core")]
#[test]
fn unavailable_gpu_report_does_not_claim_strengthening_execution() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

    let result = PackingRunner::run(&problem).expect("packing");
    let gpu = result.gpu_packing_report();

    assert!(!gpu.larger_batch_planner());
    assert!(!gpu.dominance_prefilter());
    assert!(!gpu.shape_union_mask());
    assert_eq!(gpu.candidate_hash(), "not-computed");
    assert!(!gpu.readback_compression());
    assert!(!gpu.cpu_exact_confirm_optimized());
    assert!(!gpu.deterministic_result());
    assert!(!gpu.cpu_reference_confirmed());
    assert!(!gpu.cpu_reference_match());
    assert_eq!(gpu.unavailable_reason(), "native_gpu_backend_not_built");
}

#[cfg(not(feature = "native-c-core"))]
#[test]
fn default_build_does_not_synthesize_portable_packing_results() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

    assert_eq!(
        PackingRunner::run(&problem),
        Err(PackingRunnerError::BackendExecutorUnavailable {
            backend: SelectedSearchBackend::CpuGeometryExactCover,
            reason: "native_geometry_exact_cover_not_connected",
        })
    );
}

#[derive(Clone, Copy)]
struct FixedGpuCapabilityProvider {
    capability: GpuSearchCapability,
}

impl SearchBackendCapabilityProvider for FixedGpuCapabilityProvider {
    fn gpu_capability(
        &self,
        _device: GpuDeviceSelection,
    ) -> Result<GpuSearchCapability, CapabilityQueryError> {
        Ok(self.capability)
    }

    fn prepared_gpu_capability(
        &self,
        _device: GpuDeviceSelection,
    ) -> Result<GpuSearchCapability, CapabilityQueryError> {
        Ok(self.capability)
    }
}

struct SuccessfulPackingExecutor {
    actual_backend: SelectedSearchBackend,
    candidate_id: u64,
    trust_override: Option<BackendTrustReport>,
    calls: Cell<usize>,
}

impl SuccessfulPackingExecutor {
    fn new(actual_backend: SelectedSearchBackend, candidate_id: u64) -> Self {
        Self {
            actual_backend,
            candidate_id,
            trust_override: None,
            calls: Cell::new(0),
        }
    }

    fn with_trust(mut self, trust: BackendTrustReport) -> Self {
        self.trust_override = Some(trust);
        self
    }

    fn calls(&self) -> usize {
        self.calls.get()
    }
}

impl SearchBackendExecutor for SuccessfulPackingExecutor {
    fn execute_packing(
        &self,
        problem: &CPackingProblem,
        _policy: &PcExecutionPolicy,
        _cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
    ) -> Result<PackingBackendOutcome, PackingRunnerError> {
        self.calls.set(self.calls.get() + 1);
        let mut candidate = clearra_core_ffi::CPackingCandidate::default();
        candidate.candidate_id = self.candidate_id;
        let multiset = if problem.piece_multiset_family.count > 0 {
            problem.piece_multiset_family.members[0]
        } else {
            problem.piece_multiset_window
        };
        let mut operation_index = 0usize;
        for (piece, count) in multiset.counts.iter().copied().enumerate().skip(1) {
            for _ in 0..count {
                candidate.operations[operation_index].piece = piece as u8;
                candidate.operations[operation_index].operation_id = operation_index as u16;
                operation_index += 1;
            }
        }
        candidate.operation_count = operation_index as u16;
        let trust = self.trust_override.unwrap_or_else(|| {
            if matches!(
                self.actual_backend,
                SelectedSearchBackend::Gpu | SelectedSearchBackend::Hybrid
            ) {
                BackendTrustReport::gpu_cpu_confirmed(true)
            } else {
                BackendTrustReport::cpu_exact()
            }
        });
        Ok(PackingBackendOutcome::exact(
            self.actual_backend,
            clearra_core_ffi::PackingCandidateBatch::from_candidates(
                problem.board.width,
                if problem.board.search_height == 0 {
                    problem.board.visible_height
                } else {
                    problem.board.search_height
                },
                [candidate],
            )
            .expect("test candidate batch"),
            ResourceReport::complete(),
            trust,
        ))
    }
}

struct FailingGpuExecutor {
    failure: GpuExecutionFailure,
    calls: Cell<usize>,
}

impl FailingGpuExecutor {
    fn new(failure: GpuExecutionFailure) -> Self {
        Self {
            failure,
            calls: Cell::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.get()
    }
}

impl SearchBackendExecutor for FailingGpuExecutor {
    fn execute_packing(
        &self,
        _problem: &CPackingProblem,
        _policy: &PcExecutionPolicy,
        _cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
    ) -> Result<PackingBackendOutcome, PackingRunnerError> {
        self.calls.set(self.calls.get() + 1);
        Err(PackingRunnerError::GpuExecution(self.failure))
    }
}

struct TestExecutorResolver<'a> {
    cpu: &'a dyn SearchBackendExecutor,
    gpu: &'a dyn SearchBackendExecutor,
}

impl SearchBackendExecutorResolver for TestExecutorResolver<'_> {
    fn executor_for(&self, backend: SelectedSearchBackend) -> Option<&dyn SearchBackendExecutor> {
        match backend {
            SelectedSearchBackend::None => None,
            SelectedSearchBackend::CpuGeometryExactCover
            | SelectedSearchBackend::CpuParallelGeometryExactCover => Some(self.cpu),
            SelectedSearchBackend::Gpu => Some(self.gpu),
            SelectedSearchBackend::Hybrid => None,
        }
    }

    fn cpu_fallback_executor(&self) -> &dyn SearchBackendExecutor {
        self.cpu
    }
}

#[test]
fn gpu_available_selection_executes_gpu_executor() {
    let problem = opening_problem_with_backend(RequestedSearchBackend::Gpu, false);
    let provider = FixedGpuCapabilityProvider {
        capability: GpuSearchCapability::available(2),
    };
    let cpu = SuccessfulPackingExecutor::new(SelectedSearchBackend::CpuGeometryExactCover, 1);
    let gpu = SuccessfulPackingExecutor::new(SelectedSearchBackend::Gpu, 2);
    let resolver = TestExecutorResolver {
        cpu: &cpu,
        gpu: &gpu,
    };

    let result = PackingRunner::run_with_components(&problem, &provider, &resolver)
        .expect("GPU executor outcome");

    assert_eq!(gpu.calls(), 1);
    assert_eq!(cpu.calls(), 0);
    assert_eq!(result.actual_backend(), SelectedSearchBackend::Gpu);
    assert_eq!(
        result.backend_report().selected_backend(),
        result.actual_backend()
    );
    assert_eq!(result.candidate_at(0).expect("candidate").candidate_id, 1);
    assert_eq!(
        result.execution_source(),
        PackingExecutionSource::NativeGpuPacking
    );
    assert!(result.gpu_packing_report().available());
    assert!(result.gpu_packing_report().cpu_reference_confirmed());
    assert!(!result.hybrid_scheduler_report().enabled());
}

#[test]
fn gpu_unavailable_selection_executes_cpu_with_fallback_reason() {
    let problem = opening_problem_with_backend(RequestedSearchBackend::Gpu, true);
    let provider = FixedGpuCapabilityProvider {
        capability: GpuSearchCapability::unavailable(GpuUnavailableReason::DeviceNotFound),
    };
    let cpu = SuccessfulPackingExecutor::new(SelectedSearchBackend::CpuGeometryExactCover, 11);
    let gpu = SuccessfulPackingExecutor::new(SelectedSearchBackend::Gpu, 12);
    let resolver = TestExecutorResolver {
        cpu: &cpu,
        gpu: &gpu,
    };

    let result = PackingRunner::run_with_components(&problem, &provider, &resolver)
        .expect("CPU fallback outcome");

    assert_eq!(cpu.calls(), 1);
    assert_eq!(gpu.calls(), 0);
    assert_eq!(
        result.actual_backend(),
        SelectedSearchBackend::CpuGeometryExactCover
    );
    assert_eq!(
        result.backend_report().selected_backend(),
        result.actual_backend()
    );
    assert_eq!(
        result.backend_report().fallback_reason(),
        Some(crate::backend::SearchBackendFallbackReason::GpuDeviceNotFound)
    );
    assert_eq!(result.candidate_at(0).expect("candidate").candidate_id, 1);
}

#[test]
fn gpu_unavailable_without_fallback_returns_error_before_execution() {
    let problem = opening_problem_with_backend(RequestedSearchBackend::Gpu, false);
    let provider = FixedGpuCapabilityProvider {
        capability: GpuSearchCapability::unavailable(GpuUnavailableReason::KernelUnavailable),
    };
    let cpu = SuccessfulPackingExecutor::new(SelectedSearchBackend::CpuGeometryExactCover, 21);
    let gpu = SuccessfulPackingExecutor::new(SelectedSearchBackend::Gpu, 22);
    let resolver = TestExecutorResolver {
        cpu: &cpu,
        gpu: &gpu,
    };

    let result = PackingRunner::run_with_components(&problem, &provider, &resolver);

    assert!(matches!(
        result,
        Err(PackingRunnerError::Backend(
            crate::backend::BackendSelectionError::GpuUnavailable(
                GpuUnavailableReason::KernelUnavailable
            )
        ))
    ));
    assert_eq!(cpu.calls(), 0);
    assert_eq!(gpu.calls(), 0);
}

#[test]
fn gpu_trust_mismatch_is_rejected_without_cpu_success_masking() {
    let problem = opening_problem_with_backend(RequestedSearchBackend::Gpu, true);
    let provider = FixedGpuCapabilityProvider {
        capability: GpuSearchCapability::available(0),
    };
    let cpu = SuccessfulPackingExecutor::new(SelectedSearchBackend::CpuGeometryExactCover, 31);
    let gpu = FailingGpuExecutor::new(GpuExecutionFailure::trust_mismatch(
        GpuExecutionFailureStage::CpuReferenceConfirm,
    ));
    let resolver = TestExecutorResolver {
        cpu: &cpu,
        gpu: &gpu,
    };

    let result = PackingRunner::run_with_components(&problem, &provider, &resolver);

    assert!(matches!(
        result,
        Err(PackingRunnerError::GpuExecutionRejected(resolution))
            if resolution.class() == GpuExecutionFailureClass::TrustMismatch
    ));
    assert_eq!(gpu.calls(), 1);
    assert_eq!(cpu.calls(), 0);
}

#[test]
fn gpu_success_outcome_requires_gpu_confirmed_trust() {
    let problem = opening_problem_with_backend(RequestedSearchBackend::Gpu, true);
    let provider = FixedGpuCapabilityProvider {
        capability: GpuSearchCapability::available(0),
    };
    let cpu = SuccessfulPackingExecutor::new(SelectedSearchBackend::CpuGeometryExactCover, 35);
    let gpu = SuccessfulPackingExecutor::new(SelectedSearchBackend::Gpu, 36)
        .with_trust(BackendTrustReport::cpu_exact());
    let resolver = TestExecutorResolver {
        cpu: &cpu,
        gpu: &gpu,
    };

    let result = PackingRunner::run_with_components(&problem, &provider, &resolver);

    assert!(matches!(
        result,
        Err(PackingRunnerError::BackendTrustMismatch {
            backend: SelectedSearchBackend::Gpu,
            ..
        })
    ));
    assert_eq!(gpu.calls(), 1);
    assert_eq!(cpu.calls(), 0);
}

#[test]
fn gpu_resource_incomplete_discards_partial_and_uses_only_cpu_rerun() {
    let problem = opening_problem_with_backend(RequestedSearchBackend::Gpu, true);
    let provider = FixedGpuCapabilityProvider {
        capability: GpuSearchCapability::available(0),
    };
    let cpu = SuccessfulPackingExecutor::new(SelectedSearchBackend::CpuGeometryExactCover, 41);
    let gpu = FailingGpuExecutor::new(
        GpuExecutionFailure::resource_incomplete(
            GpuExecutionFailureStage::Readback,
            GpuPartialResultDisposition::RetainedIncomplete,
        )
        .expect("resource failure"),
    );
    let resolver = TestExecutorResolver {
        cpu: &cpu,
        gpu: &gpu,
    };

    let result =
        PackingRunner::run_with_components(&problem, &provider, &resolver).expect("CPU full rerun");

    assert_eq!(gpu.calls(), 1);
    assert_eq!(cpu.calls(), 1);
    assert_eq!(result.candidate_count(), 1);
    assert_eq!(result.candidate_at(0).expect("candidate").candidate_id, 1);
    assert_eq!(
        result.actual_backend(),
        SelectedSearchBackend::CpuGeometryExactCover
    );
    let failure = result.backend_report().gpu_failure().expect("GPU failure");
    assert_eq!(
        failure.class(),
        GpuExecutionFailureClass::ResourceIncomplete
    );
    assert!(failure.discarded_partial_gpu_result());
    assert!(failure.original_gpu_result_incomplete());
}

#[test]
fn gpu_transient_before_commit_without_partial_uses_clean_cpu_fallback() {
    let problem = opening_problem_with_backend(RequestedSearchBackend::Gpu, true);
    let provider = FixedGpuCapabilityProvider {
        capability: GpuSearchCapability::available(0),
    };
    let cpu = SuccessfulPackingExecutor::new(SelectedSearchBackend::CpuGeometryExactCover, 51);
    let gpu = FailingGpuExecutor::new(
        GpuExecutionFailure::transient_before_commit(
            GpuExecutionFailureStage::Submission,
            GpuPartialResultDisposition::NotProduced,
        )
        .expect("pre-commit transient failure"),
    );
    let resolver = TestExecutorResolver {
        cpu: &cpu,
        gpu: &gpu,
    };

    let result = PackingRunner::run_with_components(&problem, &provider, &resolver)
        .expect("clean CPU fallback");

    assert_eq!(gpu.calls(), 1);
    assert_eq!(cpu.calls(), 1);
    assert_eq!(
        result.actual_backend(),
        SelectedSearchBackend::CpuGeometryExactCover
    );
    assert_eq!(result.candidate_at(0).expect("candidate").candidate_id, 1);
    let failure = result.backend_report().gpu_failure().expect("GPU failure");
    assert_eq!(
        failure.class(),
        GpuExecutionFailureClass::TransientBeforeCommit
    );
    assert!(!failure.discarded_partial_gpu_result());
}

#[test]
fn gpu_transient_with_retained_partial_is_not_combined_with_cpu() {
    let problem = opening_problem_with_backend(RequestedSearchBackend::Gpu, true);
    let provider = FixedGpuCapabilityProvider {
        capability: GpuSearchCapability::available(0),
    };
    let cpu = SuccessfulPackingExecutor::new(SelectedSearchBackend::CpuGeometryExactCover, 61);
    let gpu = FailingGpuExecutor::new(
        GpuExecutionFailure::transient_before_commit(
            GpuExecutionFailureStage::Readback,
            GpuPartialResultDisposition::RetainedIncomplete,
        )
        .expect("pre-commit transient failure"),
    );
    let resolver = TestExecutorResolver {
        cpu: &cpu,
        gpu: &gpu,
    };

    let result = PackingRunner::run_with_components(&problem, &provider, &resolver);

    assert!(matches!(
        result,
        Err(PackingRunnerError::GpuExecutionRejected(resolution))
            if resolution.class() == GpuExecutionFailureClass::TransientBeforeCommit
    ));
    assert_eq!(gpu.calls(), 1);
    assert_eq!(cpu.calls(), 0);
}

fn opening_problem_with_backend(
    backend: RequestedSearchBackend,
    allow_fallback: bool,
) -> clearra_problem::SearchProblem {
    let execution_policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(backend)
        .with_allow_backend_fallback(allow_fallback);
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled)
        .with_execution_policy(execution_policy);
    ProblemCompiler::compile_opening_pc(&query).expect("problem")
}

fn opening_query_with_rule(
    rule: RuleProfile,
    verified_profile: Option<VerifiedKickTableProfile>,
) -> OpeningPcSearchQuery {
    let mut query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled)
        .with_rule(rule);
    if let Some(profile) = verified_profile {
        query = query.with_verified_kick_table_profile(profile);
    }
    query
}

fn verified_imported_srs_x_profile() -> VerifiedKickTableProfile {
    VerifiedKickTableProfile::try_new(KickTableProfile::new(
        KickTableProfileId::Imported,
        RuleProfileId::SrsX,
        SrsKicks::srs_plus_profile().entries().to_vec(),
    ))
    .expect("verified imported profile")
}
