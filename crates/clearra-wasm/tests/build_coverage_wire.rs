//! Run the same producer through typed completion and the browser JSON boundary.
use clearra_app::{encode_ctk3_compact, Ctk3Color, Ctk3Document, Ctk3Page, Ctk3Piece};
use clearra_host_contract::{AppStatus, ProductResultPayloadContent};
use clearra_wasm::{
    serialize_distributed_final_events, ProductPageSourceOwner, WasmCommandRuntime,
    WasmHostCapabilities, WasmWorkerJobRuntime,
};

fn document(masks: &[u64]) -> String {
    let pages = masks
        .iter()
        .map(|mask| {
            Ctk3Page::new(
                1,
                (0..10)
                    .map(|x| {
                        if mask & (1_u64 << x) == 0 {
                            Ctk3Color::Empty
                        } else {
                            Ctk3Color::Piece(Ctk3Piece::I)
                        }
                    })
                    .collect(),
            )
        })
        .collect();
    encode_ctk3_compact(&Ctk3Document::new(10, pages)).unwrap()
}

fn member_keys(owner: &ProductPageSourceOwner) -> Vec<String> {
    let ProductPageSourceOwner::CoveragePortfolio(owner) = owner else {
        panic!("Build must retain a real coverage page owner");
    };
    let mut keys = owner.canonical_candidate_keys_owned().unwrap();
    keys.sort();
    keys
}

#[test]
fn build_coverage_wire_matches_typed_completion_and_retains_page_ownership() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(3, false, false));
    let source = runtime
        .run_command_text(
            "clearra build-probability --base-mask 0 --target-mask 0xf --height 4 \
         --queue I --no-hold --include-mirror --backend cpu --workers 1",
        )
        .unwrap();
    let digest = &source.search_report().unwrap().normalized_solution_set_hash;
    for workers in [1, 3] {
        for masks in [vec![], vec![0x3c0], vec![0xf, 0x3c0]] {
            let suffix = if masks.is_empty() {
                String::new()
            } else {
                format!(" --required-format ctk3 --required-document {} --expected-source-set-hash {digest}", document(&masks))
            };
            let command = format!(
                "clearra build {} --base-mask 0 --target-mask 0xf --height 4 \
                 --queue I --no-hold --objective min-cover --queue-knowledge oracle \
                 --backend cpu --workers {workers}{suffix}",
                if masks.is_empty() {
                    "cover"
                } else {
                    "pinned-minimals"
                }
            );
            let typed = runtime.run_command_text(&command).unwrap();
            assert_eq!(typed.app_response().status(), AppStatus::Success);
            let expected = typed.app_response().product_result_payload().unwrap();
            let ProductResultPayloadContent::BuildCoveragePortfolioV2(portfolio) =
                expected.content()
            else {
                panic!("typed Build portfolio");
            };
            assert_eq!(portfolio.source_candidate_count(), "2");
            assert_eq!(
                portfolio.selected_candidate_count(),
                masks.len().max(1).to_string()
            );
            assert_eq!(portfolio.union_probability(), "1");
            let expected_keys = member_keys(typed.product_page_source_owner().unwrap());
            let direct_json = serialize_distributed_final_events(41, &typed).unwrap();
            let direct: serde_json::Value = serde_json::from_str(&direct_json).unwrap();
            let expected_json = serde_json::to_value(expected).unwrap();
            let terminal = direct
                .as_array()
                .unwrap()
                .iter()
                .find(|event| event["event"] == "final_response")
                .unwrap();
            assert_eq!(
                terminal["response"]["product_result_payload"],
                expected_json
            );

            let mut worker = WasmWorkerJobRuntime::new(
                WasmCommandRuntime::default()
                    .with_host_capabilities(WasmHostCapabilities::new(3, false, false)),
            );
            // Reuse the controller after draining its first result and releasing
            // the old page owner; stale terminal data must not satisfy run two.
            for _ in 0..2 {
                let job = worker.start_job(&command).unwrap();
                let mut completed = false;
                for _ in 0..10_000 {
                    if worker.advance_job(job, 256).unwrap().is_terminal() {
                        completed = true;
                        break;
                    }
                }
                assert!(completed, "bounded worker execution must terminate");
                let wire = if worker.has_completed_governed_events() {
                    let output = worker.drain_governed_events_json(job).unwrap();
                    assert_eq!(
                        member_keys(output.completed_product_page_source_owner().unwrap()),
                        expected_keys
                    );
                    output.into_json()
                } else {
                    let output = worker.drain_events_json(job).unwrap();
                    let owner = worker.take_completed_product_page_source_owner().unwrap();
                    assert_eq!(member_keys(&owner), expected_keys);
                    output
                };
                let events: serde_json::Value = serde_json::from_str(&wire).unwrap();
                let finals: Vec<_> = events
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|event| event["event"] == "final_response")
                    .collect();
                assert_eq!(finals.len(), 1, "one terminal response per job");
                assert_eq!(finals[0]["response"]["status"], "success");
                assert_eq!(
                    finals[0]["response"]["product_result_payload"],
                    expected_json
                );
            }
        }
    }
}
