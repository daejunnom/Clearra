#![cfg(not(target_arch = "wasm32"))]

use clearra_host_contract::AppStatus;
use clearra_wasm::WasmCommandRuntime;

const DEFAULT_NATIVE_TEST_STACK_BYTES: usize = 2 * 1024 * 1024;

#[test]
fn actual_pc_minimals_wasm_command_runtime_fits_the_default_two_mib_stack() {
    let handle = std::thread::Builder::new()
        .name("pc-minimals-wasm-runtime-2mib".to_owned())
        .stack_size(DEFAULT_NATIVE_TEST_STACK_BYTES)
        .spawn(|| {
            let result = WasmCommandRuntime::default()
                .run_command_text(
                    "clearra pc minimals --lines 1 --board-mask 0x3f --height 1 \
                     --pieces 1 --queue I --hold empty",
                )
                .expect("actual pc.minimals WASM command runtime");

            assert_eq!(result.app_response().status(), AppStatus::Success);
            assert!(result.product_page_source_owner().is_some());
            let payload = result
                .app_response()
                .product_result_payload()
                .expect("pc.minimals product payload");
            assert_eq!(payload.contract(), "pc.minimals");
            assert_eq!(payload.result_kind(), "pc-minimum-cover.v2");
        })
        .expect("spawn two MiB pc.minimals runtime thread");

    handle
        .join()
        .expect("actual pc.minimals runtime exceeded the two MiB stack contract");
}
