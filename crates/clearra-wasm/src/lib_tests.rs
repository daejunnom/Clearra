// SRP rationale: this test module has one behavior-level change reason: verifying the complete public WASM command and JSON envelope contract.

use clearra_app::{
    decode_ctk3_exact, encode_ctk3_compact, AppCommand, AppContext, AppCoreExecutorService,
    AppRenderModel, AppResponse, AppServices, Ctk3Color, Ctk3Document, Ctk3Operation, Ctk3Page,
    Ctk3PageFlags, Ctk3Piece, Ctk3Rotation, DistributedSearchPreparation, FinesseReport,
};
use clearra_core_domain::resource::{
    ExecutionAvailabilityReason, ExecutionAvailabilityState as CoreExecutionAvailabilityState,
};
use clearra_core_executor::{
    CoreExecutionResult, WasmBuildProbabilityCandidateProducer, WasmCpuSearchError,
    WasmCpuSearchSession,
};
use clearra_host_contract::{AppStatus, ProductResultPayloadContent};
use clearra_host_contract::{
    ExecutionAvailabilityState as HostExecutionAvailabilityState, ExecutionCompletenessState,
};
use clearra_problem::ProblemCompiler;
use serde_json::Value;
use std::sync::{Mutex, MutexGuard};

use crate::wasm_command_runtime::solution_page_store_is_public;

use super::*;

#[test]
fn wasm_sequence_dependencies_exposes_exact_decimal_report() {
    let mut page = Ctk3Page::new(0, Vec::new());
    page.flags = Ctk3PageFlags::default();
    page.operation = Some(Ctk3Operation {
        piece: Ctk3Piece::O,
        rotation: Ctk3Rotation::Spawn,
        x: 0,
        y: 0,
    });
    let document =
        encode_ctk3_compact(&Ctk3Document::new(10, vec![page])).expect("one-operation CTK3");
    let result = WasmCommandRuntime::default()
        .run_command_text(&format!(
            "clearra utility sequence-dependencies --document {document} --rule-profile srs-plus --kick-profile srs-plus --timeout-seconds 900"
        ))
        .expect("WASM sequence-dependencies execution");
    assert_eq!(result.app_response().status(), AppStatus::Success);
    let report = result
        .search_report()
        .expect("typed WASM dependency report");
    assert_eq!(report.backend_selected, "wasm-cpu-sequence-dependencies");
    assert!(report
        .summary_fields
        .iter()
        .any(|(key, value)| { key == "contract_id" && value == "operation-dependency-report.v1" }));
    assert!(report
        .summary_fields
        .iter()
        .any(|(key, value)| key == "exact_order_count" && value == "1"));
}

#[test]
fn wasm_sequence_exposes_normalized_replay_report() {
    let mut page = Ctk3Page::new(0, Vec::new());
    page.flags = Ctk3PageFlags::default();
    page.operation = Some(Ctk3Operation {
        piece: Ctk3Piece::O,
        rotation: Ctk3Rotation::Spawn,
        x: 0,
        y: 0,
    });
    let document =
        encode_ctk3_compact(&Ctk3Document::new(10, vec![page])).expect("one-operation CTK3");
    let result = WasmCommandRuntime::default()
        .run_command_text(&format!(
            "clearra utility sequence --document {document} --rule-profile srs-plus --kick-profile srs-plus --timeout-seconds 900"
        ))
        .expect("WASM sequence execution");
    assert_eq!(result.app_response().status(), AppStatus::Success);
    let report = result.search_report().expect("typed WASM sequence report");
    assert_eq!(report.backend_selected, "wasm-cpu-operation-sequence");
    assert!(report
        .summary_fields
        .iter()
        .any(|(key, value)| key == "contract_id" && value == "operation-sequence.v1"));
    assert!(report
        .summary_fields
        .iter()
        .any(|(key, value)| key == "normalized_trace" && value == "0:O:0:0:0"));
}

#[test]
fn wasm_typed_field_document_transforms_share_app_authority() {
    let mut page = Ctk3Page::new(
        1,
        vec![
            Ctk3Color::Piece(Ctk3Piece::J),
            Ctk3Color::Empty,
            Ctk3Color::Piece(Ctk3Piece::S),
            Ctk3Color::Gray,
        ],
    );
    page.comment = "WASM identity".to_owned();
    let source = encode_ctk3_compact(&Ctk3Document::new(4, vec![page])).expect("CTK3");
    let runtime = WasmCommandRuntime::default();

    let gray = runtime
        .run_command_text(&format!(
            "clearra utility to-gray --format ctk3 --document {source}"
        ))
        .expect("WASM to-gray execution");
    assert_eq!(gray.app_response().status(), AppStatus::Success);
    let ProductResultPayloadContent::FieldDocument(gray_payload) = gray
        .app_response()
        .product_result_payload()
        .expect("to-gray payload")
        .content()
    else {
        panic!("expected field-document payload")
    };
    assert_eq!(gray_payload.filename(), "clearra-to-gray.ctk3");
    let decoded = decode_ctk3_exact(gray_payload.document()).expect("gray CTK3");
    assert!(decoded.pages[0]
        .cells
        .iter()
        .all(|cell| matches!(cell, Ctk3Color::Empty | Ctk3Color::Gray)));

    let once = runtime
        .run_command_text(&format!(
            "clearra utility mirror --format ctk3 --document {source}"
        ))
        .expect("WASM mirror execution");
    let ProductResultPayloadContent::FieldDocument(once_payload) = once
        .app_response()
        .product_result_payload()
        .expect("mirror payload")
        .content()
    else {
        panic!("expected field-document payload")
    };
    let twice = runtime
        .run_command_text(&format!(
            "clearra utility mirror --format ctk3 --document {}",
            once_payload.document()
        ))
        .expect("second WASM mirror execution");
    let ProductResultPayloadContent::FieldDocument(twice_payload) = twice
        .app_response()
        .product_result_payload()
        .expect("second mirror payload")
        .content()
    else {
        panic!("expected field-document payload")
    };
    assert_eq!(
        decode_ctk3_exact(twice_payload.document()),
        decode_ctk3_exact(&source)
    );
}

#[test]
fn wasm_command_compiles_to_app_request() {
    let request = WasmCommandRuntime::default()
        .compile_command_text("clearra pc --lines 2 --backend cpu")
        .expect("AppRequest");

    match request.command() {
        AppCommand::Pc(command) => {
            assert_eq!(command.query().target().lines(), 2);
            assert_eq!(command.query().execution_policy().backend().as_str(), "cpu");
        }
        _ => panic!("expected pc command"),
    }

    let result = WasmCommandRuntime::default()
        .run_command_text("clearra pc --lines 2 --backend auto --queue IIOOO")
        .expect("general WASM CPU search backend");
    assert_eq!(
        result.app_response().status(),
        AppStatus::Success,
        "{:?}",
        result.app_response()
    );
    assert_eq!(
        result.app_response().backend_report().backend_selected(),
        "wasm-cpu"
    );
    assert!(result.app_response().resource_report().solver_executed());
    assert!(result
        .app_response()
        .resource_report()
        .probability_complete());
    assert_eq!(
        result
            .app_response()
            .resource_report()
            .execution_availability()
            .state(),
        HostExecutionAvailabilityState::Available
    );
    assert_eq!(
        result
            .app_response()
            .resource_report()
            .result_completeness(),
        ExecutionCompletenessState::Complete
    );
    assert_eq!(
        result.webgpu_backend().outcome_state,
        WebGpuBackendOutcomeState::Unavailable
    );
    assert!(!result.webgpu_backend().fallback_used);
    assert_eq!(result.webgpu_backend().fallback_backend, None);
    assert_eq!(
        result.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("webgpu_not_selected")
    );

    let request = WasmCommandRuntime::default()
        .compile_command_text("clearra pc --lines 2 --backend auto --queue IIOOO")
        .expect("AppRequest");
    let app_response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request);
    let core_result = app_response
        .render_model()
        .and_then(|model| model.core_result())
        .expect("WASM CPU CoreExecutionResult");
    assert!(core_result.solution_found());
    assert_eq!(core_result.field("count_complete"), Some("true"));
}

#[test]
fn wasm_command_profiles_are_request_local_and_fail_closed() {
    let runtime = WasmCommandRuntime::default();
    let request = runtime
        .compile_command_text(
            "clearra pc --lines 2 --backend cpu \
             --board-profile standard-10 \
             --piece-profile standard-tetrominoes \
             --bag-profile standard-7-bag \
             --rule srs-x --score-profile guideline --spin-profile all-mini-plus",
        )
        .expect("verified WASM request-local profiles");
    let profiles = request.request_profiles();
    assert_eq!(profiles.board().as_str(), "standard-10");
    assert_eq!(profiles.piece_set().as_str(), "standard-tetrominoes");
    assert_eq!(profiles.bag().as_str(), "standard-7-bag");
    assert_eq!(profiles.rule().as_str(), "srs-x");
    assert_eq!(profiles.spin().as_str(), "all-mini-plus");
    assert_eq!(profiles.score().as_str(), "guideline");

    for command in [
        "clearra pc --lines 2 --board-profile wide-10",
        "clearra pc --lines 2 --piece-profile pentominoes",
        "clearra pc --lines 2 --bag-profile history-6-rolls",
        "clearra pc --lines 2 --rule custom",
        "clearra pc --lines 2 --spin-profile unverified-spin",
        "clearra pc --lines 2 --score-profile classic-score",
    ] {
        assert!(
            runtime.compile_command_text(command).is_err(),
            "{command} must reject without fallback"
        );
    }
}

#[test]
fn unavailable_gpu_and_hybrid_keep_distinct_cpu_selection_semantics() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));

    let gpu = runtime
        .run_command_text(
            "clearra pc --lines 2 --backend gpu --allow-backend-fallback \
             --workers 1 --queue IIOOO",
        )
        .expect("explicit GPU fallback result");
    assert_eq!(gpu.app_response().status(), AppStatus::Success);
    assert!(gpu.app_response().backend_report().fallback_used());
    assert_eq!(
        gpu.app_response()
            .backend_report()
            .backend_fallback_reason(),
        Some("gpu_device_not_found")
    );
    assert!(gpu.webgpu_backend().fallback_used);
    assert_eq!(
        gpu.webgpu_backend().fallback_backend.as_deref(),
        Some("wasm-cpu")
    );
    assert_eq!(
        gpu.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("gpu_device_not_found")
    );

    let hybrid = runtime
        .run_command_text(
            "clearra pc --lines 2 --backend hybrid --no-backend-fallback \
             --workers 1 --queue IIOOO",
        )
        .expect("hybrid CPU selection result");
    assert_eq!(hybrid.app_response().status(), AppStatus::Success);
    assert_eq!(
        hybrid.app_response().backend_report().backend_selected(),
        "wasm-cpu"
    );
    assert!(!hybrid.app_response().backend_report().fallback_used());
    assert!(!hybrid.webgpu_backend().fallback_used);
    assert_eq!(hybrid.webgpu_backend().fallback_backend, None);
    assert_eq!(
        hybrid.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("gpu_device_not_found")
    );
}

#[test]
fn canonical_pc_tiling_returns_exact_geometry_without_buildup_or_probability() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra pc tiling --lines 2 --queue IIOOO \
             --backend cpu --workers 1 --no-hold",
        )
        .expect("canonical pc tiling search");
    let report = result.search_report().expect("canonical pc tiling report");

    assert!(report.unique_solution_count > 0);
    assert!(!report.buildability_verified);
    assert!(!report.coverage_calculated);
    assert!(!report.probability_calculated);
    assert_eq!(report.coverage_probability, "not-calculated");
    assert_eq!(report.total_build_order_nodes, 0);
    assert_eq!(report.coverage_product_edge_checks, 0);
    assert!(report.count_complete);
    assert!(report.solution_count_calculated);
    assert!(report.solution_set_materialized);
    assert!(report.solution_keys_complete);
    assert_ne!(report.normalized_solution_set_hash, "not-calculated");
}

#[test]
fn browser_worker_final_event_keeps_the_pc_tiling_product_result_kind() {
    let mut runtime = WasmWorkerJobRuntime::default();
    let job_id = runtime
        .start_job(
            "clearra pc tiling --lines 2 --queue IIOOO \
             --backend cpu --workers 1 --no-hold",
        )
        .expect("browser pc tiling job");
    while !runtime
        .advance_job(job_id, 4096)
        .expect("advance browser pc tiling job")
        .is_terminal()
    {}

    let json = runtime
        .drain_events_json(job_id)
        .expect("browser pc tiling event JSON");
    let events: Value = serde_json::from_str(&json).expect("valid browser pc tiling events");
    let final_event = events
        .as_array()
        .and_then(|events| {
            events
                .iter()
                .find(|event| event["event"] == "final_response")
        })
        .expect("final browser pc tiling response");

    assert_eq!(final_event["response"]["status"], "success");
    assert_eq!(
        final_event["response"]["result"],
        serde_json::json!({"kind": "pc-tiling-family.v1"})
    );
    let search_report = final_event["search_report"]
        .as_object()
        .expect("browser pc tiling search report");
    let unique_solution_count = search_report["unique_solution_count"]
        .as_u64()
        .expect("browser pc tiling solution count");
    let normalized_solution_keys = search_report["normalized_solution_keys"]
        .as_array()
        .expect("browser pc tiling normalized solution keys");
    assert!(unique_solution_count > 0);
    assert_eq!(search_report["solution_count_calculated"], true);
    assert_eq!(search_report["solution_set_materialized"], true);
    assert_eq!(search_report["solution_keys_complete"], true);
    assert_eq!(
        search_report["solution_keys_materialized_count"].as_u64(),
        Some(unique_solution_count)
    );
    assert_eq!(normalized_solution_keys.len() as u64, unique_solution_count);
    assert!(normalized_solution_keys
        .iter()
        .all(|key| key.as_str().is_some_and(|key| key.starts_with("ctk1|"))));
    assert!(final_event["response"]
        .get("product_capability_result")
        .is_none());
    assert!(!json.contains("pc_tiling_memory_admission_evidence"));
}

#[test]
fn browser_worker_pc_save_products_keep_distinct_full_typed_families() {
    for (subcommand, payload_kind, result_kind) in [
        ("saves", "pc-save-groups", "pc-save-groups.v2"),
        ("best-save", "pc-best-save", "pc-best-save.v2"),
    ] {
        let mut runtime = WasmWorkerJobRuntime::default();
        let command = format!(
            "clearra pc {subcommand} --lines 2 --board-mask 0xf3fcf \
             --height 2 --pieces 1 --patterns P7 --no-hold --backend cpu"
        );
        let job_id = runtime.start_job(&command).expect("browser PC save job");
        let mut terminal = false;
        for _ in 0..256 {
            terminal = runtime
                .advance_job(job_id, 4096)
                .expect("advance browser PC save job")
                .is_terminal();
            if terminal {
                break;
            }
        }
        assert!(terminal, "tiny browser PC save command must finish");

        let json = runtime
            .drain_events_json(job_id)
            .expect("browser PC save event JSON");
        let events: Value = serde_json::from_str(&json).expect("valid browser PC save events");
        let final_event = events
            .as_array()
            .and_then(|events| {
                events
                    .iter()
                    .find(|event| event["event"] == "final_response")
            })
            .expect("final browser PC save response");
        let response = &final_event["response"];
        assert_eq!(response["status"], "success", "{command}: {json}");
        assert_eq!(response["result"]["kind"], result_kind);
        assert_eq!(
            response["product_result_payload"]["content"]["payload_kind"],
            payload_kind
        );
        let payload = &response["product_result_payload"]["content"]["payload"];
        assert_eq!(payload["metadata"]["completeness"]["complete"], true);
        assert!(payload["metadata"]["pc_probability"].is_string());

        if subcommand == "saves" {
            let groups = payload["groups"].as_array().expect("full save groups");
            assert_eq!(payload["group_count"], groups.len().to_string());
            assert_eq!(groups.len(), 1);
            assert!(groups[0]["canonical_candidate_id"].is_string());
            assert!(groups[0]["unconditional_probability"].is_string());
            assert!(groups[0]["conditional_probability_given_pc"].is_string());
        } else {
            let winners = payload["winners"]
                .as_array()
                .expect("ordinary full best-save winner list");
            assert_eq!(payload["winner_count"], winners.len().to_string());
            assert_eq!(winners.len(), 1);
            assert_eq!(payload["schema_id"], "clearra-save-v1");
            assert_eq!(payload["probability_basis"], "whole-universe-unconditional");
            assert_eq!(
                payload["canonical_selection"],
                "smallest-canonical-candidate-id"
            );
            assert_eq!(payload["canonical_winner"], winners[0]);
            let canonical_candidate_id = payload["canonical_winner"]["group"]
                ["canonical_candidate_id"]
                .as_str()
                .expect("canonical best-save candidate ID")
                .parse::<u64>()
                .expect("numeric canonical best-save candidate ID");
            assert!(winners
                .iter()
                .all(|winner| winner["group"]["canonical_candidate_id"]
                    .as_str()
                    .and_then(|value| value.parse::<u64>().ok())
                    .is_some_and(|candidate_id| candidate_id >= canonical_candidate_id)));
            assert!(winners[0]["group"]["canonical_candidate_id"].is_string());
            assert!(!json.contains("portfolio"));
            assert!(!json.contains("tie_cursor"));
            assert!(!json.contains("tie_metadata"));
        }
    }
}

