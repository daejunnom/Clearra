use clearra_host_contract::{AppStatus, ProductResultPayloadContent};
use clearra_wasm::{
    WasmCommandRuntime, WasmHostCapabilities, WasmWorkerJobRuntime,
    serialize_distributed_final_events,
};
#[test]
fn recovery_build_public_route_matches_typed_and_browser_json() {
    let command = "clearra recovery build --start-mask 0 --middle-mask 0xf --result-mask 0xc030 --height 8 --first-supply O --second-supply I --allow-piece-exchange --no-hold";
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(1, false, false));
    let result = runtime.run_command_text(command).unwrap();
    assert_eq!(result.app_response().status(), AppStatus::Success);
    let payload = result.app_response().product_result_payload().unwrap();
    let ProductResultPayloadContent::RecoveryBuild(report) = payload.content() else {
        panic!("new typed result")
    };
    assert_eq!(report.recovery_count, "1");
    assert_eq!(
        report.examples[0].exchange_balance,
        vec![-1, 0, 0, 1, 0, 0, 0]
    );
    let expected = serde_json::to_value(payload).unwrap();
    let wire: serde_json::Value =
        serde_json::from_str(&serialize_distributed_final_events(7, &result).unwrap()).unwrap();
    let terminal = wire
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["event"] == "final_response")
        .unwrap();
    assert_eq!(terminal["response"]["product_result_payload"], expected);
    let mut worker = WasmWorkerJobRuntime::new(runtime);
    for _ in 0..2 {
        let job = worker.start_job(command).unwrap();
        let mut done = false;
        for _ in 0..10000 {
            if worker.advance_job(job, 256).unwrap().is_terminal() {
                done = true;
                break;
            }
        }
        assert!(done);
        let json = if worker.has_completed_governed_events() {
            worker.drain_governed_events_json(job).unwrap().into_json()
        } else {
            worker.drain_events_json(job).unwrap()
        };
        let events: serde_json::Value = serde_json::from_str(&json).unwrap();
        let finals: Vec<_> = events
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["event"] == "final_response")
            .collect();
        assert_eq!(finals.len(), 1);
        assert_eq!(finals[0]["response"]["product_result_payload"], expected);
    }
}

#[test]
fn recovery_build_distributed_protocol_matches_serial_and_requires_all_pairs() {
    use clearra_wasm::{
        WasmDistributedCoordinator, WasmDistributedPreparation, WasmDistributedProducerAdvance,
        WasmDistributedVerifierRuntime,
    };
    let base = "clearra recovery build --start-mask 0 --middle-mask 0xf --result-mask 0xc030 --height 8 --first-supply [IO] --second-supply [IO] --allow-piece-exchange";
    // Host capability describes logical processors, not granted compute slots.
    // Match the user's 12-logical-processor host and keep the UI reserve intact.
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(12, false, false));
    for hold in ["--no-hold", "--hold"] {
        let expected = runtime
            .run_command_text(&format!("{base} {hold} --workers 1"))
            .unwrap();
        assert_eq!(expected.app_response().status(), AppStatus::Success);
        for requested in [2, 4, 11] {
            let WasmDistributedPreparation::Coordinator(mut c) =
                WasmDistributedCoordinator::prepare(
                    &runtime,
                    &format!("{base} {hold} --workers {requested}"),
                )
                .unwrap()
            else {
                panic!("must enter worker pool")
            };
            assert_eq!(c.worker_count(), requested);
            let init = c
                .worker_initialization()
                .expect("same existing binary worker protocol");
            let mut workers = (0..requested - 1)
                .map(|_| WasmDistributedVerifierRuntime::prepare_forward(&runtime, &init).unwrap())
                .collect::<Vec<_>>();
            let mut results = Vec::new();
            loop {
                match c.advance_producer(64, 32).unwrap() {
                    WasmDistributedProducerAdvance::Batch(bytes) => {
                        let index = results.len() % workers.len();
                        let w = &mut workers[index];
                        let mut part = w.consume(&bytes).unwrap();
                        while part.has_pending_work {
                            assert!(part.partial.is_none());
                            part = w.continue_work().unwrap();
                        }
                        results.push(part.partial.unwrap());
                    }
                    WasmDistributedProducerAdvance::Pending => break,
                    _ => panic!("cannot finish before receiving issued pairs"),
                }
            }
            assert!(results.len() > 1);
            for result in results.into_iter().rev() {
                c.absorb_partial(&result).unwrap();
            }
            assert_eq!(
                c.advance_producer(64, 32).unwrap(),
                WasmDistributedProducerAdvance::Completed
            );
            for w in &mut workers {
                assert!(w.finish().unwrap().is_empty());
            }
            let actual = c.finish(requested).unwrap();
            assert_eq!(actual.app_response().status(), AppStatus::Success);
            assert_eq!(
                actual.app_response().product_result_payload(),
                expected.app_response().product_result_payload()
            );
        }
    }
}
