#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use commands::{
    cancel_job, get_job_events, prewarm_search_backend, product_page_get, product_page_next,
    product_page_release, run_request, start_job, validate_request, DesktopBridgeState,
};

fn main() {
    let _native_build_probability_registration =
        clearra_gui_host::register_system_native_build_probability_host();
    tauri::Builder::default()
        .manage(DesktopBridgeState::default())
        .invoke_handler(tauri::generate_handler![
            run_request,
            validate_request,
            start_job,
            cancel_job,
            get_job_events,
            product_page_next,
            product_page_get,
            product_page_release,
            prewarm_search_backend
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Clearra desktop host");
}
