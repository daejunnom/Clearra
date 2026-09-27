use clearra_host_contract::{AppStatus,ProductResultPayloadContent};
use clearra_wasm::{WasmCommandRuntime,WasmHostCapabilities,WasmWorkerJobRuntime,serialize_distributed_final_events};
#[test]
fn recovery_build_public_route_matches_typed_and_browser_json() {
    let command="clearra recovery build --start-mask 0 --middle-mask 0xf --result-mask 0xc030 --height 8 --first-supply O --second-supply I --allow-piece-exchange --no-hold";
    let runtime=WasmCommandRuntime::default().with_host_capabilities(WasmHostCapabilities::new(1,false,false));
    let result=runtime.run_command_text(command).unwrap();
    assert_eq!(result.app_response().status(),AppStatus::Success);
    let payload=result.app_response().product_result_payload().unwrap();
    let ProductResultPayloadContent::RecoveryBuild(report)=payload.content() else {panic!("new typed result")};
    assert_eq!(report.recovery_count,"1");assert_eq!(report.examples[0].exchange_balance,vec![-1,0,0,1,0,0,0]);
    let expected=serde_json::to_value(payload).unwrap();
    let wire:serde_json::Value=serde_json::from_str(&serialize_distributed_final_events(7,&result).unwrap()).unwrap();
    let terminal=wire.as_array().unwrap().iter().find(|event|event["event"]=="final_response").unwrap();
    assert_eq!(terminal["response"]["product_result_payload"],expected);
    let mut worker=WasmWorkerJobRuntime::new(runtime);
    for _ in 0..2 {
        let job=worker.start_job(command).unwrap();let mut done=false;
        for _ in 0..10000 {if worker.advance_job(job,256).unwrap().is_terminal() {done=true;break;}}
        assert!(done);
        let json=if worker.has_completed_governed_events() {worker.drain_governed_events_json(job).unwrap().into_json()} else {worker.drain_events_json(job).unwrap()};
        let events:serde_json::Value=serde_json::from_str(&json).unwrap();
        let finals:Vec<_>=events.as_array().unwrap().iter().filter(|event|event["event"]=="final_response").collect();
        assert_eq!(finals.len(),1);assert_eq!(finals[0]["response"]["product_result_payload"],expected);
    }
}
