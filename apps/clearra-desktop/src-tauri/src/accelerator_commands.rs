//! Desktop's in-process, explicit accelerator-asset management bridge.
//! Search never invokes these commands. One download slot owns one cancellation
//! token and bounded progress counters; the CLI keeps signing and file authority.

use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};

const RETAINED_COMPLETIONS: usize = 16;

struct ActiveDownload {
    id: u64,
    cancelled: Arc<AtomicBool>,
    transferred: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    completion: Arc<Mutex<Option<Result<String, String>>>>,
}

struct CompletedDownload {
    id: u64,
    transferred: u64,
    total: u64,
    result: Result<String, String>,
}

#[derive(Default)]
struct DownloadSlot {
    next_id: u64,
    active: Option<ActiveDownload>,
    completed: VecDeque<CompletedDownload>,
}

impl DownloadSlot {
    fn reap_completed(&mut self) -> Result<(), String> {
        let finished = if let Some(active) = &self.active {
            let result = active
                .completion
                .lock()
                .map_err(|error| error.to_string())?
                .clone();
            result.map(|result| CompletedDownload {
                id: active.id,
                transferred: active.transferred.load(Ordering::Acquire),
                total: active.total.load(Ordering::Acquire),
                result,
            })
        } else {
            None
        };
        if let Some(finished) = finished {
            self.completed.push_back(finished);
            if self.completed.len() > RETAINED_COMPLETIONS {
                self.completed.pop_front();
            }
            self.active = None;
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct AcceleratorState {
    slot: Mutex<DownloadSlot>,
}

#[tauri::command]
pub async fn accelerator_asset_action(
    state: tauri::State<'_, AcceleratorState>,
    product: String,
    action: String,
    profile: String,
) -> Result<String, String> {
    if !matches!(action.as_str(), "check" | "status" | "remove") {
        return Err("accelerator: use the explicit download operation".to_owned());
    }
    {
        let mut slot = state.slot.lock().map_err(|error| error.to_string())?;
        slot.reap_completed()?;
        if slot.active.is_some() {
            return Err("accelerator: another download is active".to_owned());
        }
    }
    tauri::async_runtime::spawn_blocking(move || {
        let cancelled = AtomicBool::new(false);
        clearra_cli::run_native_accelerator_action(
            &product,
            &action,
            &profile,
            &cancelled,
            &mut |_, _| {},
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn accelerator_asset_start_download(
    state: tauri::State<'_, AcceleratorState>,
    product: String,
    profile: String,
) -> Result<u64, String> {
    let mut slot = state.slot.lock().map_err(|error| error.to_string())?;
    slot.reap_completed()?;
    if slot.active.is_some() {
        return Err("accelerator: another download is active".to_owned());
    }
    slot.next_id = slot
        .next_id
        .checked_add(1)
        .ok_or_else(|| "accelerator: download operation ID overflow".to_owned())?;
    let id = slot.next_id;
    let cancelled = Arc::new(AtomicBool::new(false));
    let transferred = Arc::new(AtomicU64::new(0));
    let total = Arc::new(AtomicU64::new(0));
    let completion = Arc::new(Mutex::new(None));
    slot.active = Some(ActiveDownload {
        id,
        cancelled: Arc::clone(&cancelled),
        transferred: Arc::clone(&transferred),
        total: Arc::clone(&total),
        completion: Arc::clone(&completion),
    });
    drop(slot);
    tauri::async_runtime::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            clearra_cli::run_native_accelerator_action(
                &product,
                "download",
                &profile,
                &cancelled,
                &mut |bytes, size| {
                    transferred.store(bytes, Ordering::Release);
                    total.store(size, Ordering::Release);
                },
            )
        }))
        .unwrap_or_else(|_| Err("accelerator: download worker failed".to_owned()));
        if let Ok(mut guard) = completion.lock() {
            *guard = Some(result);
        }
    });
    Ok(id)
}

#[tauri::command]
pub fn accelerator_asset_progress(
    state: tauri::State<'_, AcceleratorState>,
    operation_id: u64,
) -> Result<String, String> {
    let mut slot = state.slot.lock().map_err(|error| error.to_string())?;
    slot.reap_completed()?;
    let (transferred, total, completion) = if let Some(index) = slot
        .completed
        .iter()
        .position(|completed| completed.id == operation_id)
    {
        let completed = slot.completed.remove(index).expect("position is in bounds");
        (
            completed.transferred,
            completed.total,
            Some(completed.result),
        )
    } else if let Some(active) = slot
        .active
        .as_ref()
        .filter(|active| active.id == operation_id)
    {
        (
            active.transferred.load(Ordering::Acquire),
            active.total.load(Ordering::Acquire),
            None,
        )
    } else {
        return Err("accelerator: unknown download operation".to_owned());
    };
    let (result, error) = match completion {
        Some(Ok(result)) => (Some(result), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    };
    Ok(serde_json::json!({
        "operation_id": operation_id,
        "done": result.is_some() || error.is_some(),
        "transferred_bytes": transferred,
        "total_bytes": total,
        "result": result,
        "error": error,
    })
    .to_string())
}

#[tauri::command]
pub fn accelerator_asset_cancel(
    state: tauri::State<'_, AcceleratorState>,
    operation_id: u64,
) -> Result<(), String> {
    let slot = state.slot.lock().map_err(|error| error.to_string())?;
    let active = slot
        .active
        .as_ref()
        .filter(|active| active.id == operation_id)
        .ok_or_else(|| "accelerator: download operation is not active".to_owned())?;
    active.cancelled.store(true, Ordering::Release);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_download_is_reaped_without_reusing_its_operation_id() {
        let mut slot = DownloadSlot::default();
        slot.next_id = 3;
        slot.active = Some(ActiveDownload {
            id: 3,
            cancelled: Arc::new(AtomicBool::new(false)),
            transferred: Arc::new(AtomicU64::new(1)),
            total: Arc::new(AtomicU64::new(1)),
            completion: Arc::new(Mutex::new(Some(Ok("{}".to_owned())))),
        });
        slot.reap_completed().expect("reap completed download");
        assert!(slot.active.is_none());
        assert_eq!(
            slot.completed.front().map(|completed| completed.id),
            Some(3)
        );
        assert_eq!(
            slot.completed
                .front()
                .map(|completed| completed.transferred),
            Some(1)
        );
        assert_eq!(slot.next_id, 3);
    }

    #[test]
    fn completed_result_survives_another_window_reaping_the_slot() {
        let mut slot = DownloadSlot::default();
        slot.active = Some(ActiveDownload {
            id: 1,
            cancelled: Arc::new(AtomicBool::new(false)),
            transferred: Arc::new(AtomicU64::new(7)),
            total: Arc::new(AtomicU64::new(7)),
            completion: Arc::new(Mutex::new(Some(Ok("done".to_owned())))),
        });
        slot.reap_completed()
            .expect("another window checks the slot");
        assert_eq!(
            slot.completed.pop_front().map(|completed| completed.result),
            Some(Ok("done".to_owned()))
        );
    }
}
