use clearra_app::{encode_ctk3_compact, Ctk3Color, Ctk3Document, Ctk3Page, Ctk3Piece};
use clearra_host_contract::{
    AppCommandKind, AppStatus, JobEvent, ProductResultPayloadContent, QueryEnvelope,
};
use clearra_wasm::{
    wasm_worker_event_to_host_contract, ProductPageSourceOwner, WasmCommandRuntime,
    WasmHostCapabilities, WasmWorkerAdvanceStatus, WasmWorkerJobEvent, WasmWorkerJobRuntime,
};

#[test]
fn wasm_runtime_does_not_spawn_process() {
    let mut runtime = WasmWorkerJobRuntime::default();
    let job_id = runtime
        .start_job("clearra pc --lines 2 | clearra verify")
        .expect("job starts before parser phase");
    while !runtime
        .advance_job(job_id, 64)
        .expect("parser failure becomes event")
        .is_terminal()
    {}
    let events = runtime.drain_events(job_id);

    assert!(events.iter().any(|event| matches!(
        wasm_worker_event_to_host_contract(event),
        JobEvent::Failed(report)
            if report.diagnostics().iter().any(|diagnostic|
                diagnostic.code() == "E_WASM_PROCESS_SEMANTICS_FORBIDDEN")
    )));
}

#[test]
fn cli_gui_wasm_share_app_request_schema() {
    let request = WasmCommandRuntime::default()
        .compile_command_text("clearra pc --lines 2 --backend cpu")
        .expect("AppRequest");

    assert_eq!(request.query(), &QueryEnvelope::PcOpening);
    assert_eq!(request.backend_policy().backend_requested(), "cpu");
}

#[test]
fn wasm_worker_event_maps_to_host_contract_job_event() {
    let mut runtime = WasmWorkerJobRuntime::default();
    let job_id = runtime.start_job("clearra verify kicks").expect("job");
    while !runtime
        .advance_job(job_id, 64)
        .expect("job runs")
        .is_terminal()
    {}
    let events = runtime.drain_events(job_id);

    assert!(events.iter().any(|event| matches!(
        wasm_worker_event_to_host_contract(event),
        JobEvent::Progress(_)
    )));
    assert!(events.iter().any(|event| matches!(
        wasm_worker_event_to_host_contract(event),
        JobEvent::Completed(_)
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        WasmWorkerJobEvent::FinalResponse {
            response,
            ..
        } if response.command() == Some(AppCommandKind::VerifyKicks)
            && response.status() == AppStatus::Success
            && response.result().is_some_and(|result| result.kind() == "verify-kicks")
    )));

    let cancelled_job = runtime.start_job("clearra verify kicks").expect("job");
    assert_eq!(
        runtime
            .advance_job(cancelled_job, 64)
            .expect("prepare active computation"),
        WasmWorkerAdvanceStatus::Pending
    );
    assert_eq!(
        runtime.status(cancelled_job),
        Some(clearra_wasm::WasmWorkerJobStatus::Running)
    );
    let cancellation = runtime
        .cancellation_token(cancelled_job)
        .expect("active computation scope");
    runtime.cancel_job(cancelled_job).expect("cancel job");
    assert!(cancellation.is_cancelled());
    assert!(runtime.drain_events(cancelled_job).iter().any(|event| {
        matches!(
            event,
            WasmWorkerJobEvent::Cancelled {
                scope_released: true,
                ..
            }
        )
    }));
}

// Exercise the public command runtime, not a synthetic coverage/result fixture.
const LEFT_I: &str = "ctk1|initial=0000000000000000|placements=I:000000000000000f";
const RIGHT_I: &str = "ctk1|initial=0000000000000000|placements=I:00000000000003c0";
const CENTER_I: &str = "ctk1|initial=0000000000000000|placements=I:0000000000000078";

fn selected_document(masks: &[u64]) -> String {
    let pages = masks
        .iter()
        .map(|mask| {
            let cells = (0..10)
                .map(|x| {
                    if mask & (1_u64 << x) == 0 {
                        Ctk3Color::Empty
                    } else {
                        Ctk3Color::Piece(Ctk3Piece::I)
                    }
                })
                .collect();
            Ctk3Page::new(1, cells)
        })
        .collect();
    encode_ctk3_compact(&Ctk3Document::new(10, pages)).expect("selected I drawings")
}

#[test]
fn build_mirror_source_and_pins_preserve_exact_sets_and_union_probability() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(1, false, false));
    for (target, mirror, expected) in [
        ("0xf", "--include-mirror", vec![LEFT_I, RIGHT_I]),
        ("0xf", "--no-mirror", vec![LEFT_I]),
        ("0x78", "--include-mirror", vec![CENTER_I]),
    ] {
        let source = runtime
            .run_command_text(&format!(
                "clearra build-probability --base-mask 0 --target-mask {target} --height 4 \
                 --queue I --no-hold --aggregate buildability --result-mode all-solutions \
                 {mirror} --backend cpu --no-backend-fallback --workers 1"
            ))
            .expect("complete Build source execution");
        assert_eq!(source.app_response().status(), AppStatus::Success);
        let report = source.search_report().expect("complete source report");
        let mut keys = report.normalized_solution_keys.clone();
        keys.sort();
        let mut expected = expected;
        expected.sort();
        assert_eq!(keys, expected);
        assert_eq!(report.unique_solution_count, keys.len());
        assert!(report.count_complete && report.probability_complete);
        assert!(report.solution_keys_complete && !report.resource_truncated);
        assert_eq!(report.covered_pattern_count, 1);
        assert_eq!(report.materialized_pattern_count, 1);
        assert_eq!(report.coverage_probability, "1");

        if target != "0xf" || mirror != "--include-mirror" {
            continue;
        }
        // Both candidates cover the same queue. A single mirrored pin must
        // select that drawing; requiring both must increase the minimum to two.
        for (masks, expected_pins) in [
            (vec![0x3c0_u64], vec![RIGHT_I]),
            (vec![0xf_u64, 0x3c0_u64], vec![LEFT_I, RIGHT_I]),
        ] {
            let document = selected_document(&masks);
            let pinned = runtime
                .run_command_text(&format!(
                    "clearra build pinned-minimals --base-mask 0 --target-mask 0xf --height 4 \
                     --queue I --no-hold --objective min-cover --queue-knowledge oracle \
                     --required-format ctk3 --required-document {document} \
                     --expected-source-set-hash {} --backend cpu --no-backend-fallback --workers 1",
                    report.normalized_solution_set_hash
                ))
                .expect("selected drawings revalidated against the full source");
            assert_eq!(pinned.app_response().status(), AppStatus::Success);
            let payload = pinned
                .app_response()
                .product_result_payload()
                .expect("typed minimum payload");
            let ProductResultPayloadContent::BuildCoveragePortfolioV2(minimum) = payload.content()
            else {
                panic!("expected a Build minimum portfolio");
            };
            assert_eq!(minimum.source_candidate_count(), "2");
            assert_eq!(
                minimum.selected_candidate_count(),
                expected_pins.len().to_string()
            );
            assert_eq!(minimum.union_probability(), "1");
            assert!(minimum.completeness().complete());
            let Some(ProductPageSourceOwner::CoveragePortfolio(owner)) =
                pinned.product_page_source_owner()
            else {
                panic!("minimum must retain its real member page source");
            };
            let mut actual = owner
                .canonical_candidate_keys_owned()
                .expect("complete selected member keys");
            actual.sort();
            let mut expected_pins = expected_pins;
            expected_pins.sort();
            assert_eq!(actual, expected_pins);
        }
    }
}
