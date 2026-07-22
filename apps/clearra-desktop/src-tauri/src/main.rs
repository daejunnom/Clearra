mod commands;

use commands::{
    cancel_job, get_job_events, prewarm_search_backend, run_request, start_job, validate_request,
    DesktopBridgeState,
};

fn main() {
    tauri::Builder::default()
        .manage(DesktopBridgeState::default())
        .invoke_handler(tauri::generate_handler![
            run_request,
            validate_request,
            start_job,
            cancel_job,
            get_job_events,
            prewarm_search_backend
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Clearra desktop host");
}
