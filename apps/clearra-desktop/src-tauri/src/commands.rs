use std::sync::{Arc, Mutex};

use clearra_gui_host::{DesktopTauriCommandBridge, GuiJobCancelHandle, GuiJobCancelToken};

#[derive(Default)]
struct ProductPageOperationState {
    next_id: u64,
    active: Option<(u64, GuiJobCancelHandle)>,
}

#[derive(Default)]
struct ProductPageOperationSlot {
    state: Mutex<ProductPageOperationState>,
}

impl ProductPageOperationSlot {
    fn begin(&self) -> Result<(u64, GuiJobCancelToken), String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        if state.active.is_some() {
            return Err("a desktop product page operation is already active".to_owned());
        }
        state.next_id = state
            .next_id
            .checked_add(1)
            .ok_or_else(|| "desktop product page operation id overflow".to_owned())?;
        let operation_id = state.next_id;
        let token = GuiJobCancelToken::new();
        state.active = Some((operation_id, token.handle()));
        Ok((operation_id, token))
    }

    fn cancel(&self) -> Result<(), String> {
        let state = self.state.lock().map_err(|error| error.to_string())?;
        if let Some((_, handle)) = state.active.as_ref() {
            handle.cancel();
        }
        Ok(())
    }

    fn finish(&self, operation_id: u64) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        if state
            .active
            .as_ref()
            .is_some_and(|(active_id, _)| *active_id == operation_id)
        {
            state.active = None;
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct DesktopBridgeState {
    bridge: Arc<Mutex<DesktopTauriCommandBridge>>,
    product_page_operation: Arc<ProductPageOperationSlot>,
}

#[tauri::command]
pub fn run_request(
    state: tauri::State<'_, DesktopBridgeState>,
    request_json: String,
) -> Result<String, String> {
    state.product_page_operation.cancel()?;
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
    state.product_page_operation.cancel()?;
    let mut bridge = state.bridge.lock().map_err(|error| error.to_string())?;
    bridge
        .start_job(&request_json)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn cancel_job(state: tauri::State<'_, DesktopBridgeState>, job_id: u64) -> Result<(), String> {
    let mut bridge = state.bridge.lock().map_err(|error| error.to_string())?;
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
pub async fn product_page_next(
    state: tauri::State<'_, DesktopBridgeState>,
    maximum_work_steps: u64,
) -> Result<String, String> {
    let (operation_id, cancellation) = state.product_page_operation.begin()?;
    let bridge = Arc::clone(&state.bridge);
    let product_page_operation = Arc::clone(&state.product_page_operation);
    tauri::async_runtime::spawn_blocking(move || {
        let mut bridge = match bridge.lock() {
            Ok(bridge) => bridge,
            Err(error) => {
                product_page_operation.finish(operation_id)?;
                return Err(error.to_string());
            }
        };
        let result = bridge
            .product_page_next_with_cancel(maximum_work_steps, &mut || cancellation.is_cancelled())
            .map_err(|error| error.to_string());
        product_page_operation.finish(operation_id)?;
        drop(bridge);
        result
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn product_page_get(
    state: tauri::State<'_, DesktopBridgeState>,
    alternative_index: String,
    member_page_number: String,
) -> Result<String, String> {
    let (operation_id, cancellation) = state.product_page_operation.begin()?;
    let bridge = Arc::clone(&state.bridge);
    let product_page_operation = Arc::clone(&state.product_page_operation);
    tauri::async_runtime::spawn_blocking(move || {
        let mut bridge = match bridge.lock() {
            Ok(bridge) => bridge,
            Err(error) => {
                product_page_operation.finish(operation_id)?;
                return Err(error.to_string());
            }
        };
        let result = bridge
            .product_page_get_with_cancel(&alternative_index, &member_page_number, &mut || {
                cancellation.is_cancelled()
            })
            .map_err(|error| error.to_string());
        product_page_operation.finish(operation_id)?;
        drop(bridge);
        result
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn product_page_release(
    state: tauri::State<'_, DesktopBridgeState>,
) -> Result<(), String> {
    state.product_page_operation.cancel()?;
    let bridge = Arc::clone(&state.bridge);
    tauri::async_runtime::spawn_blocking(move || {
        let mut bridge = bridge.lock().map_err(|error| error.to_string())?;
        bridge.product_page_release();
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::ProductPageOperationSlot;

    #[test]
    fn product_page_release_signal_reaches_the_active_operation() {
        let slot = ProductPageOperationSlot::default();
        let (operation_id, cancellation) = slot.begin().expect("begin page operation");
        assert!(!cancellation.is_cancelled());
        assert!(slot.begin().is_err());

        slot.cancel().expect("cancel page operation");
        assert!(cancellation.is_cancelled());
        slot.finish(operation_id).expect("finish page operation");

        let (replacement_id, replacement) = slot.begin().expect("begin replacement operation");
        assert!(!replacement.is_cancelled());
        assert!(replacement_id > operation_id);
        slot.finish(replacement_id)
            .expect("finish replacement operation");
    }
}

#[tauri::command]
pub async fn prewarm_search_backend(gpu_device: Option<u8>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        clearra_gui_host::prewarm_search_backend(gpu_device)
    })
    .await
    .map_err(|error| error.to_string())
}