#[test]
fn browser_worker_spin_structure_routes_keep_closed_payloads_and_live_cover_paging() {
    for (route, result_kind, payload_kind) in [
        (
            "search",
            "spin-structure-family.v2",
            "spin-structure-family",
        ),
        (
            "cover --objective min-cover --max-patterns 8",
            "spin-structure-coverage.v1",
            "coverage-portfolio",
        ),
        (
            "guaranteed --final-piece T --max-patterns 8 --dependency-report",
            "spin-structure-guaranteed.v1",
            "spin-structure-family",
        ),
    ] {
        let mut runtime = WasmWorkerJobRuntime::default();
        let command = format!(
            "clearra spin-structure {route} --board-mask 0x14000043ff --height 4 \
             --pieces T --spin-profile t-spins --lines any --fill-top 4 \
             --max-placements 1"
        );
        let job_id = runtime.start_job(&command).expect("browser spin job");
        let mut terminal = false;
        for _ in 0..256 {
            terminal = runtime
                .advance_job(job_id, 4096)
                .expect("advance browser spin job")
                .is_terminal();
            if terminal {
                break;
            }
        }
        assert!(terminal, "tiny browser spin command must finish: {command}");

        let json = runtime
            .drain_events_json(job_id)
            .expect("browser spin event JSON");
        let events: Value = serde_json::from_str(&json).expect("valid browser spin events");
        let response = events
            .as_array()
            .and_then(|events| {
                events
                    .iter()
                    .find(|event| event["event"] == "final_response")
            })
            .map(|event| &event["response"])
            .expect("final browser spin response");
        assert_eq!(response["status"], "success", "{command}: {json}");
        assert_eq!(response["result"]["kind"], result_kind, "{command}");
        assert_eq!(
            response["product_result_payload"]["content"]["payload_kind"], payload_kind,
            "{command}"
        );
        let payload = &response["product_result_payload"]["content"]["payload"];
        if payload_kind == "coverage-portfolio" {
            assert_eq!(payload["set_contract"], "portfolio-alternative-set.v1");
            assert_eq!(payload["alternative_index"], "1");
            assert!(payload["enumeration_complete"].is_boolean());
            assert_eq!(payload["page_handle_available"], true);
            assert!(payload["members"]
                .as_array()
                .is_some_and(|members| !members.is_empty()));
        } else if result_kind == "spin-structure-guaranteed.v1" {
            assert_eq!(payload["schema_id"], "spin-structure-guaranteed.v1");
            assert_eq!(payload["guaranteed_final_piece"], "T");
            assert_eq!(
                payload["guarantee_basis"],
                "every-unique-non-target-piece-order-exact-replay-final-piece-last"
            );
            assert_eq!(payload["dependency_report_included"], true);
            assert_eq!(
                payload["dependency_relation"],
                "non-target-universal-precedence"
            );
            assert_eq!(payload["dependency_edge_count"], "0");
        } else {
            assert_eq!(payload["schema_id"], "spin-structure-family.v2");
            assert!(payload["guaranteed_final_piece"].is_null());
            assert!(payload["dependency_report_included"].is_null());
        }

        let page_source = runtime.take_completed_product_page_source_owner();
        if payload_kind == "coverage-portfolio" {
            let mut store = ProductPageStore::from_source(
                page_source.expect("spin cover transfers its immutable page source"),
            )
            .expect("open spin cover page store");
            let coverage = store
                .coverage_portfolio_mut()
                .expect("spin cover uses the common coverage portfolio pager");
            assert_eq!(coverage.loaded_page_count(), 1);
            let mut enumeration_complete = false;
            for _ in 0..10_000 {
                let advance = coverage
                    .next_page(u64::MAX, &mut || false)
                    .expect("advance exact spin cover alternatives");
                enumeration_complete = advance.checkpoint().enumeration_complete();
                if enumeration_complete {
                    break;
                }
            }
            assert!(enumeration_complete, "finite exact tie paging must finish");
            assert!(coverage.loaded_page_count() >= 1);
        } else {
            assert!(page_source.is_none(), "ordinary spin families do not page");
        }
    }
}

#[test]
fn wasm_pc_score_minimals_returns_the_score_only_portfolio_and_live_page_owner() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra pc score-minimals --lines 1 --board-mask 0x3f --height 1 \
             --pieces 1 --queue I --hold empty --score-profile tetrio \
             --spin-profile t-spins --initial-b2b 0",
        )
        .expect("canonical WASM pc score-minimals search");

    assert_eq!(result.app_response().status(), AppStatus::Success);
    assert!(
        result.app_response().solution_set_artifact().is_none(),
        "WASM product completion must not eagerly encode the score-minimum portfolio"
    );
    assert!(result.product_page_source_owner().is_some());
    let payload = result
        .app_response()
        .product_result_payload()
        .expect("score-minimals product payload");
    assert_eq!(payload.contract(), "pc.score-minimals");
    assert_eq!(payload.result_kind(), "pc-score-portfolio.v2");
    let ProductResultPayloadContent::CoveragePortfolio(page) = payload.content() else {
        panic!("expected score-minimals coverage portfolio payload")
    };
    assert!(page.page_handle_available());
    assert_eq!(page.alternative_index(), "1");
    assert_eq!(page.member_page_number(), "1");
    assert!(!page.members().is_empty());
    assert_eq!(page.canonical_selection(), None);
    assert_eq!(page.canonical_witness(), None);
    assert!(page
        .members()
        .iter()
        .all(|member| !member.candidate_id().starts_with('0')));
}

#[test]
fn wasm_pc_minimals_returns_the_exact_portfolio_and_live_page_owner() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra pc minimals --lines 1 --board-mask 0x3f --height 1 \
             --pieces 1 --queue I --hold empty",
        )
        .expect("canonical WASM pc minimals search");

    assert_eq!(result.app_response().status(), AppStatus::Success);
    assert!(
        result.app_response().solution_set_artifact().is_none(),
        "WASM product completion must not eagerly encode the minimum-cover portfolio"
    );
    assert!(result.product_page_source_owner().is_some());
    let payload = result
        .app_response()
        .product_result_payload()
        .expect("pc minimals product payload");
    assert_eq!(payload.contract(), "pc.minimals");
    assert_eq!(payload.result_kind(), "pc-minimum-cover.v2");
    let ProductResultPayloadContent::CoveragePortfolio(page) = payload.content() else {
        panic!("expected pc minimals coverage portfolio payload")
    };
    assert!(page.page_handle_available());
    assert_eq!(page.alternative_index(), "1");
    assert_eq!(page.member_page_number(), "1");
    assert!(!page.members().is_empty());
    assert!(page
        .members()
        .iter()
        .all(|member| !member.candidate_id().starts_with('0')));
}

#[test]
fn canonical_minimals_preserves_the_legacy_count_all_coverage_identity_set() {
    let runtime = WasmCommandRuntime::default();
    // v0.7.4's GUI lowered minimum-cover through the generic count-all route.
    // The canonical command uses count-unique because path multiplicity is not
    // part of a coverage portfolio; the exact candidate identity/coverage set
    // must nevertheless remain identical.
    let legacy = runtime
        .run_command_text(
            "clearra pc --lines 2 --queue IIOOO --count all \
             --objective minimum-cover --backend cpu --workers 1",
        )
        .expect("legacy v0.7.4-style minimum-cover result");
    let canonical = runtime
        .run_command_text(
            "clearra pc minimals --lines 2 --queue IIOOO \
             --backend cpu --workers 1",
        )
        .expect("canonical minimum-cover result");
    let legacy_report = legacy.search_report().expect("legacy search report");
    let canonical_report = canonical.search_report().expect("canonical search report");

    assert_eq!(
        canonical_report.unique_solution_count,
        legacy_report.unique_solution_count
    );
    assert_eq!(
        canonical_report.normalized_solution_keys,
        legacy_report.normalized_solution_keys
    );
    assert_eq!(
        canonical_report.normalized_solution_set_hash,
        legacy_report.normalized_solution_set_hash
    );
    assert_eq!(
        canonical_report.covered_pattern_count,
        legacy_report.covered_pattern_count
    );
    assert_eq!(
        canonical_report.total_possible_pattern_count,
        legacy_report.total_possible_pattern_count
    );
    assert_eq!(
        canonical_report.coverage_probability,
        legacy_report.coverage_probability
    );
}

#[test]
fn two_line_distinct_bag_is_complete_empty_but_duplicate_fixed_queue_is_not() {
    let runtime = WasmCommandRuntime::default();
    // A two-line PC consumes five tetrominoes. Every Standard7Bag/P7 prefix has
    // five distinct pieces, and exhaustive exact-cover enumeration has no such
    // tiling. IIOOO deliberately contains duplicates and therefore belongs to
    // a different fixed-queue universe with four exact solution identities.
    let opening = runtime
        .run_command_text("clearra pc --lines 2 --backend cpu --workers 1")
        .expect("two-line Standard7Bag opening result");
    let explicit_p7 = runtime
        .run_command_text("clearra pc --lines 2 --patterns P7 --backend cpu --workers 1")
        .expect("two-line P7 result");
    let fixed = runtime
        .run_command_text("clearra pc --lines 2 --queue IIOOO --backend cpu --workers 1")
        .expect("two-line duplicate fixed-queue result");
    let opening_report = opening.search_report().expect("opening report");
    let p7_report = explicit_p7.search_report().expect("P7 report");
    let fixed_report = fixed.search_report().expect("fixed queue report");

    assert_eq!(opening_report.total_possible_pattern_count, "5040");
    assert_eq!(opening_report.unique_solution_count, 0);
    assert_eq!(opening_report.covered_pattern_count, 0);
    assert_eq!(p7_report.unique_solution_count, 0);
    assert_eq!(p7_report.covered_pattern_count, 0);
    assert_eq!(fixed_report.unique_solution_count, 4);
    assert_eq!(fixed_report.covered_pattern_count, 1);
}

#[test]
fn canonical_minimals_complete_empty_payload_is_not_a_validation_failure() {
    let result = WasmCommandRuntime::default()
        .run_command_text("clearra pc minimals --lines 2 --backend cpu --workers 1")
        .expect("complete-empty canonical minimum-cover result");

    assert_eq!(result.app_response().status(), AppStatus::Success);
    let payload = result
        .app_response()
        .product_result_payload()
        .expect("minimum-cover payload");
    let ProductResultPayloadContent::CoveragePortfolio(page) = payload.content() else {
        panic!("expected complete-empty coverage portfolio")
    };
    assert_eq!(page.optimal_cardinality(), "0");
    assert_eq!(page.known_alternative_count(), "1");
    assert_eq!(page.total_alternative_count(), Some("1"));
    assert!(page.enumeration_complete());
    assert_eq!(page.member_page_number(), "1");
    assert_eq!(page.total_member_pages(), "1");
    assert!(page.members().is_empty());
    assert_eq!(page.canonical_selection(), None);
    assert_eq!(page.canonical_witness(), None);
}

#[test]
fn wasm_pc_path_returns_the_complete_normal_replay_family_without_a_page_owner() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra pc path --lines 1 --board-mask 0x3f0 --height 1 \
             --pieces 1 --queue I --hold empty",
        )
        .expect("canonical WASM pc.path search");

    assert_eq!(
        result.app_response().status(),
        AppStatus::Success,
        "{:#?}",
        result.app_response()
    );
    assert!(result.product_page_source_owner().is_none());
    let payload = result
        .app_response()
        .product_result_payload()
        .expect("pc.path product payload");
    assert_eq!(payload.contract(), "pc.path");
    assert_eq!(payload.result_kind(), "pc-path-family.v2");
    let ProductResultPayloadContent::PcPathFamily(family) = payload.content() else {
        panic!("expected pc.path replay-family payload")
    };
    assert_eq!(family.witness_contract(), "pc-path-witness.v2");
    assert_eq!(
        family.ordering(),
        "candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending"
    );
    assert!(family.complete());
    assert_eq!(family.witness_count(), family.witnesses().len().to_string());
    assert!(!family.witnesses().is_empty());
    assert_eq!(
        family.canonical_selection(),
        "smallest-canonical-candidate-id"
    );
    assert_eq!(family.canonical_witness(), family.witnesses().first());
    assert!(family.witnesses().windows(2).all(|pair| {
        (
            pair[0].candidate_id(),
            pair[0].pattern_id(),
            pair[0].normalized_trace_key(),
        ) < (
            pair[1].candidate_id(),
            pair[1].pattern_id(),
            pair[1].normalized_trace_key(),
        )
    }));
    assert!(family.witnesses().iter().all(|witness| {
        !witness.steps().is_empty()
            && witness.steps().last().is_some_and(|step| {
                step.board_after_line_clear_mask() == "0x0000000000000000"
                    && step.cleared_lines() == "1"
                    && !step.line_clear_identity().is_empty()
            })
    }));
}

#[test]
fn wasm_pc_score_returns_every_normalized_field_with_its_whole_universe_average() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra pc score --lines 2 --patterns [IO][IO][IO][IO][IO] \
             --score-profile tetrio \
             --spin-profile t-spins --initial-b2b 0",
        )
        .expect("canonical WASM pc score search");

    assert_eq!(result.app_response().status(), AppStatus::Success);
    assert!(result.product_page_source_owner().is_none());
    let payload = result
        .app_response()
        .product_result_payload()
        .expect("pc score product payload");
    assert_eq!(payload.contract(), "pc.score");
    assert_eq!(payload.result_kind(), "pc-score-summary.v2");
    let ProductResultPayloadContent::PcScoreFieldSummary(summary) = payload.content() else {
        panic!("expected pc score field-summary payload")
    };
    assert_eq!(
        summary.field_contract(),
        "pc-score-solution-field-average.v1"
    );
    assert_eq!(summary.ordering(), "normalized-solution-field-order");
    assert_eq!(
        summary.solution_field_average_basis(),
        "whole-materialized-pattern-universe-failed-pc-zero"
    );
    assert_eq!(summary.score_evaluation_basis(), "all-traces");
    assert_eq!(summary.score_evaluation_scope(), "full");
    assert_eq!(
        summary.overall_score_basis(),
        "all-materialized-patterns-failed-pc-zero"
    );
    assert_eq!(summary.materialized_pattern_count(), "32");
    assert_eq!(summary.scored_pattern_count(), "16");
    assert_eq!(summary.failed_pc_pattern_count(), "16");
    assert!(summary.complete());
    assert_eq!(
        summary.solution_field_count(),
        summary.fields().len().to_string()
    );
    assert_eq!(summary.fields().len(), 8);
    let maximum_field_average = summary
        .fields()
        .iter()
        .map(|field| {
            assert!(field.normalized_field_key().starts_with("ctk1|"));
            assert_eq!(field.pattern_count(), "32");
            assert!(field.score_complete());
            field
                .average_score()
                .parse::<f64>()
                .expect("finite field average")
        })
        .fold(0.0_f64, f64::max);
    assert_eq!(maximum_field_average.to_bits(), 1140.625_f64.to_bits());
    assert!(summary.fields().iter().any(|field| {
        field.covered_pattern_count() == "1"
            && field
                .average_score()
                .parse::<f64>()
                .is_ok_and(|score| score.to_bits() == 109.375_f64.to_bits())
    }));
    let overall_score = summary
        .overall_score()
        .parse::<f64>()
        .expect("finite all-pattern score");
    assert_eq!(overall_score.to_bits(), 1820.3125_f64.to_bits());
    assert!(overall_score > maximum_field_average);
    assert_eq!(
        summary
            .score_covered_pattern_conditional_average_score()
            .expect("covered queue conditional average")
            .parse::<f64>()
            .expect("finite conditional average")
            .to_bits(),
        3640.625_f64.to_bits()
    );
}

#[test]
fn wasm_pc_score_finder_returns_the_complete_normal_score_only_witness_family() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra pc score-finder --lines 1 --board-mask 0x3f --height 1 \
             --pieces 1 --queue I --hold empty --initial-b2b 1",
        )
        .expect("canonical WASM pc score-finder search");

    assert_eq!(result.app_response().status(), AppStatus::Success);
    assert!(result.product_page_source_owner().is_none());
    let payload = result
        .app_response()
        .product_result_payload()
        .expect("pc score-finder product payload");
    assert_eq!(payload.contract(), "pc.score-finder");
    assert_eq!(payload.result_kind(), "pc-fixed-score-witness.v2");
    let ProductResultPayloadContent::ScorePatternWinnerFamily(family) = payload.content() else {
        panic!("expected pc score-finder winner-family payload")
    };
    assert_eq!(family.winner_contract(), "pc-score-pattern-winner.v1");
    assert_eq!(
        family.ordering(),
        "pattern-id-ascending-then-candidate-id-ascending"
    );
    assert_eq!(family.equality(), "score-only-attack-informational");
    assert_eq!(
        family.informational_attack_basis(),
        "canonical-equal-score-trace"
    );
    assert_eq!(family.winner_count(), family.winners().len().to_string());
    assert_eq!(
        family.canonical_selection(),
        "smallest-canonical-candidate-id"
    );
    assert!(family
        .winners()
        .iter()
        .any(|winner| winner == family.canonical_winner()));
    let canonical_candidate_id = family
        .canonical_winner()
        .candidate_id()
        .parse::<u64>()
        .expect("canonical candidate ID");
    assert!(family.winners().iter().all(|winner| {
        winner.candidate_id().parse::<u64>().expect("candidate ID") >= canonical_candidate_id
    }));
    assert!(family
        .winners()
        .windows(2)
        .all(|pair| pair[0].candidate_id() < pair[1].candidate_id()));
    assert!(family
        .winners()
        .iter()
        .all(|winner| winner.pattern_id() == "0"));
}

