#![cfg(not(target_arch = "wasm32"))]

use clearra_host_contract::{AppStatus, ProductResultPayloadContent};
use clearra_wasm::WasmCommandRuntime;

#[test]
fn build_replay_accepts_only_query_authorized_original_and_mirror_targets() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let base = "clearra build-probability --base-mask 0 --target-mask 0xf --height 4 \
                --hold empty --queue I --aggregate buildability --rule srs-plus \
                --no-build-dependency-dag --result-mode complete-replay-paths \
                --backend cpu --no-backend-fallback --workers 1";
            for (option, expected_mirror) in [
                ("--no-mirror", None),
                ("--include-mirror", Some("0x00000000000003c0")),
            ] {
                let result = WasmCommandRuntime::default()
                    .run_command_text(&format!("{base} {option}"))
                    .expect("GUI Build replay command executes");
                assert_eq!(
                    result.app_response().status(),
                    AppStatus::Success,
                    "{result:?}"
                );
                let payload = result
                    .app_response()
                    .product_result_payload()
                    .expect("Build replay payload");
                let ProductResultPayloadContent::BuildPathFamily(family) = payload.content() else {
                    panic!("Build replay family required");
                };
                assert_eq!(family.target_terminal_board_mask(), "0x000000000000000f");
                assert_eq!(family.mirrored_terminal_board_mask(), expected_mirror);
                assert!(!family.witnesses().is_empty());
                assert!(family.witnesses().iter().all(|witness| {
                    let terminal = witness
                        .steps()
                        .last()
                        .unwrap()
                        .board_after_line_clear_mask();
                    terminal == family.target_terminal_board_mask()
                        || Some(terminal) == expected_mirror
                }));
                if let Some(mirror) = expected_mirror {
                    assert!(family.witnesses().iter().any(|witness| witness
                        .steps()
                        .last()
                        .unwrap()
                        .board_after_line_clear_mask()
                        == mirror));
                }
            }
        })
        .expect("test stack")
        .join()
        .expect("Build replay contract");
}
