#![cfg(not(target_arch = "wasm32"))]

use clearra_host_contract::{AppStatus, ProductResultPayloadContent};
use clearra_wasm::WasmCommandRuntime;

#[test]
fn cli_build_minimum_empty_success_reaches_typed_host_and_page_owner() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        let result = WasmCommandRuntime::default().run_command_text(
            "clearra build cover --base-mask 0 --target-mask 0xf --height 4 --queue O --hold empty --queue-knowledge oracle --objective min-cover --workers 1"
        ).expect("Build command executes");
        assert_eq!(result.app_response().status(), AppStatus::Success, "{result:?}");
        let payload = result.app_response().product_result_payload().expect("typed empty result");
        let ProductResultPayloadContent::BuildCoveragePortfolioV2(cover) = payload.content() else { panic!("Build cover payload"); };
        assert_eq!(cover.source_candidate_count(), "0");
        assert_eq!(cover.selected_candidate_count(), "0");
        assert_eq!(cover.required_pattern_count(), "0");
        assert_eq!(cover.union_probability(), "0");
        assert!(cover.completeness().complete());
        assert!(cover.page_source_available());
        assert!(result.product_page_source_owner().is_some());
    }).unwrap().join().expect("empty result contract");
}