#[test]
fn coverage_summary_reports_not_calculated_solution_availability_without_a_fake_zero() {
    let app_response = AppResponse::success(AppRenderModel::Percent(CoreExecutionResult::new(
        vec![
            ("backend_requested".to_owned(), "cpu".to_owned()),
            ("backend_selected".to_owned(), "wasm-cpu".to_owned()),
            (
                "search_output_policy".to_owned(),
                "coverage-summary".to_owned(),
            ),
            (
                "unique_solution_count".to_owned(),
                "not-calculated".to_owned(),
            ),
            (
                "normalized_unique_solution_count".to_owned(),
                "not-calculated".to_owned(),
            ),
            (
                "normalized_solution_set_hash".to_owned(),
                "not-calculated".to_owned(),
            ),
            (
                "actual_normalized_solution_set_hash".to_owned(),
                "not-calculated".to_owned(),
            ),
            ("solution_count_calculated".to_owned(), "false".to_owned()),
            ("solution_set_materialized".to_owned(), "false".to_owned()),
            (
                "solution_keys_materialized_count".to_owned(),
                "0".to_owned(),
            ),
            ("solution_keys_complete".to_owned(), "false".to_owned()),
            ("solution_page_available".to_owned(), "false".to_owned()),
        ],
        Vec::new(),
    )));
    let report = WasmSearchReport::from_response(&app_response).expect("percent search report");
    let core_result = app_response
        .render_model()
        .and_then(|model| model.core_result())
        .expect("synthetic core result");

    assert_eq!(report.unique_solution_count, 0);
    assert!(!report.solution_count_calculated);
    assert!(!report.solution_set_materialized);
    assert_eq!(report.solution_keys_materialized_count, 0);
    assert!(!report.solution_keys_complete);
    assert!(!report.solution_page_available);
    assert!(!solution_page_store_is_public(core_result));

    let json = serialize_search_report_from_app_response(&app_response)
        .expect("serialized percent search report");
    let value: Value = serde_json::from_str(&json).expect("percent search report JSON");
    assert_eq!(value["unique_solution_count"], 0);
    assert_eq!(value["solution_count_calculated"], false);
    assert_eq!(value["solution_set_materialized"], false);
    assert_eq!(value["solution_keys_materialized_count"], 0);
    assert_eq!(value["solution_keys_complete"], false);
    assert_eq!(value["solution_page_available"], false);

    let execution = WasmExecutionResult::from_app_response(app_response, false);
    assert!(execution.tiling_solution_page_store().is_none());
}

#[test]
fn calculated_complete_empty_solution_set_remains_an_actual_zero() {
    let app_response = AppResponse::success(AppRenderModel::Percent(CoreExecutionResult::new(
        vec![
            ("backend_selected".to_owned(), "wasm-cpu".to_owned()),
            ("search_output_policy".to_owned(), "summary".to_owned()),
            ("unique_solution_count".to_owned(), "0".to_owned()),
            (
                "normalized_unique_solution_count".to_owned(),
                "0".to_owned(),
            ),
            ("solution_count_calculated".to_owned(), "true".to_owned()),
            ("solution_set_materialized".to_owned(), "true".to_owned()),
            (
                "solution_keys_materialized_count".to_owned(),
                "0".to_owned(),
            ),
            ("solution_keys_complete".to_owned(), "true".to_owned()),
            ("solution_page_available".to_owned(), "false".to_owned()),
        ],
        Vec::new(),
    )));
    let core_result = app_response
        .render_model()
        .and_then(|model| model.core_result())
        .expect("synthetic core result");
    assert!(!solution_page_store_is_public(core_result));

    let execution = WasmExecutionResult::from_app_response(app_response, false);
    let report = execution.search_report().expect("empty search report");

    assert_eq!(report.unique_solution_count, 0);
    assert!(report.solution_count_calculated);
    assert!(report.solution_set_materialized);
    assert!(report.solution_keys_complete);
    assert!(!report.solution_page_available);
    assert!(execution.tiling_solution_page_store().is_none());
}

#[test]
fn malformed_coverage_summary_availability_is_unavailable_without_artifacts() {
    let valid_fields = vec![
        ("backend_selected".to_owned(), "wasm-cpu".to_owned()),
        (
            "search_output_policy".to_owned(),
            "coverage-summary".to_owned(),
        ),
        (
            "unique_solution_count".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "normalized_unique_solution_count".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "normalized_solution_set_hash".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "actual_normalized_solution_set_hash".to_owned(),
            "not-calculated".to_owned(),
        ),
        ("solution_count_calculated".to_owned(), "false".to_owned()),
        ("solution_set_materialized".to_owned(), "false".to_owned()),
        (
            "solution_keys_materialized_count".to_owned(),
            "0".to_owned(),
        ),
        ("solution_keys_complete".to_owned(), "false".to_owned()),
        ("solution_page_available".to_owned(), "false".to_owned()),
    ];
    let malformed_cases = [
        ("unique_solution_count", "0"),
        ("normalized_unique_solution_count", "0"),
        ("normalized_solution_set_hash", "cts1:fake"),
        ("actual_normalized_solution_set_hash", "cts1:fake"),
        ("solution_count_calculated", "true"),
        ("solution_set_materialized", "true"),
        ("solution_keys_materialized_count", "1"),
        ("solution_keys_complete", "true"),
        ("solution_page_available", "true"),
    ];

    for (key, value) in malformed_cases {
        let mut fields = valid_fields.clone();
        fields
            .iter_mut()
            .find(|(field_key, _)| field_key == key)
            .expect("coverage availability key")
            .1 = value.to_owned();
        let app_response = AppResponse::success(AppRenderModel::Percent(
            CoreExecutionResult::new(fields, Vec::new())
                .with_normalized_solution_keys(vec!["fake-solution-key".to_owned()])
                .with_packing_candidate_keys(vec!["fake-packing-key".to_owned()]),
        ));
        let execution = WasmExecutionResult::from_app_response(app_response, false);
        let report = execution.search_report().expect("synthetic search report");

        assert!(!report.solution_count_calculated, "malformed {key}");
        assert!(!report.solution_set_materialized, "malformed {key}");
        assert_eq!(
            report.solution_keys_materialized_count, 0,
            "malformed {key}"
        );
        assert!(!report.solution_keys_complete, "malformed {key}");
        assert!(!report.solution_page_available, "malformed {key}");
        assert_eq!(report.unique_solution_count, 0, "malformed {key}");
        assert!(
            report.normalized_solution_keys.is_empty(),
            "malformed {key}"
        );
        assert!(report.packing_candidate_keys.is_empty(), "malformed {key}");
        assert_eq!(report.normalized_solution_set_hash, "not-calculated");
        assert!(
            execution.tiling_solution_page_store().is_none(),
            "malformed {key}"
        );
        let summary = report
            .summary_fields
            .iter()
            .cloned()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            summary.get("unique_solution_count").map(String::as_str),
            Some("not-calculated"),
            "malformed {key}"
        );
        assert_eq!(
            summary.get("solution_count_calculated").map(String::as_str),
            Some("false"),
            "malformed {key}"
        );
    }
}

#[test]
fn invalid_contract_hides_finesse_search_but_keeps_score_exception() {
    let fields = vec![
        ("backend_selected".to_owned(), "wasm-cpu".to_owned()),
        (
            "search_output_policy".to_owned(),
            "coverage-summary".to_owned(),
        ),
        (
            "unique_solution_count".to_owned(),
            "not-calculated".to_owned(),
        ),
    ];
    for (mode, expected_report) in [("search", false), ("score", true)] {
        let response = AppResponse::success(AppRenderModel::Percent(
            CoreExecutionResult::new(fields.clone(), Vec::new())
                .with_finesse_report(FinesseReport::new(mode, "oracle", true, None, Vec::new())),
        ));
        let report = WasmSearchReport::from_response(&response).expect("WASM search report");
        assert_eq!(report.finesse_report.is_some(), expected_report, "{mode}");
        assert!(!report.solution_set_materialized, "{mode}");
        assert!(report.normalized_solution_keys.is_empty(), "{mode}");
    }
}

#[test]
fn wasm_setup_command_preserves_the_exact_residue_contract() {
    let request = WasmCommandRuntime::default()
        .compile_command_text("clearra setup --remaining IOTS")
        .expect("setup AppRequest");

    let AppCommand::Setup(command) = request.command() else {
        panic!("expected setup command");
    };
    assert_eq!(command.query().residue().remaining_count(), 4);
    assert_eq!(command.query().residue().cycle(), Some(2));
    assert_eq!(command.query().residue().duplicate_piece(), None);
    assert_eq!(
        command.query().hold_policy(),
        clearra_problem::SetupHoldPolicy::EnabledEmpty
    );

    let cycle_boundary = WasmCommandRuntime::default()
        .compile_command_text("clearra setup --remaining IOT --allow-post-cycle-borrow")
        .expect("cycle-seven setup AppRequest");
    let AppCommand::Setup(command) = cycle_boundary.command() else {
        panic!("expected setup command");
    };
    assert_eq!(command.query().residue().cycle(), Some(7));
    assert_eq!(
        command.query().cycle_reset_borrow_policy(),
        clearra_problem::SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
    );
}

#[test]
fn wasm_setup_command_preserves_observed_qb_and_next_cycle_inventory() {
    let request = WasmCommandRuntime::default()
        .compile_command_text(
            "clearra setup --remaining TI --mode qb --qb OS \
             --next-cycle-remaining OOSITZ",
        )
        .expect("QB setup AppRequest");

    let AppCommand::Setup(command) = request.command() else {
        panic!("expected setup command");
    };
    assert_eq!(
        command.query().search_mode(),
        clearra_problem::SetupSearchMode::QueueBased
    );
    assert_eq!(
        command.query().residue().pieces(),
        &[
            clearra_core_domain::piece::piece_kind::PieceKind::T,
            clearra_core_domain::piece::piece_kind::PieceKind::I,
        ]
    );
    assert_eq!(
        command
            .query()
            .queue()
            .as_fixed_sequence()
            .expect("fixed QB queue")
            .pieces(),
        &[
            clearra_core_domain::piece::piece_kind::PieceKind::O,
            clearra_core_domain::piece::piece_kind::PieceKind::S,
        ]
    );
    assert_eq!(
        command.query().next_cycle_remaining_pieces(),
        Some(
            &[
                clearra_core_domain::piece::piece_kind::PieceKind::O,
                clearra_core_domain::piece::piece_kind::PieceKind::O,
                clearra_core_domain::piece::piece_kind::PieceKind::S,
                clearra_core_domain::piece::piece_kind::PieceKind::I,
                clearra_core_domain::piece::piece_kind::PieceKind::T,
                clearra_core_domain::piece::piece_kind::PieceKind::Z,
            ][..]
        )
    );
}

#[test]
fn distributed_setup_finalize_preserves_the_cancellation_reason() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let preparation = WasmDistributedCoordinator::prepare(
        &runtime,
        "clearra setup-finder --remaining IOT --workers 2",
    )
    .expect("distributed setup preparation");
    let coordinator = match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
        _ => panic!("setup search must use the distributed coordinator"),
    };
    coordinator.cancel();

    let error = match coordinator.finish(2) {
        Ok(_) => panic!("cancelled setup finalize must not complete"),
        Err(error) => error,
    };

    assert_eq!(error.code(), "E_WASM_DISTRIBUTED_SETUP_FINISH");
    assert_eq!(error.message(), "wasm_cpu_search_cancelled");
}

#[test]
fn occupied_initial_hold_plus_p7_solves_eight_piece_scenario() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra pc --lines 4 --board-mask 0x80787 --height 4 --pieces 8 --patterns P7 --hold S --backend cpu --workers 1",
        )
        .expect("WASM scenario result");
    let report = result
        .search_report()
        .unwrap_or_else(|| panic!("WASM search report: {:?}", result.app_response()));

    assert!(
        report.solution_found,
        "initial hold S and the seven P7 pieces form the eight placed pieces"
    );
    assert!(report.count_complete);
    assert!(report.projects_unplaced_lookahead);
    assert_eq!(report.source_sequence_length, 7);
}

#[test]
fn finite_pattern_releases_terminal_hold_for_complete_build_coverage() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra build-probability --base-mask 0x0 --target-mask 0xe0380e0380 --height 4 --patterns [LOJ]! --hold empty --no-mirror --workers 1",
        )
        .expect("finite-pattern build probability result");
    let report = result
        .search_report()
        .unwrap_or_else(|| panic!("WASM search report: {:?}", result.app_response()));

    assert!(report.count_complete);
    assert!(report.probability_complete);
    assert!(report.projects_unplaced_lookahead);
    assert_eq!(report.source_sequence_length, 3);
    assert_eq!(report.covered_pattern_count, 6);
    assert_eq!(report.unique_solution_count, 2);
    assert_eq!(report.normalized_solution_set_hash, "cts1:2770e9c1ff9a940e");
    assert!(
        (report
            .coverage_probability
            .parse::<f64>()
            .expect("coverage probability")
            - 1.0)
            .abs()
            <= f64::EPSILON
    );
}

#[test]
fn finesse_fixed_queue_witness_reaches_the_typed_wasm_json_contract() {
    const COMMAND: &str = "clearra finesse search --base-mask 0x0 --target-mask 0xf --height 1 \
         --queue I --no-hold --pattern-knowledge oracle --rule srs-plus";
    let result = WasmCommandRuntime::default()
        .run_command_text(COMMAND)
        .expect("fixed-queue finesse search");
    let report = result.search_report().expect("finesse search report");
    let finesse = report
        .finesse_report
        .as_ref()
        .expect("typed finesse report");
    let witness = finesse
        .representative_witness
        .as_ref()
        .expect("fixed queue witness");
    assert_eq!(witness.policy, "oracle");
    assert_eq!(witness.queue, ["I"]);
    assert_eq!(
        finesse
            .exact_total_inputs
            .as_deref()
            .and_then(|value| value.parse::<u32>().ok()),
        Some(witness.total_inputs)
    );
    assert_eq!(witness.input_sequence.len(), witness.total_inputs as usize);
    assert_eq!(
        witness.input_sequence.last().map(String::as_str),
        Some("hard-drop")
    );
    assert_eq!(witness.placements.len(), 1);
    assert_eq!(witness.placements[0].piece, "I");
    assert_eq!(witness.placements[0].rotation, 0);
    assert_eq!((witness.placements[0].x, witness.placements[0].y), (0, 0));

    let request = WasmCommandRuntime::default()
        .compile_command_text(COMMAND)
        .expect("finesse AppRequest");
    let app_response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request);
    let json = serialize_search_report_from_app_response(&app_response)
        .expect("serialized finesse search report");
    let value: Value = serde_json::from_str(&json).expect("valid search report JSON");
    let json_witness = &value["finesse_report"]["representative_witness"];
    assert_eq!(json_witness["queue"], serde_json::json!(["I"]));
    assert_eq!(json_witness["total_inputs"], witness.total_inputs);
    assert_eq!(
        json_witness["input_sequence"].as_array().map(Vec::len),
        Some(witness.total_inputs as usize)
    );
    assert_eq!(
        json_witness["placements"],
        serde_json::json!([{"piece":"I","rotation":0,"x":0,"y":0}])
    );
}

#[test]
fn finesse_fixed_queue_score_reaches_the_typed_wasm_report_contract() {
    const COMMAND: &str = "clearra finesse score --initial-mask 0 --height 4 \
         --placements O:spawn:4:0 --queue O --no-hold --pattern-knowledge both \
         --rule srs-plus --workers 2";
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let result = runtime
        .run_command_text(COMMAND)
        .expect("fixed-queue finesse score");
    let report = result.search_report().expect("finesse score report");
    let finesse = report
        .finesse_report
        .as_ref()
        .expect("typed finesse score report");
    let witness = finesse
        .representative_witness
        .as_ref()
        .expect("fixed score witness");

    assert_eq!(report.workers_used, 1, "score remains globally serial");
    assert!(!report.cpu_parallel_execution);
    assert_eq!(finesse.mode, "score");
    assert_eq!(finesse.exact_total_inputs.as_deref(), Some("1"));
    assert_eq!(witness.total_inputs, 1);
    assert_eq!(witness.input_sequence, ["hard-drop"]);
    assert_eq!(witness.placements.len(), 1);

    let request = runtime
        .compile_command_text(COMMAND)
        .expect("finesse score AppRequest");
    let app_response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request);
    let core_result = app_response
        .render_model()
        .and_then(|model| model.core_result())
        .expect("score core result");
    assert_eq!(core_result.field("backend_selected"), None);
    assert_eq!(core_result.field("workers_used"), None);
    let json = serialize_search_report_from_app_response(&app_response)
        .expect("serialized finesse score report");
    let value: Value = serde_json::from_str(&json).expect("valid score report JSON");
    assert_eq!(value["workers_used"], 1);
    assert_eq!(value["finesse_report"]["mode"], "score");
    assert_eq!(value["finesse_report"]["exact_total_inputs"], "1");
    assert_eq!(
        value["finesse_report"]["representative_witness"]["input_sequence"],
        serde_json::json!(["hard-drop"])
    );
}

