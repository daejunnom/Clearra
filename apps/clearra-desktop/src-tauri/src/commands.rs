use std::sync::Mutex;

use clearra_gui_host::DesktopTauriCommandBridge;

#[derive(Default)]
pub struct DesktopBridgeState {
    bridge: Mutex<DesktopTauriCommandBridge>,
}

#[tauri::command]
pub fn run_request(
    state: tauri::State<'_, DesktopBridgeState>,
    request_json: String,
) -> Result<String, String> {
    let bridge = state.bridge.lock().map_err(|error| error.to_string())?;
    bridge
        .run_request(&request_json)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn validate_request(
    state: tauri::State<'_, DesktopBridgeState>,
    request_json: String,
) -> Result<String, String> {
    let bridge = state.bridge.lock().map_err(|error| error.to_string())?;
    bridge
        .validate_request(&request_json)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn start_job(
    state: tauri::State<'_, DesktopBridgeState>,
    request_json: String,
) -> Result<u64, String> {
    let mut bridge = state.bridge.lock().map_err(|error| error.to_string())?;
    bridge
        .start_job(&request_json)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn cancel_job(state: tauri::State<'_, DesktopBridgeState>, job_id: u64) -> Result<(), String> {
    let bridge = state.bridge.lock().map_err(|error| error.to_string())?;
    bridge.cancel_job(job_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_job_events(
    state: tauri::State<'_, DesktopBridgeState>,
    job_id: u64,
) -> Result<String, String> {
    let mut bridge = state.bridge.lock().map_err(|error| error.to_string())?;
    bridge
        .get_job_events(job_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn prewarm_search_backend(gpu_device: Option<u8>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        clearra_gui_host::prewarm_search_backend(gpu_device)
    })
    .await
    .map_err(|error| error.to_string())
}
