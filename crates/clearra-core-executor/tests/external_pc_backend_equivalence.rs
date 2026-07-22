#![cfg(feature = "native-c-core")]

use std::{collections::BTreeSet, fs, path::PathBuf};

use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{NormalizedTilingSolutionKey, PiecePlacementMask},
};
use clearra_core_executor::{CoreExecutionResult, PcService};
use clearra_fumen::SourceFumenColoredFieldSet;
use clearra_geometry::{
    canonical::mirror_transform::MirrorTransform, layout::board64_layout::Board64Layout,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::{builtin_rules::srs_plus, rule_profile::RuleProfile};
#[path = "external_pc_backend_equivalence/support.rs"]
mod external_pc_backend_equivalence_support;
use external_pc_backend_equivalence_support::*;

#[test]
fn pco_scenario_uses_user_confirmed_board_hold_and_piece_window() {
    let query =
        external_pc_scenario_query(ExternalPcCase::PcoIHold, RequestedSearchBackend::Cpu, true);

    assert_eq!(query.initial_board().occupied_mask(), 0x0000_00e0_f87e_3f87);
    assert_ne!(query.initial_board().occupied_mask(), 0x3f0);
    assert_eq!(query.hold_state().piece(), Some(PieceKind::I));
    assert_eq!(query.piece_window().max_pieces(), 4);
    assert_eq!(query.exact_pieces(), Some(4));
}

#[cfg(feature = "native-c-core")]
#[test]
fn pco_i_hold_cpu_matches_full_63_solution_set() {
    let source_set = pco_full_63_solution_set();
    let result = run_external_case(ExternalPcCase::PcoIHold, RequestedSearchBackend::Cpu, true)
        .expect("PCO CPU run");

    assert_matches_source_colored_field_set(&result, &source_set);
    assert_eq!(result.actual_normalized_unique_solution_count, 63);
    assert!(result.count_complete);
}

#[cfg(feature = "native-c-core")]
#[test]
fn pco_mirror_preserves_complete_normalized_tiling_set() {
    let original = run_external_case(ExternalPcCase::PcoIHold, RequestedSearchBackend::Cpu, true)
        .expect("PCO original CPU run");
    let mirrored = run_external_case(
        ExternalPcCase::PcoIHoldMirror,
        RequestedSearchBackend::Cpu,
        true,
    )
    .expect("PCO mirrored CPU run");

    assert_mirrored_solution_sets(&original, &mirrored, 4);
}

#[cfg(feature = "native-c-core")]
#[test]
fn tsar_cannon_cpu_matches_full_42_solution_set() {
    assert_fixture_marker(
        TSAR_FIXTURE,
        "\"fixture_id\": \"tsar_cannon_after_2bag_full_42\"",
    );
    let source_set = tsar_full_42_solution_set();
    let cpu = run_external_case(
        ExternalPcCase::TsarCannonFull42,
        RequestedSearchBackend::Cpu,
        true,
    )
    .expect("Tsar CPU run");

    assert_matches_source_solution_set(&cpu, &source_set);
    assert!(cpu.final_board_empty);
    assert!(cpu.count_complete);
}

#[cfg(feature = "native-c-core")]
#[test]
fn tsar_mirror_preserves_complete_normalized_tiling_set() {
    let original = run_external_case(
        ExternalPcCase::TsarCannonFull42,
        RequestedSearchBackend::Cpu,
        true,
    )
    .expect("Tsar original CPU run");
    let mirrored = run_external_case(
        ExternalPcCase::TsarCannonFull42Mirror,
        RequestedSearchBackend::Cpu,
        true,
    )
    .expect("Tsar mirrored CPU run");

    assert_eq!(original.actual_normalized_unique_solution_count, 42);
    assert_mirrored_solution_sets(&original, &mirrored, 5);
}

#[cfg(feature = "native-c-core")]
#[test]
fn tsar_gpu_request_matches_the_cpu_solution_set_with_trust_or_fallback() {
    let source_set = tsar_full_42_solution_set();
    let cpu = run_external_case(
        ExternalPcCase::TsarCannonFull42,
        RequestedSearchBackend::Cpu,
        true,
    )
    .expect("Tsar CPU run");
    let gpu = run_external_case(
        ExternalPcCase::TsarCannonFull42,
        RequestedSearchBackend::Gpu,
        true,
    )
    .expect("Tsar GPU request");

    assert_matches_source_solution_set(&gpu, &source_set);
    assert_equivalent_result_contract(&cpu, &gpu);
    assert_gpu_execution_or_explicit_fallback(&gpu, RequestedSearchBackend::Gpu);
}

#[cfg(feature = "native-c-core")]
#[test]
fn tsar_hybrid_request_matches_the_cpu_solution_set_with_trust_or_fallback() {
    let source_set = tsar_full_42_solution_set();
    let cpu = run_external_case(
        ExternalPcCase::TsarCannonFull42,
        RequestedSearchBackend::Cpu,
        true,
    )
    .expect("Tsar CPU run");
    let hybrid = run_external_case(
        ExternalPcCase::TsarCannonFull42,
        RequestedSearchBackend::Hybrid,
        true,
    )
    .expect("Tsar hybrid request");

    assert_matches_source_solution_set(&hybrid, &source_set);
    assert_equivalent_result_contract(&cpu, &hybrid);
    assert_gpu_execution_or_explicit_fallback(&hybrid, RequestedSearchBackend::Hybrid);
}

#[cfg(feature = "native-c-core")]
#[test]
fn tsar_gpu_unavailable_without_fallback_returns_error() {
    let result = run_external_case(
        ExternalPcCase::TsarCannonFull42,
        RequestedSearchBackend::Gpu,
        false,
    );

    #[cfg(feature = "webgpu-search")]
    if pollster::block_on(clearra_webgpu::WebGpuGeometryExactCoverBackend::adapter_available()) {
        let evidence = result.expect("a connected WebGPU backend must execute without fallback");
        assert_connected_gpu_execution(&evidence, RequestedSearchBackend::Gpu);
        return;
    }

    assert!(
        result.is_err(),
        "an unavailable GPU backend must not silently fall back"
    );
}

#[cfg(feature = "native-c-core")]
#[test]
fn tsar_backend_runs_release_native_memory() {
    for backend in [
        RequestedSearchBackend::Cpu,
        RequestedSearchBackend::Gpu,
        RequestedSearchBackend::Hybrid,
    ] {
        let evidence = run_external_case(ExternalPcCase::TsarCannonFull42, backend, true)
            .expect("backend run");
        assert!(
            evidence.memory_leak_report_clean,
            "{backend:?} memory leak report should be clean"
        );
    }
}

#[cfg(feature = "search-stage-profiling")]
#[test]
#[ignore = "manual exact external-PC stage profile"]
fn profile_pco_and_tsar_search_stages() {
    for case in [ExternalPcCase::PcoIHold, ExternalPcCase::TsarCannonFull42] {
        let native_profile = clearra_core_ffi::NativeSearchProfileSession::start()
            .expect("stage profiling must be enabled in the linked C core");
        let executor_profile = clearra_core_executor::ExecutorSearchProfileSession::start()
            .expect("executor stage profiling must not already be active");
        let started = std::time::Instant::now();
        let result = run_external_case(case, RequestedSearchBackend::Cpu, true)
            .expect("external PC profile run");
        let elapsed = started.elapsed();
        let executor_stages = executor_profile.finish();
        let native_stages = native_profile.finish();

        eprintln!(
            "external_pc={case:?} elapsed_ms={} solutions={}",
            elapsed.as_millis(),
            result.actual_normalized_unique_solution_count,
        );
        for stage in executor_stages
            .into_iter()
            .filter(|stage| stage.invocation_count != 0 || stage.work_item_count != 0)
        {
            eprintln!(
                "stage={} duration_us={} invocations={} work_items={}",
                stage.name,
                stage.duration_ns / 1_000,
                stage.invocation_count,
                stage.work_item_count,
            );
        }
        for stage in native_stages
            .into_iter()
            .filter(|stage| stage.invocation_count != 0 || stage.work_item_count != 0)
        {
            eprintln!(
                "stage={} duration_us={} invocations={} work_items={}",
                stage.name,
                stage.duration_ns / 1_000,
                stage.invocation_count,
                stage.work_item_count,
            );
        }
    }
}