#[test]
fn browser_worker_final_event_keeps_the_fixed_score_typed_report() {
    let mut runtime = WasmWorkerJobRuntime::default();
    let job_id = runtime
        .start_job(
            "clearra finesse score --initial-mask 0 --height 4 \
             --placements O:spawn:4:0 --queue O --no-hold \
             --pattern-knowledge both --rule srs-plus --workers 2",
        )
        .expect("browser score job");
    while !runtime
        .advance_job(job_id, 4096)
        .expect("advance browser score job")
        .is_terminal()
    {}
    let json = runtime
        .drain_events_json(job_id)
        .expect("browser event JSON");
    let events: Value = serde_json::from_str(&json).expect("valid browser events");
    let final_event = events
        .as_array()
        .and_then(|events| {
            events
                .iter()
                .find(|event| event["event"] == "final_response")
        })
        .expect("final browser response");

    assert_eq!(final_event["response"]["status"], "success");
    assert_eq!(
        final_event["response"]["resource_report"]["execution_availability"]["state"],
        "available"
    );
    assert_eq!(
        final_event["response"]["resource_report"]["result_completeness"],
        "complete"
    );
    assert_eq!(
        final_event["response"]["result"],
        serde_json::json!({"kind": "build-probability"})
    );
    assert_eq!(final_event["search_report"]["workers_used"], 1);
    assert_eq!(
        final_event["search_report"]["finesse_report"]["mode"],
        "score"
    );
    assert_eq!(
        final_event["search_report"]["finesse_report"]["exact_total_inputs"],
        "1"
    );
    assert_eq!(
        final_event["search_report"]["finesse_report"]["representative_witness"]["total_inputs"],
        1
    );
}

#[test]
fn build_probability_tiling_only_returns_geometry_without_buildup_or_coverage() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
             --queue I --hold empty --no-mirror --tiling-only --workers 1",
        )
        .expect("build-probability tiling-only result");
    let report = result.search_report().expect("tiling-only report");

    assert_eq!(report.unique_solution_count, 1);
    assert!(!report.buildability_verified);
    assert!(!report.coverage_calculated);
    assert!(!report.probability_calculated);
    assert_eq!(report.coverage_probability, "not-calculated");
    assert_eq!(report.total_build_order_nodes, 0);
    assert_eq!(report.coverage_product_edge_checks, 0);
    assert!(report.count_complete);
}

#[test]
fn build_probability_explicit_gpu_fallback_is_reported_as_an_unsupported_kernel() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let result = runtime
        .run_command_text(
            "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
             --queue I --hold empty --no-mirror --tiling-only --backend gpu \
             --allow-backend-fallback --workers 1",
        )
        .expect("build-probability CPU fallback result");

    assert_eq!(result.app_response().status(), AppStatus::Success);
    assert_eq!(
        result.app_response().backend_report().backend_selected(),
        "wasm-cpu-build-probability"
    );
    assert!(result.app_response().backend_report().fallback_used());
    assert_eq!(
        result
            .app_response()
            .backend_report()
            .backend_fallback_reason(),
        Some("gpu_kernel_unavailable")
    );
    assert_eq!(
        result.app_response().backend_report().fallback_backend(),
        Some("wasm-cpu-build-probability")
    );
    assert!(result.webgpu_backend().fallback_used);
    assert_eq!(
        result.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("gpu_kernel_unavailable")
    );
}

#[test]
fn build_probability_explicit_gpu_without_fallback_is_unsupported() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let result = runtime
        .run_command_text(
            "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
             --queue I --hold empty --no-mirror --tiling-only --backend gpu \
             --no-backend-fallback --workers 1",
        )
        .expect("unsupported build-probability response");

    assert_eq!(result.app_response().status(), AppStatus::Unsupported);
    let diagnostic = result
        .app_response()
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code() == "E_PRODUCT_RUNTIME_UNSUPPORTED")
        .expect("unsupported build-probability diagnostic");
    assert!(diagnostic.message().contains("webgpu_backend_unavailable"));
    assert!(!result.app_response().backend_report().fallback_used());
    assert_eq!(
        result.app_response().backend_report().backend_selected(),
        "none"
    );
}

#[test]
fn build_complete_replay_accepts_original_and_mirrored_target_witnesses() {
    let result = WasmCommandRuntime::default()
        .with_host_capabilities(
            WasmHostCapabilities::new(4, false, false)
                .with_product_retention_budget(ProductRetentionBudget::new(64 * 1024 * 1024)),
        )
        .run_command_text(
            "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
             --queue I --hold empty --include-mirror --workers 1 \
             --result-mode complete-replay-paths",
        )
        .expect("tiny mirrored Build replay command");
    let response = result.app_response();
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    let ProductResultPayloadContent::BuildPathFamily(family) = response
        .product_result_payload()
        .expect("Build replay product")
        .content()
    else {
        panic!("expected Build replay family");
    };
    assert_eq!(family.target_terminal_board_mask(), "0x000000000000000f");
    assert_eq!(
        family.mirrored_terminal_board_mask(),
        Some("0x00000000000003c0")
    );
    for terminal in ["0x000000000000000f", "0x00000000000003c0"] {
        assert!(
            family.witnesses().iter().any(|witness| {
                witness
                    .steps()
                    .last()
                    .is_some_and(|step| step.board_after_line_clear_mask() == terminal)
            }),
            "missing original or mirrored terminal {terminal}"
        );
    }
}

#[test]
fn build_product_retention_budget_does_not_rewrite_search_memory_authority() {
    let mut runtime = WasmCommandRuntime::default();
    runtime.set_product_retention_budget(ProductRetentionBudget::new(1));
    let command = "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
                   --queue I --hold empty --no-mirror --workers 1 --result-mode field-average-score";
    let request = runtime
        .compile_command_text(command)
        .expect("product budget is not a finite parser route");
    assert_eq!(request.resource_budget().max_memory_mib(), None);
    let AppCommand::BuildProbability(build) = request.command() else {
        panic!("Build request");
    };
    assert_eq!(
        build
            .query()
            .core_query()
            .execution_policy()
            .max_memory_mib(),
        None
    );
    let result = runtime
        .run_command_text(command)
        .expect("search executes without finite-route rejection");
    assert_eq!(result.app_response().status(), AppStatus::ExecutionFailed);
    assert!(format!("{:?}", result.app_response())
        .contains("Build product whole-live memory limit exceeded"));
}

#[test]
fn build_probability_hybrid_unavailable_selects_cpu_without_fallback() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let result = runtime
        .run_command_text(
            "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
             --queue I --hold empty --no-mirror --tiling-only --backend hybrid \
             --no-backend-fallback --workers 1",
        )
        .expect("hybrid build-probability CPU result");

    assert_eq!(result.app_response().status(), AppStatus::Success);
    assert_eq!(
        result.app_response().backend_report().backend_selected(),
        "wasm-cpu-build-probability"
    );
    assert!(!result.app_response().backend_report().fallback_used());
    assert_eq!(
        result
            .app_response()
            .backend_report()
            .backend_fallback_reason(),
        None
    );
    assert_eq!(
        result.app_response().backend_report().fallback_backend(),
        None
    );
    assert!(!result.webgpu_backend().fallback_used);
    assert_eq!(
        result.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("gpu_kernel_unavailable")
    );
}

#[test]
fn inverse_b2b_constraint_removes_a_normal_non_pc_line_clear() {
    let request = WasmCommandRuntime::default()
        .compile_command_text(
            "clearra build-probability --base-mask 0x803f0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror --preserve-b2b --spin-profile t-spins",
        )
        .expect("constrained request");
    let response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request);
    let core = response
        .render_model()
        .and_then(|model| model.core_result())
        .expect("core result");

    assert_eq!(
        core.field("execution_constraint_materialized"),
        Some("true")
    );
    assert_eq!(core.field("unique_solution_count"), Some("0"));
    assert_eq!(core.field("covered_pattern_count"), Some("0"));
    assert_eq!(core.field("solution_found"), Some("false"));
}

#[test]
fn all_piece_spin_profiles_do_not_promote_an_upward_mobile_o_clear() {
    let runtime = WasmCommandRuntime::default();
    let base_command = "clearra pc --lines 4 --board-mask 0xf3fcff3fcf --height 4 --pieces 2 --queue OO --no-hold --backend cpu --workers 1";
    let unconstrained = runtime
        .run_command_text(base_command)
        .expect("unconstrained O-piece perfect clear");
    assert!(
        unconstrained
            .search_report()
            .expect("unconstrained search report")
            .solution_found
    );

    for profile in ["all-spin", "all-spin-plus", "all-mini", "all-mini-plus"] {
        let constrained = runtime
            .run_command_text(&format!(
                "{base_command} --preserve-b2b --spin-profile {profile}"
            ))
            .unwrap_or_else(|error| panic!("{profile} constrained search failed: {error:?}"));
        let report = constrained
            .search_report()
            .unwrap_or_else(|| panic!("{profile} search report"));
        assert!(
            !report.solution_found,
            "{profile} must reject the first ordinary O double before the final perfect clear"
        );
        assert_eq!(report.unique_solution_count, 0, "{profile}");
        assert!(report.solution_count_calculated, "{profile}");
        assert!(report.solution_set_materialized, "{profile}");
        assert_eq!(report.covered_pattern_count, 0, "{profile}");
    }
}

#[test]
fn all_mini_plus_b2b_build_probability_matches_the_93_percent_reference() {
    let result = WasmCommandRuntime::default()
        .run_command_text(
            "clearra build-probability --base-mask 0x0 --target-mask 0xe81a06fffbf --height 8 --patterns P7 --hold empty --aggregate build --rule srs-plus --spin-profile all-mini-plus --preserve-b2b --include-mirror --workers 1",
        )
        .expect("All-Mini+ B2B build probability");
    let report = result
        .search_report()
        .unwrap_or_else(|| panic!("WASM search report: {:?}", result.app_response()));

    assert_eq!(report.materialized_pattern_count, 5_040);
    assert_eq!(report.covered_pattern_count, 4_704);
    assert!(
        (report
            .coverage_probability
            .parse::<f64>()
            .expect("coverage probability")
            - 4_704.0 / 5_040.0)
            .abs()
            <= 1.0e-12
    );
}

#[test]
fn all_mini_plus_b2b_pc_preserves_asymmetric_srs_plus_hold_paths() {
    let runtime = WasmCommandRuntime::default();
    let result = runtime
        .run_command_text(
            "clearra pc --lines 5 --board-mask 0xf01e0783f0f --height 5 --pieces 7 --patterns P7 --hold empty --backend cpu --workers 1 --preserve-b2b --spin-profile all-mini-plus",
        )
        .expect("All-Mini+ B2B 5L PC probability");
    let report = result
        .search_report()
        .unwrap_or_else(|| panic!("WASM search report: {:?}", result.app_response()));

    assert_eq!(report.materialized_pattern_count, 5_040);
    // ISOTZLJ and its hold-equivalent patterns use an asymmetric first-success
    // I-kick predecessor; reverse kick lookup must not discard those 18 queues.
    assert_eq!(report.covered_pattern_count, 4_032);
    assert!(
        (report
            .coverage_probability
            .parse::<f64>()
            .expect("coverage probability")
            - 4_032.0 / 5_040.0)
            .abs()
            <= 1.0e-12
    );

    let fixed_queue = runtime
        .run_command_text(
            "clearra pc --lines 5 --board-mask 0xf01e0783f0f --height 5 --pieces 7 --patterns ISOTZLJ --hold empty --backend cpu --workers 1 --preserve-b2b --spin-profile all-mini-plus",
        )
        .expect("asymmetric SRS+ hold path");
    let fixed_queue_report = fixed_queue
        .search_report()
        .unwrap_or_else(|| panic!("WASM search report: {:?}", fixed_queue.app_response()));

    assert_eq!(fixed_queue_report.materialized_pattern_count, 1);
    assert_eq!(fixed_queue_report.covered_pattern_count, 1);
}

#[test]
fn wasm_runtime_does_not_use_native_path_semantics() {
    let error = WasmCommandRuntime::default()
        .compile_command_text("clearra pc --fixture C:\\field.json")
        .expect_err("native paths are rejected");

    assert_eq!(error.code(), "E_WASM_NATIVE_PATH_FORBIDDEN");
}

#[test]
fn wasm_runtime_does_not_spawn_process() {
    let error = WasmCommandRuntime::default()
        .compile_command_text("clearra pc --lines 2 | verify")
        .expect_err("process syntax is rejected");

    assert_eq!(error.code(), "E_WASM_PROCESS_SEMANTICS_FORBIDDEN");
}

