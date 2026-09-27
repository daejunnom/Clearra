use clearra_wasm::{
    WasmCommandRuntime, WasmDistributedCoordinator, WasmDistributedPreparation,
    WasmDistributedProducerAdvance, WasmDistributedVerifierRuntime, WasmHostCapabilities,
};
#[test]
fn recovery_browser_parallel_wire_equals_serial_and_rejects_repeated_partial() {
    let command="clearra recovery build --start-mask 0 --middle-mask 0xf --result-mask 0xc030 --height 8 --first-supply [IJLOSTZ] --second-supply [IJLOSTZ] --allow-piece-exchange --hold --preserve-b2b --workers 4";
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(8, false, false));
    let expected = runtime
        .run_command_text(&command.replace("--workers 4", "--workers 1"))
        .unwrap();
    let WasmDistributedPreparation::Coordinator(mut c) =
        WasmDistributedCoordinator::prepare(&runtime, command).unwrap()
    else {
        panic!("recovery must use distributed worker path")
    };
    assert_eq!(c.worker_count(), 4);
    let init = c.worker_initialization().unwrap();
    let mut worker = WasmDistributedVerifierRuntime::prepare_forward(&runtime, &init).unwrap();
    let mut receipts = Vec::new();
    loop {
        match c.advance_producer(32, 64).unwrap() {
            WasmDistributedProducerAdvance::Batch(bytes) => {
                let reply = worker.consume(&bytes).unwrap();
                assert!(!reply.has_pending_work);
                receipts.push(reply.partial.unwrap());
            }
            WasmDistributedProducerAdvance::Pending => break,
            WasmDistributedProducerAdvance::Completed => break,
            _ => panic!("unexpected producer state"),
        }
    }
    assert_eq!(receipts.len(), 2);
    for bytes in receipts.iter().rev() {
        c.absorb_partial(bytes).unwrap();
    }
    assert!(c.absorb_partial(&receipts[0]).is_err());
    assert!(matches!(
        c.advance_producer(32, 64).unwrap(),
        WasmDistributedProducerAdvance::Completed
    ));
    assert!(worker.finish().unwrap().is_empty());
    let actual = c.finish(3).unwrap();
    assert_eq!(
        actual.app_response().product_result_payload(),
        expected.app_response().product_result_payload()
    );
}
