//! Real host execution followed by the browser event serializer, not a mock DTO.
use clearra_host_contract::{AppStatus, ProductResultPayloadContent};
use clearra_wasm::{serialize_distributed_final_events, WasmCommandRuntime, WasmHostCapabilities};

#[test]
fn automatic_early_request_mode_and_two_or_three_role_witnesses_survive_wasm_json() {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(1, false, false));
    for early in [2_u32, 3] {
        let top = 0xc03_u128 << (20 * early);
        let all = (0..=early).fold(0_u128, |board, row| board | (0xc03_u128 << (20 * row)));
        let mut command = format!("clearra recovery boundary --initial-board-mask 0 --stage-one-board-mask 0x{top:x} --target-board-mask 0x{all:x} --height {} --queue {} --stage-one-count 1 --no-hold --max-early-placements auto --preserve-b2b --role-mask 1:0x{top:x}",
            2 * (early + 1), "O".repeat((early + 1) as usize));
        for index in 0..early {
            command.push_str(&format!(
                " --role-mask {}:0x{:x}",
                index + 2,
                0xc03_u128 << (20 * index)
            ));
        }
        // Reuse the real runtime: no result from the previous command may leak.
        let result = runtime.run_command_text(&command).unwrap();
        assert_eq!(result.app_response().status(), AppStatus::Success);
        let payload = result.app_response().product_result_payload().unwrap();
        let ProductResultPayloadContent::BoundaryRecovery(boundary) = payload.content() else {
            panic!("typed boundary result");
        };
        assert_eq!(boundary.status, "non-pc-recovery");
        assert_eq!(boundary.early_placement_limit_mode.as_deref(), Some("auto"));
        assert_eq!(boundary.max_early_placements, early as u8);
        assert_eq!(boundary.borrowed_stage_two_count, early as usize);
        assert!(boundary.steps.iter().all(|step| step.b2b_active_after));
        let wire = serialize_distributed_final_events(early as u64, &result).unwrap();
        let events: serde_json::Value = serde_json::from_str(&wire).unwrap();
        let final_events: Vec<_> = events
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["event"] == "final_response")
            .collect();
        assert_eq!(final_events.len(), 1);
        assert_eq!(
            final_events[0]["response"]["product_result_payload"],
            serde_json::to_value(payload).unwrap()
        );
    }
}