#[test]
fn cancel_long_6l_stops_before_natural_completion_and_releases_scope() {
    let mut runtime = WasmWorkerJobRuntime::default();
    let job_id = runtime.start_job("clearra verify kicks").expect("job");
    while !runtime
        .advance_job(job_id, 64)
        .expect("advance job")
        .is_terminal()
    {}
    let events = runtime.drain_events(job_id);

    assert!(events
        .iter()
        .any(|event| matches!(event, WasmWorkerJobEvent::Progress { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, WasmWorkerJobEvent::FinalResponse { .. })));

    let cancel_id = runtime
        .start_job("clearra pc --lines 6 --backend cpu --queue IOTSZJLIOTSZJLI")
        .expect("active job");
    assert_eq!(
        runtime.advance_job(cancel_id, 64).expect("prepare job"),
        WasmWorkerAdvanceStatus::Pending
    );
    assert_eq!(
        runtime
            .advance_job(cancel_id, 1)
            .expect("advance one exact search slice"),
        WasmWorkerAdvanceStatus::Pending,
        "6L search must yield before natural completion"
    );
    runtime.cancel_job(cancel_id).expect("cancel");
    let cancelled_events = runtime.drain_events(cancel_id);
    assert!(cancelled_events.iter().any(|event| matches!(
        event,
        WasmWorkerJobEvent::Cancelled {
            scope_released: true,
            ..
        }
    )));
    let serialized = crate::json_event_envelope::serialize_worker_events(&cancelled_events)
        .expect("serialize cancelled worker events");
    let serialized: Value = serde_json::from_str(&serialized).expect("cancelled event JSON");
    let cancelled = serialized
        .as_array()
        .and_then(|events| events.iter().find(|event| event["event"] == "cancelled"))
        .expect("serialized cancelled event");
    assert_eq!(cancelled["execution_availability"]["state"], "cancelled");
    assert_eq!(
        cancelled["execution_availability"]["reason"],
        "cancelled-by-caller"
    );
    assert_eq!(cancelled["result_completeness"], "incomplete");
    assert!(!cancelled_events
        .iter()
        .any(|event| matches!(event, WasmWorkerJobEvent::FinalResponse { .. })));
}

#[test]
fn parse_stage_failure_is_not_executed_and_never_emits_a_final_response() {
    let mut runtime = WasmWorkerJobRuntime::default();
    let job_id = runtime
        .start_job("clearra pc --lines not-a-number")
        .expect("queued job");

    assert_eq!(
        runtime.advance_job(job_id, 1).expect("parse failure"),
        WasmWorkerAdvanceStatus::Failed
    );
    let events = runtime.drain_events(job_id);
    assert!(!events
        .iter()
        .any(|event| matches!(event, WasmWorkerJobEvent::FinalResponse { .. })));
    let serialized = crate::json_event_envelope::serialize_worker_events(&events)
        .expect("serialize parse failure");
    let value: Value = serde_json::from_str(&serialized).expect("parse failure JSON");
    let failed = value
        .as_array()
        .and_then(|events| events.iter().find(|event| event["event"] == "failed"))
        .expect("failed event");
    assert!(failed["response"].is_null());
    assert_eq!(failed["execution_availability"]["state"], "unavailable");
    assert_eq!(failed["execution_availability"]["reason"], "not-executed");
    assert!(failed["execution_availability"]["descriptor_pattern_count"].is_null());
    assert!(failed["execution_availability"]["dense_pattern_count"].is_null());
    assert!(failed["execution_availability"]["required_dense_bytes"].is_null());
    assert!(failed["execution_availability"]["required_memory_bytes"].is_null());
    assert_eq!(failed["result_completeness"], "not-executed");
}

#[cfg(target_pointer_width = "64")]
#[test]
fn six_line_budget_admission_remains_typed_below_inactive_raw_wasm_boundaries() {
    let command = "clearra pc --lines 6 --backend cpu --workers 2 --max-memory-mib 64";
    let command_runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    // Test-only lower-layer admission oracle. Raw distributed and Worker
    // entry points are asserted separately below and must not use this parse.
    let request = command_runtime
        .compile_command_text(command)
        .expect("compile six-line command");
    let AppCommand::Pc(pc) = request.command() else {
        panic!("six-line command must compile as PC");
    };
    let problem = ProblemCompiler::compile_opening_pc(pc.query()).expect("six-line problem");
    assert_eq!(
        problem
            .piece_source()
            .materialized_universe()
            .unwrap()
            .total_possible_pattern_count(),
        1_066_867_200
    );
    assert_eq!(
        problem
            .piece_source()
            .materialized_universe()
            .unwrap()
            .pattern_count(),
        1_066_867_200
    );
    let admission_error = match WasmCpuSearchSession::new(&problem) {
        Ok(_) => panic!("six-line dense representation must exceed sixty-four MiB"),
        Err(error) => error,
    };
    let WasmCpuSearchError::ResourceAdmission { resource_report } = admission_error else {
        panic!("expected typed six-line admission failure, got {admission_error:?}");
    };
    let availability = resource_report.execution_availability();
    assert!(!resource_report.execution_started());
    assert!(!resource_report.result_complete());
    assert_eq!(
        availability.state(),
        CoreExecutionAvailabilityState::Exhausted
    );
    assert_eq!(
        availability.reason(),
        Some(ExecutionAvailabilityReason::MemoryBudgetExceeded)
    );
    assert_eq!(availability.descriptor_pattern_count(), Some(1_066_867_200));
    assert_eq!(availability.dense_pattern_count(), Some(1_066_867_200));
    assert_eq!(availability.required_dense_bytes(), Some(133_358_400));
    assert_eq!(availability.required_memory_bytes(), Some(133_358_400));

    let distributed_rejection = match WasmDistributedCoordinator::prepare(&command_runtime, command)
    {
        Err(error) => error,
        Ok(_) => panic!("raw finite distributed entry has no parser/owner authority"),
    };
    assert_eq!(
        distributed_rejection.code(),
        "E_WASM_FINITE_AUTHORITY_UNAVAILABLE"
    );
    assert!(distributed_rejection.message().is_empty());
    assert_eq!(distributed_rejection.message_capacity_for_test(), 0);
    assert!(distributed_rejection.resource_report().is_none());

    let mut worker = WasmWorkerJobRuntime::new(command_runtime);
    let rejection = worker
        .start_job(command)
        .expect_err("raw finite worker entry has no parser/owner authority");
    assert_eq!(rejection.code(), "E_WASM_FINITE_AUTHORITY_UNAVAILABLE");
    assert!(rejection.message().is_empty());
    assert_eq!(rejection.message_capacity_for_test(), 0);
    assert!(rejection.resource_report().is_none());
}

#[test]
fn distributed_build_admission_remains_typed_below_inactive_raw_wasm_boundaries() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let command = "clearra build-probability --base-mask 0x0 \
        --target-mask 0xffffffffff --height 4 \
        --no-hold --no-mirror --workers 2 --max-memory-mib 1";
    // Test-only lower-layer admission oracle; this preparation is not public
    // finite-ingress evidence.
    let prepared_command = runtime
        .prepare_command_text(command)
        .expect("compile finite Build admission fixture");
    let (request, _) = prepared_command.into_parts();
    let prepared = match runtime.app_context().prepare_distributed_search(request) {
        DistributedSearchPreparation::Search(prepared) => prepared,
        DistributedSearchPreparation::Ready(_) => {
            panic!("Build command must prepare a typed search")
        }
    };
    let (field, aggregation) = prepared
        .build_probability_request()
        .expect("Build probability request");
    let (finesse_metric, finesse_pattern_knowledge) = prepared
        .build_probability_finesse_request()
        .unwrap_or_default();
    let admission_error = match WasmBuildProbabilityCandidateProducer::new_with_finesse_typed(
        prepared.problem(),
        field,
        aggregation,
        finesse_metric,
        finesse_pattern_knowledge,
    ) {
        Ok(_) => panic!("one-replica Build plan must exceed one MiB"),
        Err(error) => error,
    };
    let WasmCpuSearchError::ResourceAdmission { resource_report } = admission_error else {
        panic!("expected typed Build admission failure, got {admission_error:?}");
    };
    let availability = resource_report.execution_availability();
    assert!(!resource_report.execution_started());
    assert!(!resource_report.result_complete());
    assert_eq!(
        availability.state(),
        CoreExecutionAvailabilityState::Exhausted
    );
    assert_eq!(
        availability.reason(),
        Some(ExecutionAvailabilityReason::MemoryBudgetExceeded)
    );
    assert_eq!(availability.descriptor_pattern_count(), Some(1_058_400));
    assert_eq!(availability.dense_pattern_count(), Some(1_058_400));
    assert_eq!(availability.required_dense_bytes(), Some(132_304));
    assert_eq!(availability.required_memory_bytes(), Some(17_066_704));

    let distributed_rejection = match WasmDistributedCoordinator::prepare(&runtime, command) {
        Err(error) => error,
        Ok(_) => panic!("raw finite distributed entry has no parser/owner authority"),
    };
    assert_eq!(
        distributed_rejection.code(),
        "E_WASM_FINITE_AUTHORITY_UNAVAILABLE"
    );
    assert!(distributed_rejection.message().is_empty());
    assert_eq!(distributed_rejection.message_capacity_for_test(), 0);
    assert!(distributed_rejection.resource_report().is_none());

    let mut worker = WasmWorkerJobRuntime::new(runtime);
    let rejection = worker
        .start_job(command)
        .expect_err("raw finite worker entry has no parser/owner authority");
    assert_eq!(rejection.code(), "E_WASM_FINITE_AUTHORITY_UNAVAILABLE");
    assert!(rejection.message().is_empty());
    assert_eq!(rejection.message_capacity_for_test(), 0);
    assert!(rejection.resource_report().is_none());
}

#[cfg(target_pointer_width = "64")]
#[test]
fn distributed_build_aggregate_admission_remains_typed_below_the_raw_boundary() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let command = "clearra build-probability --base-mask 0x0 \
        --target-mask 0xffffffffff --height 4 \
        --no-hold --no-mirror --workers 2 --max-memory-mib 20";
    // Test-only lower-layer admission oracle; the production raw boundary is
    // checked after the producer-plus-verifier accounting assertions.
    let prepared_command = runtime
        .prepare_command_text(command)
        .expect("compile aggregate-admission command");
    let (request, _) = prepared_command.into_parts();
    let prepared = match runtime.app_context().prepare_distributed_search(request) {
        DistributedSearchPreparation::Search(prepared) => prepared,
        DistributedSearchPreparation::Ready(_) => {
            panic!("build command must prepare a distributed search")
        }
    };
    let (field, aggregation) = prepared
        .build_probability_request()
        .expect("build-probability request");
    let (finesse_metric, finesse_pattern_knowledge) = prepared
        .build_probability_finesse_request()
        .unwrap_or_default();

    let standalone = WasmBuildProbabilityCandidateProducer::new_with_finesse_typed(
        prepared.problem(),
        field,
        aggregation,
        finesse_metric,
        finesse_pattern_knowledge,
    )
    .expect("one-replica plan fits twenty MiB");
    drop(standalone);

    let aggregate_error =
        match WasmBuildProbabilityCandidateProducer::new_with_finesse_and_verifiers_typed(
            prepared.problem(),
            field,
            aggregation,
            finesse_metric,
            finesse_pattern_knowledge,
            1,
            0,
        ) {
            Ok(_) => panic!("producer plus verifier must not fit twenty MiB"),
            Err(error) => error,
        };
    let WasmCpuSearchError::ResourceAdmission { resource_report } = aggregate_error else {
        panic!("expected typed aggregate admission failure, got {aggregate_error:?}");
    };
    assert!(!resource_report.execution_started());
    assert!(!resource_report.result_complete());
    assert_eq!(
        resource_report.execution_availability().reason(),
        Some(ExecutionAvailabilityReason::MemoryBudgetExceeded)
    );
    assert_eq!(
        resource_report
            .execution_availability()
            .descriptor_pattern_count(),
        Some(1_058_400)
    );
    assert_eq!(
        resource_report
            .execution_availability()
            .dense_pattern_count(),
        Some(1_058_400)
    );
    assert_eq!(
        resource_report
            .execution_availability()
            .required_dense_bytes(),
        Some(132_304)
    );
    assert_eq!(
        resource_report
            .execution_availability()
            .required_memory_bytes(),
        Some(34_133_408)
    );

    let rejection = match WasmDistributedCoordinator::prepare(&runtime, command) {
        Err(error) => error,
        Ok(_) => panic!("raw finite distributed entry has no parser/owner authority"),
    };
    assert_eq!(rejection.code(), "E_WASM_FINITE_AUTHORITY_UNAVAILABLE");
    assert!(rejection.message().is_empty());
    assert_eq!(rejection.message_capacity_for_test(), 0);
    assert!(rejection.resource_report().is_none());
}

#[test]
fn wasm_output_keys_are_not_localized() {
    let result = WasmCommandRuntime::default()
        .run_command_text("clearra verify kicks")
        .expect("runtime output");
    let value: Value = serde_json::to_value(result.app_response()).expect("AppResponse json");

    assert_eq!(value["command"], "verify-kicks");
    assert_eq!(value["status"], "success");
    assert!(value["diagnostics"].is_array());
    assert!(value["backend_report"].is_object());
    assert!(value["resource_report"].is_object());
}

#[test]
fn wasm_user_shader_rejected() {
    let error = WasmCommandRuntime::default()
        .compile_command_text("clearra pc --wgsl user-shader.wgsl")
        .expect_err("user WGSL must not enter the typed command runtime");

    assert_eq!(error.code(), "E_WASM_COMMAND_UNSUPPORTED");
    let report = WebGpuBackendReport::not_requested();
    assert!(!report.shader.user_shader_allowed);
    assert!(!report.shader.runtime_shader_injection_allowed);
    assert!(report.shader.shader_hash.is_none());
}

#[test]
fn distributed_b2b_constraint_matches_serial_exact_result() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let serial_command = "clearra pc --lines 4 --count unique --backend cpu --workers 1 --queue IOTSZJLIOTS --preserve-b2b --spin-profile t-spins";
    let distributed_command = "clearra pc --lines 4 --count unique --backend cpu --workers 2 --queue IOTSZJLIOTS --preserve-b2b --spin-profile t-spins";
    let serial = runtime
        .run_command_text(serial_command)
        .expect("serial exact result");
    let distributed = run_distributed_cpu(&runtime, distributed_command);

    let serial_report = serial.search_report().expect("serial search report");
    let distributed_report = distributed
        .search_report()
        .expect("distributed search report");
    assert_eq!(
        distributed_report.unique_solution_count,
        serial_report.unique_solution_count
    );
    assert_eq!(
        distributed_report.normalized_solution_set_hash,
        serial_report.normalized_solution_set_hash
    );
    assert_eq!(
        distributed_report.covered_pattern_count,
        serial_report.covered_pattern_count
    );
    assert!(distributed_report.cpu_parallel_execution);
    assert_eq!(distributed_report.workers_used, 2);
    assert!(distributed_report
        .summary_fields
        .iter()
        .any(|(key, value)| { key == "execution_constraint_materialized" && value == "true" }));
}

#[test]
fn distributed_build_probability_b2b_constraint_matches_serial_exact_result() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let serial_command = "clearra build-probability --base-mask 0x0 --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI --no-hold --no-mirror --workers 1 --preserve-b2b --spin-profile t-spins";
    let distributed_command = "clearra build-probability --base-mask 0x0 --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI --no-hold --no-mirror --workers 2 --preserve-b2b --spin-profile t-spins";
    let serial = runtime
        .run_command_text(serial_command)
        .expect("serial build probability result");
    let distributed = run_distributed_cpu(&runtime, distributed_command);
    let serial_report = serial.search_report().expect("serial search report");
    let distributed_report = distributed
        .search_report()
        .expect("distributed search report");

    assert_eq!(serial_report.unique_solution_count, 8);
    assert_eq!(
        distributed_report.unique_solution_count,
        serial_report.unique_solution_count
    );
    assert_eq!(
        distributed_report.normalized_solution_set_hash,
        serial_report.normalized_solution_set_hash
    );
    assert_eq!(
        distributed_report.covered_pattern_count,
        serial_report.covered_pattern_count
    );
    assert!(distributed_report.cpu_parallel_execution);
    assert_eq!(distributed_report.workers_used, 2);
}

#[test]
fn distributed_build_solution_probabilities_match_serial_complete_canonical_reports() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    const COMMAND: &str = "clearra build-probability --base-mask 0x0 \
        --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI \
        --no-hold --no-mirror --preserve-b2b --spin-profile t-spins \
        --solution-probabilities";
    let serial = runtime
        .run_command_text(&format!("{COMMAND} --workers 1"))
        .expect("serial per-solution probability result");
    let distributed = run_distributed_cpu(&runtime, &format!("{COMMAND} --workers 2"));
    let serial_report = serial.search_report().expect("serial search report");
    let distributed_report = distributed
        .search_report()
        .expect("distributed search report");

    assert!(!serial_report.solution_probabilities.is_empty());
    assert_eq!(serial_report.solution_probabilities.len(), 8);
    assert_eq!(
        distributed_report.solution_probabilities,
        serial_report.solution_probabilities
    );
    let report_keys = serial_report
        .solution_probabilities
        .iter()
        .map(|entry| entry.solution_key.as_str())
        .collect::<Vec<_>>();
    let normalized_keys = serial_report
        .normalized_solution_keys
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_eq!(report_keys, normalized_keys);
    assert!(report_keys.windows(2).all(|pair| pair[0] < pair[1]));
    for entry in &serial_report.solution_probabilities {
        assert_eq!(entry.probability, "1", "{}", entry.solution_key);
        assert_eq!(entry.covered_pattern_count, 1, "{}", entry.solution_key);
        assert_eq!(entry.pattern_count, 1, "{}", entry.solution_key);
        assert!(entry.probability_complete, "{}", entry.solution_key);
    }

    let expected_metadata = [
        ("solution_probabilities_requested", "true"),
        ("solution_probability_count", "8"),
        ("solution_probability_complete", "true"),
        (
            "solution_probability_basis",
            "normalized-solution-pattern-bitset-or-union",
        ),
        ("solution_probability_incomplete_reason", "none"),
    ];
    for (key, expected) in expected_metadata {
        let serial_value = search_summary_field(serial_report, key);
        let distributed_value = search_summary_field(distributed_report, key);
        assert_eq!(serial_value, expected, "serial {key}");
        assert_eq!(distributed_value, serial_value, "distributed {key}");
    }
}

#[test]
fn distributed_build_probability_finesse_matches_serial_report_and_witness() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    // Seven O pieces keep the distributed eligibility threshold while making the
    // exact geometry catalog deliberately small and deterministic.
    let serial_command = "clearra build-probability --base-mask 0x0 --target-mask 0xfc3f3fcff --height 4 --queue OOOOOOO --no-hold --no-mirror --workers 1 --finesse inputs --pattern-knowledge both";
    let distributed_command = "clearra build-probability --base-mask 0x0 --target-mask 0xfc3f3fcff --height 4 --queue OOOOOOO --no-hold --no-mirror --workers 2 --finesse inputs --pattern-knowledge both";
    let serial = runtime
        .run_command_text(serial_command)
        .expect("serial finesse build probability result");
    let distributed = run_distributed_cpu(&runtime, distributed_command);
    let serial_result = serial.search_report().expect("serial search report");
    let distributed_result = distributed
        .search_report()
        .expect("distributed search report");

    assert_eq!(
        distributed_result.normalized_solution_keys,
        serial_result.normalized_solution_keys
    );
    assert_eq!(
        distributed_result.normalized_solution_set_hash,
        serial_result.normalized_solution_set_hash
    );
    assert_eq!(
        distributed_result.finesse_report,
        serial_result.finesse_report
    );
    assert!(serial_result
        .finesse_report
        .as_ref()
        .and_then(|report| report.representative_witness.as_ref())
        .is_some());
    assert_eq!(distributed_result.workers_used, 2);
}

#[test]
fn ctk3_spawn_blocked_finesse_matches_serial_instead_of_failing_distribution() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    // ctk3_w0kEaIIDmggnun6Vo_iPi8HogDAUR74DBhQocwCBgAEDCBQocODAAQQCDBAghACBAAMGiIkwGuQ
    // Page one is the occupied base. Page two contributes the colorless target delta.
    const COMMAND: &str = "clearra build-probability \
        --base-mask 0x3effbfeffbfeffbfeffbfeffbfeffbfeffbfeffbfeffbfef \
        --target-mask 0xa07e1fffe3c00000000000000000000000000000000000000000000000 \
        --height 24 --patterns P7 --hold empty --no-mirror \
        --finesse inputs --pattern-knowledge both";
    let serial = runtime
        .run_command_text(&format!("{COMMAND} --workers 1"))
        .expect("serial CTK3 finesse build probability");
    let distributed = run_distributed_cpu(&runtime, &format!("{COMMAND} --workers 2"));
    let serial_report = serial.search_report().expect("serial search report");
    let distributed_report = distributed
        .search_report()
        .expect("distributed search report");

    assert_build_probability_semantics_match(serial_report, distributed_report);
    assert!(!serial_report.solution_found);
    assert_eq!(serial_report.covered_pattern_count, 0);
    assert!(serial_report
        .finesse_report
        .as_ref()
        .is_some_and(|report| report.representative_witness.is_none()));
}

#[test]
fn initial_hold_cannot_bypass_a_blocked_current_piece_in_serial_or_distributed_finesse() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    const COMMAND: &str = "clearra build-probability \
        --base-mask 0x400000000000000000000000000000000000000000000000000000 \
        --target-mask 0xf --height 24 --queue OI --hold empty --no-mirror \
        --finesse inputs --pattern-knowledge both";
    let serial = runtime
        .run_command_text(&format!("{COMMAND} --workers 1"))
        .expect("serial blocked-current-piece finesse build probability");
    let distributed = runtime
        .run_command_text(&format!("{COMMAND} --workers 2"))
        .expect("two-worker blocked-current-piece finesse build probability");
    let serial_report = serial.search_report().expect("serial search report");
    let distributed_report = distributed
        .search_report()
        .expect("distributed search report");

    assert_build_probability_semantics_match(serial_report, distributed_report);
    assert!(!serial_report.solution_found);
    assert_eq!(serial_report.covered_pattern_count, 0);
    assert!(serial_report
        .finesse_report
        .as_ref()
        .is_some_and(|report| report.representative_witness.is_none()));
}

#[cfg(feature = "stage-profiling")]
#[test]
fn distributed_finesse_finalizer_records_every_coordinator_profile_stage() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let command = "clearra build-probability --base-mask 0x0 \
        --target-mask 0xfc3f3fcff --height 4 --queue OOOOOOO --no-hold \
        --no-mirror --workers 2 --finesse inputs --pattern-knowledge both";
    let preparation =
        WasmDistributedCoordinator::prepare(&runtime, command).expect("distributed preparation");
    let mut coordinator = match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
        _ => panic!("finesse search must use the distributed coordinator"),
    };
    let mut verifier = coordinator
        .prepare_in_process_verifier(&runtime, command)
        .expect("distributed verifier");
    loop {
        match coordinator
            .advance_producer(16_384, 16)
            .expect("geometry producer")
        {
            WasmDistributedProducerAdvance::Pending
            | WasmDistributedProducerAdvance::Initialization(_) => {}
            WasmDistributedProducerAdvance::Batch(batch) => {
                let mut consumed = verifier.consume(&batch).expect("candidate batch");
                if let Some(partial) = consumed.partial.take() {
                    coordinator
                        .absorb_partial(&partial)
                        .expect("merge streamed partial result");
                }
                while consumed.has_pending_work {
                    consumed = verifier.continue_work().expect("continue worker task");
                    if let Some(partial) = consumed.partial.take() {
                        coordinator
                            .absorb_partial(&partial)
                            .expect("merge streamed partial result");
                    }
                }
            }
            WasmDistributedProducerAdvance::Completed => break,
            WasmDistributedProducerAdvance::Cancelled => panic!("unexpected cancellation"),
        }
    }
    let partial = verifier.finish().expect("partial exact result");
    if !partial.is_empty() {
        coordinator
            .absorb_partial(&partial)
            .expect("merge partial exact result");
    }

    // Start after the worker has finished: every recorded finesse span below
    // must therefore belong to coordinator-side reconstruction and aggregation.
    let profile = ExecutorSearchProfileSession::start().expect("profile session");
    let result = coordinator.finish(2).expect("distributed exact result");
    let stages = profile.finish();
    assert_eq!(result.app_response().status(), AppStatus::Success);
    for required in [
        "finesse.geometry",
        "finesse.target_grouping",
        "finesse.movement_bfs",
        "finesse.annotation_prune",
        "finesse.product_dp",
        "finesse.aggregation",
    ] {
        let stage = stages
            .iter()
            .find(|stage| stage.name == required)
            .unwrap_or_else(|| panic!("missing coordinator profile stage {required}"));
        assert!(
            stage.invocation_count > 0,
            "coordinator profile stage {required} was not invoked"
        );
    }
}

#[cfg(feature = "stage-profiling")]
#[test]
fn fixed_queue_finesse_score_records_all_seven_profile_stages_serially() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let profile = ExecutorSearchProfileSession::start().expect("profile session");
    let result = runtime
        .run_command_text(
            "clearra finesse score --initial-mask 0 --height 4 \
             --placements O:spawn:4:0 --queue O --no-hold --workers 2 \
             --pattern-knowledge both",
        )
        .expect("serial fixed-queue finesse score");
    let stages = profile.finish();
    assert_eq!(result.app_response().status(), AppStatus::Success);
    for required in [
        "finesse.geometry",
        "finesse.target_grouping",
        "finesse.movement_bfs",
        "finesse.annotation_prune",
        "finesse.product_dp",
        "finesse.aggregation",
        "finesse.witness",
    ] {
        let stage = stages
            .iter()
            .find(|stage| stage.name == required)
            .unwrap_or_else(|| panic!("missing score profile stage {required}"));
        assert!(
            stage.invocation_count > 0,
            "score profile stage {required} was not invoked"
        );
    }
}

#[test]
fn distributed_build_probability_tiling_matches_serial_without_buildup() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let serial_command = "clearra build-probability --base-mask 0x0 --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI --no-hold --no-mirror --tiling-only --workers 1";
    let distributed_command = "clearra build-probability --base-mask 0x0 --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI --no-hold --no-mirror --tiling-only --workers 2";
    let serial = runtime
        .run_command_text(serial_command)
        .expect("serial build-probability tiling result");
    let (serial_solution_count, serial_solution_set_hash) = {
        let report = serial.search_report().expect("serial search report");
        (
            report.unique_solution_count,
            report.normalized_solution_set_hash.clone(),
        )
    };
    // Release only retained output memory before the next large run. This does
    // not grant verifier authority; coordinator delegation/fallback does that.
    drop(serial);
    let distributed = run_distributed_cpu(&runtime, distributed_command);
    let distributed_report = distributed
        .search_report()
        .expect("distributed search report");

    assert_eq!(
        distributed_report.unique_solution_count,
        serial_solution_count
    );
    assert_eq!(
        distributed_report.normalized_solution_set_hash,
        serial_solution_set_hash
    );
    assert_eq!(distributed_report.total_build_order_nodes, 0);
    assert_eq!(distributed_report.coverage_product_edge_checks, 0);
    assert!(!distributed_report.buildability_verified);
    assert_eq!(distributed_report.workers_used, 2);
}

#[test]
fn distributed_build_probability_tiling_unions_distinct_mirror_passes() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let serial_command = "clearra build-probability --base-mask 0x0 --target-mask 0xcc33fffff --height 4 --queue OOOOOOO --no-hold --include-mirror --tiling-only --workers 1";
    let distributed_command = "clearra build-probability --base-mask 0x0 --target-mask 0xcc33fffff --height 4 --queue OOOOOOO --no-hold --include-mirror --tiling-only --workers 2";
    let serial = runtime
        .run_command_text(serial_command)
        .expect("serial mirrored build-probability tiling result");
    let (serial_solution_count, serial_solution_set_hash) = {
        let report = serial.search_report().expect("serial search report");
        (
            report.unique_solution_count,
            report.normalized_solution_set_hash.clone(),
        )
    };
    // Release only retained output memory before the next large run. This does
    // not grant verifier authority; coordinator delegation/fallback does that.
    drop(serial);
    let distributed = run_distributed_cpu(&runtime, distributed_command);
    let distributed_report = distributed
        .search_report()
        .expect("distributed search report");

    assert!(serial_solution_count > 0);
    assert_eq!(
        distributed_report.unique_solution_count,
        serial_solution_count
    );
    assert_eq!(
        distributed_report.normalized_solution_set_hash,
        serial_solution_set_hash
    );
    assert!(distributed_report
        .summary_fields
        .iter()
        .any(|(key, value)| { key == "build_mirror_distinct_target" && value == "true" }));
    assert!(distributed_report
        .summary_fields
        .iter()
        .any(|(key, value)| { key == "build_mirror_search_executed" && value == "true" }));
    assert_eq!(distributed_report.total_build_order_nodes, 0);
    assert!(!distributed_report.buildability_verified);
}

#[test]
fn distributed_tiling_root_tasks_match_serial_hold_supply_result() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let serial_command = "clearra pc --lines 4 --board-mask 0x80787 --height 4 --pieces 8 --patterns P7 --hold S --tiling-only --backend cpu --workers 1";
    let distributed_command = "clearra pc --lines 4 --board-mask 0x80787 --height 4 --pieces 8 --patterns P7 --hold S --tiling-only --backend cpu --workers 2";
    let serial = runtime
        .run_command_text(serial_command)
        .expect("serial tiling result");
    let (serial_solution_count, serial_solution_set_hash) = {
        let report = serial.search_report().expect("serial search report");
        (
            report.unique_solution_count,
            report.normalized_solution_set_hash.clone(),
        )
    };
    // Release only retained output memory before the next large run. This does
    // not grant verifier authority; coordinator delegation/fallback does that.
    drop(serial);
    let distributed = run_distributed_cpu(&runtime, distributed_command);
    let distributed_report = distributed
        .search_report()
        .expect("distributed search report");

    assert_eq!(
        distributed_report.unique_solution_count,
        serial_solution_count
    );
    assert_eq!(
        distributed_report.normalized_solution_set_hash,
        serial_solution_set_hash
    );
    assert!(!distributed_report.buildability_verified);
    assert_eq!(distributed_report.workers_used, 2);
}

fn assert_build_probability_semantics_match(
    serial: &WasmSearchReport,
    distributed: &WasmSearchReport,
) {
    assert_eq!(
        distributed.supply_window_resolution,
        serial.supply_window_resolution
    );
    assert_eq!(
        distributed.projects_unplaced_lookahead,
        serial.projects_unplaced_lookahead
    );
    assert_eq!(
        distributed.source_sequence_length,
        serial.source_sequence_length
    );
    assert_eq!(
        distributed.total_possible_pattern_count,
        serial.total_possible_pattern_count
    );
    assert_eq!(distributed.solution_found, serial.solution_found);
    assert_eq!(
        distributed.packing_candidate_count,
        serial.packing_candidate_count
    );
    assert_eq!(
        distributed.geometry_candidate_family_count,
        serial.geometry_candidate_family_count
    );
    assert_eq!(
        distributed.packing_candidate_set_digest,
        serial.packing_candidate_set_digest
    );
    assert_eq!(
        distributed.packing_candidate_keys,
        serial.packing_candidate_keys
    );
    assert_eq!(
        distributed.unique_solution_count,
        serial.unique_solution_count
    );
    assert_eq!(
        distributed.normalized_solution_set_hash,
        serial.normalized_solution_set_hash
    );
    assert_eq!(
        distributed.normalized_solution_keys,
        serial.normalized_solution_keys
    );
    assert_eq!(
        distributed.solution_probabilities,
        serial.solution_probabilities
    );
    assert_eq!(
        distributed.solution_average_scores,
        serial.solution_average_scores
    );
    assert_eq!(distributed.finesse_report, serial.finesse_report);
    assert_eq!(distributed.build_variant_count, serial.build_variant_count);
    assert_eq!(
        distributed.build_variant_count_exact,
        serial.build_variant_count_exact
    );
    assert_eq!(
        distributed.buildability_verified,
        serial.buildability_verified
    );
    assert_eq!(distributed.coverage_calculated, serial.coverage_calculated);
    assert_eq!(
        distributed.probability_calculated,
        serial.probability_calculated
    );
    assert_eq!(
        distributed.materialized_pattern_count,
        serial.materialized_pattern_count
    );
    assert_eq!(
        distributed.covered_pattern_count,
        serial.covered_pattern_count
    );
    assert_eq!(
        distributed.coverage_probability,
        serial.coverage_probability
    );
    assert_eq!(
        distributed.probability_complete,
        serial.probability_complete
    );
    assert_eq!(distributed.count_complete, serial.count_complete);
    assert_eq!(distributed.resource_truncated, serial.resource_truncated);
    assert_eq!(
        distributed.resource_truncation_reason,
        serial.resource_truncation_reason
    );
    assert_eq!(
        distributed.representative_pattern_id,
        serial.representative_pattern_id
    );
    assert_eq!(distributed.representative_path, serial.representative_path);
}

fn search_summary_field<'a>(report: &'a WasmSearchReport, key: &str) -> &'a str {
    report
        .summary_fields
        .iter()
        .find_map(|(candidate, value)| (candidate == key).then_some(value.as_str()))
        .unwrap_or_else(|| panic!("missing search summary field {key}"))
}

// These typed PC regressions intentionally exercise the process-global
// terminal resource authority. Keep only this resource-sharing family serial;
// unrelated distributed tests must remain free to run in parallel.
static TYPED_PC_DISTRIBUTED_TEST_LOCK: Mutex<()> = Mutex::new(());

fn typed_pc_distributed_test_guard() -> MutexGuard<'static, ()> {
    TYPED_PC_DISTRIBUTED_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn run_distributed_cpu(runtime: &WasmCommandRuntime, command: &str) -> WasmExecutionResult {
    completed_distributed_cpu_source(runtime, command)
        .finish(2)
        .expect("distributed exact result")
}

fn completed_distributed_cpu_source(
    runtime: &WasmCommandRuntime,
    command: &str,
) -> WasmDistributedCoordinator {
    let preparation =
        WasmDistributedCoordinator::prepare(runtime, command).expect("distributed preparation");
    let mut coordinator = match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
        WasmDistributedPreparation::Serial => {
            panic!("two-worker request unexpectedly selected the serial product path")
        }
        WasmDistributedPreparation::Ready(result) => panic!(
            "two-worker request completed during preparation: {:?}",
            result.app_response()
        ),
    };
    let mut verifier = match coordinator.worker_initialization() {
        Some(initialization) => {
            WasmDistributedVerifierRuntime::prepare_forward(runtime, &initialization)
                .expect("distributed forward verifier")
        }
        None => coordinator
            .prepare_in_process_verifier(runtime, command)
            .expect("distributed verifier"),
    };
    loop {
        match coordinator
            .advance_producer(16_384, 16)
            .expect("geometry producer")
        {
            WasmDistributedProducerAdvance::Pending => {}
            WasmDistributedProducerAdvance::Initialization(_) => {}
            WasmDistributedProducerAdvance::Batch(batch) => {
                let mut consumed = verifier.consume(&batch).expect("candidate batch");
                if let Some(partial) = consumed.partial.take() {
                    coordinator
                        .absorb_partial(&partial)
                        .expect("merge streamed partial result");
                }
                while consumed.has_pending_work {
                    consumed = verifier.continue_work().expect("continue worker task");
                    if let Some(partial) = consumed.partial.take() {
                        coordinator
                            .absorb_partial(&partial)
                            .expect("merge streamed partial result");
                    }
                }
            }
            WasmDistributedProducerAdvance::Completed => break,
            WasmDistributedProducerAdvance::Cancelled => panic!("unexpected cancellation"),
        }
    }
    let partial = verifier.finish().expect("partial exact result");
    if !partial.is_empty() {
        coordinator
            .absorb_partial(&partial)
            .expect("merge partial exact result");
    }
    coordinator
}

/// Exercises the score transport as separate browser WASM instances without
/// pretending that producer and verifier share one process-global compute
/// lease. The first coordinator exports the canonical batches, the standalone
/// verifier consumes them under its own child request, and a fresh coordinator
/// reproduces the same geometry before merging the worker partials.
fn run_distributed_score_cpu(runtime: &WasmCommandRuntime, command: &str) -> WasmExecutionResult {
    fn coordinator(runtime: &WasmCommandRuntime, command: &str) -> WasmDistributedCoordinator {
        match WasmDistributedCoordinator::prepare(runtime, command)
            .expect("distributed score preparation")
        {
            WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
            WasmDistributedPreparation::Serial => {
                panic!("multi-worker score request unexpectedly selected serial execution")
            }
            WasmDistributedPreparation::Ready(result) => panic!(
                "multi-worker score request completed during preparation: {:?}",
                result.app_response()
            ),
        }
    }

    fn produce_batches(coordinator: &mut WasmDistributedCoordinator) -> Vec<Vec<u8>> {
        let mut batches = Vec::new();
        loop {
            match coordinator
                .advance_producer(16_384, 16)
                .expect("score geometry producer")
            {
                WasmDistributedProducerAdvance::Pending
                | WasmDistributedProducerAdvance::Initialization(_) => {}
                WasmDistributedProducerAdvance::Batch(batch) => batches.push(batch),
                WasmDistributedProducerAdvance::Completed => return batches,
                WasmDistributedProducerAdvance::Cancelled => {
                    panic!("score geometry was unexpectedly cancelled")
                }
            }
        }
    }

    let mut source = coordinator(runtime, command);
    let expected_worker_count = source.worker_count();
    let batches = produce_batches(&mut source);
    drop(source);

    let verifier_runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let mut verifier = WasmDistributedVerifierRuntime::prepare(&verifier_runtime, command)
        .expect("standalone distributed score verifier");
    let mut partials = Vec::new();
    for batch in &batches {
        let mut consumed = verifier.consume(batch).expect("score candidate batch");
        if let Some(partial) = consumed.partial.take() {
            partials.push(partial);
        }
        while consumed.has_pending_work {
            consumed = verifier
                .continue_work()
                .expect("continue score worker task");
            if let Some(partial) = consumed.partial.take() {
                partials.push(partial);
            }
        }
    }
    let partial = verifier.finish().expect("final score worker partial");
    if !partial.is_empty() {
        partials.push(partial);
    }
    drop(verifier);

    let mut coordinator = coordinator(runtime, command);
    assert_eq!(coordinator.worker_count(), expected_worker_count);
    let reproduced_batches = produce_batches(&mut coordinator);
    assert_eq!(reproduced_batches, batches, "score geometry wire ordering");
    for partial in partials {
        coordinator
            .absorb_partial(&partial)
            .expect("merge score worker partial");
    }
    coordinator
        .finish(expected_worker_count)
        .expect("distributed score result")
}

#[test]
fn visible_seven_pc_uses_the_global_serial_policy_finalizer() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let preparation = WasmDistributedCoordinator::prepare(
        &runtime,
        "clearra pc --lines 4 --count unique --backend cpu --workers 2 --queue-knowledge visible-7",
    )
    .expect("visible-seven distributed preparation");

    assert!(matches!(preparation, WasmDistributedPreparation::Serial));
}

#[test]
fn ren_uses_the_forward_coordinator_with_fixed_and_auto_worker_parity() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let base = "clearra ren --board-mask 0 --height 4 --queue TIOS --no-hold --rule srs-plus";
    let serial = runtime
        .run_command_text(&format!("{base} --workers 1"))
        .expect("serial REN result");
    let serial_report = serial.search_report().expect("serial REN report");
    let serial_projection = (
        serial_report.forward_search_kind.clone(),
        serial_report.forward_initial_board_mask.clone(),
        serial_report.forward_canonical_selection.clone(),
        serial_report.canonical_forward_outcome.clone(),
        serial_report.maximum_ren,
        serial_report.forward_outcomes.clone(),
    );
    drop(serial);

    for worker_option in ["--workers 2", "--auto-workers 2"] {
        let distributed = run_distributed_cpu(&runtime, &format!("{base} {worker_option}"));
        let report = distributed.search_report().expect("distributed REN report");
        let projection = (
            report.forward_search_kind.clone(),
            report.forward_initial_board_mask.clone(),
            report.forward_canonical_selection.clone(),
            report.canonical_forward_outcome.clone(),
            report.maximum_ren,
            report.forward_outcomes.clone(),
        );
        assert_eq!(projection, serial_projection, "{worker_option}");
        assert_eq!(report.workers_used, 2, "{worker_option}");
    }
}

#[test]
fn typed_pc_score_products_preserve_parent_workers_and_match_serial_payload_bytes() {
    let _resource_guard = typed_pc_distributed_test_guard();
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    // The board complement is exactly seven non-overlapping O placements. It
    // clears the product-neutral work estimate and catalog-aware gate while
    // keeping every score product fixture solvable and deterministic.
    for (product, source) in [
        (
            "score",
            "--patterns OOOOOOO --score-profile tetrio --spin-profile t-spins",
        ),
        (
            "score-minimals",
            "--patterns OOOOOOO --score-profile tetrio --spin-profile t-spins",
        ),
        ("score-finder", "--queue OOOOOOO"),
    ] {
        let base = format!(
            "clearra pc {product} --lines 4 --board-mask 0xf03c0c0300 --height 4 \
             --pieces 7 {source} --no-hold --initial-b2b 0"
        );
        let fixed = format!("{base} --workers 2");

        let (child_request, _) = runtime
            .prepare_distributed_score_child_command_text(&fixed)
            .unwrap_or_else(|error| panic!("typed pc {product} child request: {error:?}"))
            .into_parts();
        let child_policy = match child_request.command() {
            AppCommand::Pc(command) => command.query().execution_policy(),
            AppCommand::Scenario(command) => command.query().execution_policy(),
            command => panic!("typed pc {product} child command changed: {command:?}"),
        };
        assert_eq!(
            child_policy.worker_policy(),
            clearra_pc_graph::request::WorkerPolicy::Fixed(1)
        );
        assert!(!child_policy.use_all_logical_processors());

        // A real browser verifier has a distinct WASM instance. Preparing it
        // independently proves that its raw parent command is lowered to a
        // one-session child before shared score validation.
        let verifier_runtime = WasmCommandRuntime::default()
            .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
        let standalone_verifier =
            WasmDistributedVerifierRuntime::prepare(&verifier_runtime, &fixed).unwrap_or_else(
                |error| panic!("typed pc {product} verifier preparation: {error:?}"),
            );
        drop(standalone_verifier);

        let preparation = WasmDistributedCoordinator::prepare(&runtime, &fixed)
            .unwrap_or_else(|error| panic!("typed pc {product} coordinator: {error:?}"));
        let coordinator = match preparation {
            WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
            WasmDistributedPreparation::Serial => {
                panic!("typed pc {product} unexpectedly selected serial execution")
            }
            WasmDistributedPreparation::Ready(result) => panic!(
                "typed pc {product} completed during preparation: {:?}",
                result.app_response()
            ),
        };
        assert_eq!(coordinator.worker_count(), 2);
        drop(coordinator);

        let serial = runtime
            .run_command_text(&format!("{base} --workers 1"))
            .unwrap_or_else(|error| panic!("typed pc {product} serial result: {error:?}"));
        let serial_payload = serde_json::to_vec(
            serial
                .app_response()
                .product_result_payload()
                .unwrap_or_else(|| panic!("typed pc {product} serial payload")),
        )
        .expect("serialize serial score payload");
        let serial_page_source_available = serial.product_page_source_owner().is_some();
        drop(serial);

        let distributed = run_distributed_score_cpu(&runtime, &fixed);
        let distributed_payload = serde_json::to_vec(
            distributed
                .app_response()
                .product_result_payload()
                .unwrap_or_else(|| panic!("typed pc {product} distributed payload")),
        )
        .expect("serialize distributed score payload");
        assert_eq!(distributed_payload, serial_payload, "fixed {product}");
        assert_eq!(
            distributed.product_page_source_owner().is_some(),
            serial_page_source_available,
            "fixed {product} page authority"
        );
        drop(distributed);

        let automatic = run_distributed_score_cpu(&runtime, &format!("{base} --auto-workers 2"));
        let automatic_payload = serde_json::to_vec(
            automatic
                .app_response()
                .product_result_payload()
                .unwrap_or_else(|| panic!("typed pc {product} automatic payload")),
        )
        .expect("serialize automatic score payload");
        assert_eq!(automatic_payload, serial_payload, "automatic {product}");
        assert_eq!(
            automatic.product_page_source_owner().is_some(),
            serial_page_source_available,
            "automatic {product} page authority"
        );
    }
}

#[test]
fn three_piece_fixed_score_minimals_reaches_the_shared_distributed_coordinator() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let preparation = WasmDistributedCoordinator::prepare(
        &runtime,
        "clearra pc score-minimals --lines 2 --board-mask 0xf03c0 --height 2 \
         --pieces 3 --patterns OOO --no-hold --score-profile tetrio \
         --spin-profile t-spins --initial-b2b 0 --workers 2",
    )
    .expect("three-piece fixed score-minimals preparation");

    let coordinator = match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
        WasmDistributedPreparation::Serial => {
            panic!("actual Geometry work, not a fixed piece threshold, must select the path")
        }
        WasmDistributedPreparation::Ready(result) => panic!(
            "score-minimals unexpectedly completed during preparation: {:?}",
            result.app_response()
        ),
    };
    assert_eq!(coordinator.worker_count(), 2);
}

#[test]
fn four_piece_high_work_score_minimals_uses_distributed_workers_with_serial_payload_parity() {
    let _resource_guard = typed_pc_distributed_test_guard();
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let base = "clearra pc score-minimals --lines 4 --board-mask 0xfc3f0fc3f0 --height 4 \
                --pieces 4 --queue IIII --no-hold --score-profile tetrio \
                --spin-profile t-spins --initial-b2b 0";
    let serial = runtime
        .run_command_text(&format!("{base} --workers 1"))
        .expect("four-piece serial score-minimals result");
    let serial_payload = serde_json::to_vec(
        serial
            .app_response()
            .product_result_payload()
            .expect("four-piece serial score-minimals payload"),
    )
    .expect("serialize four-piece serial score-minimals payload");
    drop(serial);

    let distributed = run_distributed_score_cpu(&runtime, &format!("{base} --workers 2"));
    let distributed_payload = serde_json::to_vec(
        distributed
            .app_response()
            .product_result_payload()
            .expect("four-piece distributed score-minimals payload"),
    )
    .expect("serialize four-piece distributed score-minimals payload");
    assert_eq!(distributed_payload, serial_payload);
    let report = distributed
        .search_report()
        .expect("four-piece distributed score-minimals report");
    assert_eq!(report.workers_used, 2);
    assert!(report.cpu_parallel_execution);
}

#[test]
fn four_piece_high_work_build_score_minimum_uses_distributed_workers_with_serial_payload_parity() {
    let _resource_guard = typed_pc_distributed_test_guard();
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let base = "clearra build-probability --base-mask 0xfc3f0fc3f0 \
                --target-mask 0x3c0f03c0f --height 4 --queue IIII --no-hold --no-mirror \
                --aggregate buildability --result-mode highest-score-minimum-set \
                --score-profile tetrio --initial-b2b 0";
    let serial = runtime
        .run_command_text(&format!("{base} --workers 1"))
        .expect("four-piece serial Build score-minimum result");
    let serial_payload = serde_json::to_vec(
        serial
            .app_response()
            .product_result_payload()
            .expect("four-piece serial Build score-minimum payload"),
    )
    .expect("serialize four-piece serial Build score-minimum payload");
    let distributed = run_distributed_cpu(&runtime, &format!("{base} --workers 2"));
    let distributed_payload = serde_json::to_vec(
        distributed
            .app_response()
            .product_result_payload()
            .expect("four-piece distributed Build score-minimum payload"),
    )
    .expect("serialize four-piece distributed Build score-minimum payload");
    assert_eq!(distributed_payload, serial_payload);
    let report = distributed
        .search_report()
        .expect("four-piece distributed Build score-minimum report");
    assert_eq!(report.workers_used, 2);
    assert!(report.cpu_parallel_execution);
}

#[test]
fn typed_pc_tiling_preserves_cli_workers_and_uses_product_neutral_distribution() {
    let _resource_guard = typed_pc_distributed_test_guard();
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let command = "clearra pc tiling --lines 4 --board-mask 0xfc3f --height 4 \
                   --pieces 7 --patterns IOOOOOO;OOOOOOO --no-hold \
                   --backend cpu --workers 2";

    let request = runtime
        .compile_command_text(command)
        .expect("typed pc tiling request");
    assert!(request.product_capability_contract().is_some());
    let workers = match request.command() {
        AppCommand::Pc(command) => command.query().execution_policy().workers(),
        AppCommand::Scenario(command) => command.query().execution_policy().workers(),
        other => panic!("typed pc tiling must lower to a PC search command: {other:?}"),
    };
    assert_eq!(workers, 2);
    drop(request);

    let distributed = run_distributed_cpu(&runtime, command);
    let response = distributed.app_response();
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    assert_eq!(
        response
            .result()
            .map(clearra_host_contract::AppResult::kind),
        Some("pc-tiling-family.v1")
    );
    let report = distributed
        .search_report()
        .expect("typed distributed pc tiling WASM report");
    assert_eq!(report.workers_used, 2);
    assert!(report.cpu_parallel_execution);
    assert!(report.unique_solution_count > 0);
    assert_eq!(
        report.solution_keys_materialized_count,
        report
            .unique_solution_count
            .min(clearra_app::PC_TILING_INITIAL_PAGE_LIMIT)
    );
    assert_eq!(
        report.solution_keys_complete,
        report.unique_solution_count <= clearra_app::PC_TILING_INITIAL_PAGE_LIMIT
    );
    assert_eq!(
        report.solution_page_available,
        report.unique_solution_count > clearra_app::PC_TILING_INITIAL_PAGE_LIMIT
    );
    assert_eq!(
        report.normalized_solution_keys.len(),
        report.solution_keys_materialized_count
    );
    if report.solution_page_available {
        let store = distributed
            .tiling_solution_page_store()
            .expect("typed distributed pc tiling page source");
        assert_eq!(store.len(), report.unique_solution_count);
    } else {
        assert!(distributed.tiling_solution_page_store().is_none());
    }
}

#[test]
fn typed_pc_tiling_one_root_keeps_the_product_neutral_coordinator() {
    let _resource_guard = typed_pc_distributed_test_guard();
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(12, false, false));
    let command = "clearra pc tiling --lines 2 --queue IIOOO \
                   --backend auto --allow-backend-fallback --workers 11 \
                   --cpu-warmup --gpu-warmup";

    let preparation = WasmDistributedCoordinator::prepare(&runtime, command)
        .expect("one-root typed PC tiling preparation");
    let coordinator = match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
        WasmDistributedPreparation::Serial => {
            panic!("one-root typed PC tiling must not rerun the fixed worker command serially")
        }
        WasmDistributedPreparation::Ready(result) => panic!(
            "one-root typed PC tiling completed during preparation: {:?}",
            result.app_response()
        ),
    };
    assert_eq!(coordinator.worker_count(), 2);
    assert!(coordinator.verification_required());
    assert_eq!(coordinator.progress().candidate_family_count, Some(1));
    drop(coordinator);

    let distributed = run_distributed_cpu(&runtime, command);
    let response = distributed.app_response();
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    assert_eq!(
        response
            .result()
            .map(clearra_host_contract::AppResult::kind),
        Some("pc-tiling-family.v1")
    );
    let report = distributed
        .search_report()
        .expect("one-root typed PC tiling WASM report");
    assert_eq!(report.workers_used, 2);
    assert!(report.cpu_parallel_execution);
    assert_eq!(report.unique_solution_count, 4);
    assert!(report.count_complete);
    assert!(report.solution_keys_complete);
}

#[test]
fn typed_pc_tiling_one_root_finishes_a_valid_empty_solution_family() {
    let _resource_guard = typed_pc_distributed_test_guard();
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(12, false, false));
    let command = "clearra pc tiling --lines 2 --queue IIIII --no-hold \
                   --backend auto --allow-backend-fallback --workers 11 \
                   --cpu-warmup --gpu-warmup";

    let distributed = run_distributed_cpu(&runtime, command);
    let response = distributed.app_response();
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    let report = distributed
        .search_report()
        .expect("empty one-root typed PC tiling WASM report");
    assert_eq!(report.workers_used, 2);
    assert!(report.cpu_parallel_execution);
    assert_eq!(report.unique_solution_count, 0);
    assert!(report.count_complete);
    assert!(report.solution_keys_complete);
    assert!(!report.solution_page_available);
}

#[test]
fn three_piece_fixed_count_all_reaches_the_shared_distributed_coordinator() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(12, false, false));
    let preparation = WasmDistributedCoordinator::prepare(
        &runtime,
        "clearra pc --lines 2 --board-mask 0xf03c0 --height 2 --pieces 3 \
         --queue OOO --no-hold --backend cpu --workers 11",
    )
    .expect("three-piece fixed CountAll preparation");

    let coordinator = match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
        WasmDistributedPreparation::Serial => {
            panic!("CountAll must not have a product- or piece-count serial exclusion")
        }
        WasmDistributedPreparation::Ready(result) => panic!(
            "CountAll unexpectedly completed during preparation: {:?}",
            result.app_response()
        ),
    };
    assert_eq!(coordinator.worker_count(), 11);
}

#[test]
fn four_piece_high_work_count_all_uses_distributed_workers_with_serial_parity() {
    let _resource_guard = typed_pc_distributed_test_guard();
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let base = "clearra pc --lines 4 --board-mask 0xfc3f0fc3f0 --height 4 --pieces 4 \
                --patterns P7 --no-hold --backend cpu";
    let serial = runtime
        .run_command_text(&format!("{base} --workers 1"))
        .expect("four-piece high-work serial CountAll result");
    let serial_report = serial
        .search_report()
        .expect("four-piece high-work serial CountAll report");
    let serial_projection = (
        serial_report.packing_candidate_count,
        serial_report.geometry_candidate_family_count.clone(),
        serial_report.packing_candidate_set_digest.clone(),
        serial_report.packing_candidate_keys.clone(),
        serial_report.unique_solution_count,
        serial_report.normalized_solution_set_hash.clone(),
        serial_report.normalized_solution_keys.clone(),
        serial_report.count_complete,
        serial_report.solution_keys_complete,
    );
    drop(serial);

    let distributed = run_distributed_cpu(&runtime, &format!("{base} --workers 2"));
    let report = distributed
        .search_report()
        .expect("four-piece high-work distributed CountAll report");
    let distributed_projection = (
        report.packing_candidate_count,
        report.geometry_candidate_family_count.clone(),
        report.packing_candidate_set_digest.clone(),
        report.packing_candidate_keys.clone(),
        report.unique_solution_count,
        report.normalized_solution_set_hash.clone(),
        report.normalized_solution_keys.clone(),
        report.count_complete,
        report.solution_keys_complete,
    );
    assert_eq!(distributed_projection, serial_projection);
    assert_eq!(report.workers_used, 2);
    assert!(report.cpu_parallel_execution);
}

#[test]
fn typed_pc_minimals_distributed_geometry_completes_with_the_same_exact_family() {
    let _resource_guard = typed_pc_distributed_test_guard();
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    const COMMAND: &str = "clearra pc minimals --lines 4 --board-mask 0xfc3f --height 4 \
        --pieces 7 --patterns IOOOOOO;OOOOOOO --no-hold --backend cpu";

    let serial = runtime
        .run_command_text(&format!("{COMMAND} --workers 1"))
        .expect("serial typed pc minimals result");
    let distributed = run_distributed_cpu(&runtime, &format!("{COMMAND} --workers 2"));
    let serial_report = serial.search_report().expect("serial minimals report");
    let distributed_report = distributed
        .search_report()
        .expect("distributed minimals report");

    assert_eq!(distributed.app_response().status(), AppStatus::Success);
    assert_eq!(distributed_report.workers_used, 2);
    assert!(distributed_report.cpu_parallel_execution);
    assert_eq!(
        distributed_report.normalized_solution_set_hash,
        serial_report.normalized_solution_set_hash
    );
    assert_eq!(
        distributed_report.unique_solution_count,
        serial_report.unique_solution_count
    );
    assert!(distributed.product_page_source_owner().is_some());
    let payload = distributed
        .app_response()
        .product_result_payload()
        .expect("distributed minimals product payload");
    assert_eq!(payload.contract(), "pc.minimals");
    assert_eq!(payload.result_kind(), "pc-minimum-cover.v2");
}

#[test]
fn distributed_minimum_completion_reuses_cancellable_owned_app_cursor() {
    let _resource_guard = typed_pc_distributed_test_guard();
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    const COMMAND: &str = "clearra pc minimals --lines 4 --board-mask 0xfc3f --height 4 \
        --pieces 7 --patterns IOOOOOO;OOOOOOO --no-hold --backend cpu --workers 2";
    let expected = run_distributed_cpu(&runtime, COMMAND);
    let coordinator = completed_distributed_cpu_source(&runtime, COMMAND);
    assert!(coordinator.requires_cooperative_completion());
    let mut completion = coordinator
        .into_cooperative_completion(2)
        .expect("staged completion");
    assert!(matches!(
        completion.advance(0).unwrap(),
        WasmDistributedCompletionAdvance::Pending
    ));
    // The first advance exposes the ordinary postprocess/exact boundary and
    // cannot synchronously complete the expensive proof.
    assert!(matches!(
        completion.advance(1).unwrap(),
        WasmDistributedCompletionAdvance::Pending
    ));
    let actual = loop {
        match completion.advance(64).expect("owned exact slice") {
            WasmDistributedCompletionAdvance::Pending => {}
            WasmDistributedCompletionAdvance::Completed(result) => break result,
            WasmDistributedCompletionAdvance::Cancelled => panic!("unexpected cancel"),
        }
    };
    assert_eq!(actual.app_response().status(), AppStatus::Success);
    assert_eq!(
        actual.app_response().product_result_payload(),
        expected.app_response().product_result_payload()
    );
    assert!(actual.product_page_source_owner().is_some());
    assert!(
        completion.advance(1).is_err(),
        "finished authority cannot be replayed"
    );

    let coordinator = completed_distributed_cpu_source(&runtime, COMMAND);
    let mut cancelled = coordinator
        .into_cooperative_completion(2)
        .expect("cancellable completion");
    assert!(matches!(
        cancelled.advance(1).unwrap(),
        WasmDistributedCompletionAdvance::Pending
    ));
    cancelled.cancel();
    assert!(matches!(
        cancelled.advance(1).unwrap(),
        WasmDistributedCompletionAdvance::Cancelled
    ));
}

#[test]
fn typed_pc_minimals_distributed_finish_preserves_a_complete_empty_portfolio() {
    let _resource_guard = typed_pc_distributed_test_guard();
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let distributed = run_distributed_cpu(
        &runtime,
        "clearra pc minimals --lines 4 --queue IIIIIIIIIO --no-hold \
         --backend cpu --workers 2",
    );

    let response = distributed.app_response();
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    let payload = response
        .product_result_payload()
        .expect("distributed complete-empty minimum-cover payload");
    let ProductResultPayloadContent::CoveragePortfolio(page) = payload.content() else {
        panic!("expected distributed complete-empty coverage portfolio")
    };
    assert_eq!(page.optimal_cardinality(), "0");
    assert_eq!(page.known_alternative_count(), "1");
    assert_eq!(page.total_alternative_count(), Some("1"));
    assert!(page.enumeration_complete());
    assert!(page.members().is_empty());
    assert_eq!(page.canonical_selection(), None);
    assert_eq!(page.canonical_witness(), None);
}

#[test]
fn typed_pc_path_distributed_geometry_preserves_the_complete_replay_family() {
    let _resource_guard = typed_pc_distributed_test_guard();
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    const COMMAND: &str = "clearra pc path --lines 4 --board-mask 0xfc3f --height 4 \
        --pieces 7 --patterns IOOOOOO;OOOOOOO --no-hold --backend cpu";

    let serial = runtime
        .run_command_text(&format!("{COMMAND} --workers 1"))
        .expect("serial typed pc path result");
    let distributed = run_distributed_cpu(&runtime, &format!("{COMMAND} --workers 2"));
    assert_eq!(
        distributed.app_response().status(),
        AppStatus::Success,
        "{:?}",
        distributed.app_response()
    );

    let serial_report = serial.search_report().expect("serial pc.path report");
    let distributed_report = distributed
        .search_report()
        .expect("distributed pc.path report");
    assert_build_probability_semantics_match(serial_report, distributed_report);
    assert_eq!(distributed_report.workers_used, 2);
    assert!(distributed_report.cpu_parallel_execution);

    let serial_payload = serial
        .app_response()
        .product_result_payload()
        .expect("serial pc.path product payload");
    let distributed_payload = distributed
        .app_response()
        .product_result_payload()
        .expect("distributed pc.path product payload");
    assert_eq!(serial_payload.contract(), "pc.path");
    assert_eq!(serial_payload.result_kind(), "pc-path-family.v2");
    assert_eq!(distributed_payload.contract(), serial_payload.contract());
    assert_eq!(
        distributed_payload.result_kind(),
        serial_payload.result_kind()
    );
    assert_eq!(distributed_payload.content(), serial_payload.content());

    let ProductResultPayloadContent::PcPathFamily(family) = distributed_payload.content() else {
        panic!("distributed pc.path must retain its complete replay-family payload")
    };
    assert!(family.complete());
    assert_eq!(family.witness_count(), family.witnesses().len().to_string());
    assert_eq!(family.canonical_witness(), family.witnesses().first());
}

#[test]
fn typed_pc_path_distributed_cooperative_pages_match_eager_family() {
    let _resource_guard = typed_pc_distributed_test_guard();
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    const COMMAND: &str = "clearra pc path --lines 2 --board-mask 0xfc3f0 --height 2 \
        --pieces 2 --patterns OO;IO --no-hold --backend cpu";
    let eager = runtime
        .run_command_text(&format!("{COMMAND} --workers 1"))
        .expect("eager baseline");
    let source = completed_distributed_cpu_source(&runtime, &format!("{COMMAND} --workers 2"));
    assert!(
        source.requires_cooperative_completion(),
        "browser replay must not take eager finish"
    );
    let mut completion = source
        .into_cooperative_completion(2)
        .expect("paged continuation");
    let mut completed = None;
    let mut advances = 0;
    for _ in 0..4096 {
        advances += 1;
        match completion.advance(128).expect("cooperative replay advance") {
            WasmDistributedCompletionAdvance::Pending => {}
            WasmDistributedCompletionAdvance::Completed(result) => {
                completed = Some(result);
                break;
            }
            WasmDistributedCompletionAdvance::Cancelled => panic!("uncancelled replay"),
        }
    }
    assert!(
        advances > 1,
        "replay source construction must yield to the host"
    );
    let completed = completed.expect("bounded tiny replay completion");
    assert_eq!(
        completed.app_response().status(),
        AppStatus::Success,
        "{:?}",
        completed.app_response()
    );
    let expected = eager
        .app_response()
        .product_result_payload()
        .expect("eager payload");
    let ProductResultPayloadContent::PcPathFamily(expected) = expected.content() else {
        panic!("eager pc path family");
    };
    let Some(clearra_app::ProductPageSourceOwner::PcReplay(source)) =
        completed.product_page_source_owner()
    else {
        panic!("browser completion must own its paged exact graph");
    };
    assert_eq!(source.witness_count().to_string(), expected.witness_count());
    let geometry_count = source.geometry_count();
    let mut store = clearra_app::PcReplayPageStore::new(std::sync::Arc::clone(source));
    let mut actual = Vec::new();
    let control = clearra_core_domain::execution_cancellation::ExecutionControl::default();
    for geometry in 1..=geometry_count {
        let first = store
            .page(geometry, 1, &control)
            .expect("first replay member page");
        let page_count: usize = first
            .metadata
            .member_page_count
            .parse()
            .expect("page count");
        actual.extend(first.witnesses);
        for member in 2..=page_count {
            actual.extend(
                store
                    .page(geometry, member, &control)
                    .expect("replay member page")
                    .witnesses,
            );
        }
    }
    assert_eq!(
        actual.as_slice(),
        expected.witnesses(),
        "all geometries and members preserve the eager exact set"
    );
}

#[test]
fn typed_pc_chance_distributed_completion_preserves_probability_evidence() {
    let _resource_guard = typed_pc_distributed_test_guard();
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    const COMMAND: &str = "clearra pc chance --lines 4 --board-mask 0xfc3f --height 4 \
        --pieces 7 --patterns IOOOOOO;OOOOOOO --no-hold --backend cpu";

    let serial = runtime
        .run_command_text(&format!("{COMMAND} --workers 1"))
        .expect("serial typed pc chance result");
    let distributed = run_distributed_cpu(&runtime, &format!("{COMMAND} --workers 2"));
    assert_eq!(
        distributed.app_response().status(),
        AppStatus::Success,
        "{:?}",
        distributed.app_response()
    );
    let serial_report = serial.search_report().expect("serial chance report");
    let distributed_report = distributed
        .search_report()
        .expect("distributed chance report");

    assert_build_probability_semantics_match(serial_report, distributed_report);

    assert_eq!(
        distributed
            .app_response()
            .result()
            .map(clearra_host_contract::AppResult::kind),
        Some("pc-probability.v2")
    );
    assert_eq!(distributed_report.workers_used, 2);
    assert!(distributed_report.cpu_parallel_execution);
    assert_eq!(
        distributed_report.coverage_probability,
        serial_report.coverage_probability
    );
    assert_eq!(
        distributed_report.covered_pattern_count,
        serial_report.covered_pattern_count
    );
    assert_eq!(
        distributed_report.total_possible_pattern_count,
        serial_report.total_possible_pattern_count
    );
    assert!(distributed_report.probability_complete);
}

#[test]
fn finesse_score_remains_serial_when_multiple_workers_are_requested() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let preparation = WasmDistributedCoordinator::prepare(
        &runtime,
        "clearra finesse score --initial-mask 0 --height 4 \
         --placements I:spawn:3:0 --queue I --no-hold --workers 2",
    )
    .expect("finesse score preparation");

    assert!(matches!(preparation, WasmDistributedPreparation::Serial));
}

#[test]
fn tiling_only_finesse_search_is_rejected_before_distribution() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let error = match WasmDistributedCoordinator::prepare(
        &runtime,
        "clearra build-probability --base-mask 0 --target-mask 0xfc3f3fcff \
         --height 4 --queue OOOOOOO --no-hold --no-mirror --tiling-only \
         --finesse inputs --pattern-knowledge both --workers 2",
    ) {
        Err(error) => error,
        Ok(_) => panic!("tiling-only finesse must not enter a root-only worker path"),
    };

    assert_eq!(error.code(), "E_WASM_COMMAND_INVALID_VALUE");
}

#[test]
fn unavailable_gpu_distributed_preparation_is_an_explicit_cpu_fallback() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let command = "clearra pc --lines 4 --count unique --backend gpu \
                   --allow-backend-fallback --workers 2 --queue IOTSZJLIOTS";
    let preparation =
        WasmDistributedCoordinator::prepare(&runtime, command).expect("GPU CPU fallback");

    match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => {
            assert_eq!(coordinator.mode(), WasmDistributedMode::CpuMulti);
            assert_eq!(
                coordinator.requested_backend(),
                WasmDistributedRequestedBackend::Gpu
            );
            assert_eq!(
                coordinator.preparation_fallback_reason(),
                WasmDistributedFallbackReason::GpuDeviceNotFound
            );
        }
        _ => panic!("fallback-enabled GPU request must preserve the distributed CPU path"),
    }

    let result = run_distributed_cpu(&runtime, command);
    assert!(result.app_response().backend_report().fallback_used());
    assert_eq!(
        result
            .app_response()
            .backend_report()
            .backend_fallback_reason(),
        Some("gpu_device_not_found")
    );
    assert!(result.webgpu_backend().fallback_used);
    assert_eq!(
        result.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("gpu_device_not_found")
    );
}

#[test]
fn unavailable_hybrid_distributed_preparation_selects_cpu_without_fallback() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, false, false));
    let command = "clearra pc --lines 4 --count unique --backend hybrid \
                   --no-backend-fallback --workers 2 --queue IOTSZJLIOTS";
    let preparation =
        WasmDistributedCoordinator::prepare(&runtime, command).expect("hybrid CPU selection");

    match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => {
            assert_eq!(coordinator.mode(), WasmDistributedMode::CpuMulti);
            assert_eq!(
                coordinator.requested_backend(),
                WasmDistributedRequestedBackend::Hybrid
            );
            assert_eq!(
                coordinator.preparation_fallback_reason(),
                WasmDistributedFallbackReason::None
            );
        }
        _ => panic!("hybrid request must preserve the distributed CPU path"),
    }

    let result = run_distributed_cpu(&runtime, command);
    assert_eq!(
        result.app_response().backend_report().backend_selected(),
        "wasm-cpu"
    );
    assert!(!result.app_response().backend_report().fallback_used());
    assert!(!result.webgpu_backend().fallback_used);
    assert_eq!(result.webgpu_backend().fallback_backend, None);
    assert_eq!(
        result.webgpu_backend().webgpu_unavailable_reason.as_deref(),
        Some("gpu_device_not_found")
    );
}

#[test]
fn build_probability_gpu_distributed_preparation_uses_kernel_fallback() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let command = "clearra build-probability --base-mask 0x0 \
                   --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI \
                   --no-hold --no-mirror --tiling-only --backend gpu \
                   --allow-backend-fallback --workers 2";
    let preparation = WasmDistributedCoordinator::prepare(&runtime, command)
        .expect("build-probability GPU fallback preparation");

    match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => {
            assert_eq!(coordinator.mode(), WasmDistributedMode::CpuMulti);
            assert_eq!(
                coordinator.requested_backend(),
                WasmDistributedRequestedBackend::Gpu
            );
            assert_eq!(
                coordinator.preparation_fallback_reason(),
                WasmDistributedFallbackReason::GpuKernelUnavailable
            );
        }
        _ => panic!("fallback-enabled build-probability must preserve the CPU distributed path"),
    }
}

#[test]
fn build_probability_gpu_distributed_preparation_defers_denied_fallback_to_serial_contract() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let command = "clearra build-probability --base-mask 0x0 \
                   --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI \
                   --no-hold --no-mirror --tiling-only --backend gpu \
                   --no-backend-fallback --workers 2";
    let preparation = WasmDistributedCoordinator::prepare(&runtime, command)
        .expect("serial unsupported contract preparation");

    assert!(matches!(preparation, WasmDistributedPreparation::Serial));
}

#[test]
fn build_probability_hybrid_distributed_preparation_selects_cpu_without_fallback() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let command = "clearra build-probability --base-mask 0x0 \
                   --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI \
                   --no-hold --no-mirror --tiling-only --backend hybrid \
                   --no-backend-fallback --workers 2";
    let preparation = WasmDistributedCoordinator::prepare(&runtime, command)
        .expect("hybrid build-probability preparation");

    match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => {
            assert_eq!(coordinator.mode(), WasmDistributedMode::CpuMulti);
            assert_eq!(
                coordinator.requested_backend(),
                WasmDistributedRequestedBackend::Hybrid
            );
            assert_eq!(
                coordinator.preparation_fallback_reason(),
                WasmDistributedFallbackReason::None
            );
        }
        _ => panic!("hybrid build-probability must preserve the CPU distributed path"),
    }
}

#[cfg(feature = "webgpu-search")]
#[test]
fn gpu_multi_request_selects_the_webgpu_distributed_product_path() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(4, true, false));
    let preparation = WasmDistributedCoordinator::prepare(
        &runtime,
        "clearra pc --lines 4 --count unique --backend gpu --workers 2 --queue IOTSZJLIOTS",
    )
    .expect("WebGPU distributed preparation");

    match preparation {
        WasmDistributedPreparation::Coordinator(coordinator) => {
            assert_eq!(coordinator.mode(), WasmDistributedMode::WebGpuMulti);
            assert_eq!(coordinator.worker_count(), 2);
            assert_eq!(
                coordinator.requested_backend(),
                WasmDistributedRequestedBackend::Gpu
            );
            assert_eq!(
                coordinator.preparation_fallback_reason(),
                WasmDistributedFallbackReason::None
            );
        }
        _ => panic!("4L two-worker GPU request must select gpu-multi"),
    }
}
