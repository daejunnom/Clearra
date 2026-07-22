use clearra_pc_graph::request::GpuDeviceSelection;
use std::time::Instant;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuSearchWarmupReport {
    connected: bool,
    device_index: Option<u8>,
    device_name: Option<String>,
    session_reused: bool,
    initialization_elapsed_ns: u64,
    unavailable_reason: Option<String>,
}

impl GpuSearchWarmupReport {
    pub const fn connected(&self) -> bool {
        self.connected
    }

    pub const fn device_index(&self) -> Option<u8> {
        self.device_index
    }

    pub fn device_name(&self) -> Option<&str> {
        self.device_name.as_deref()
    }

    pub fn unavailable_reason(&self) -> Option<&str> {
        self.unavailable_reason.as_deref()
    }

    pub const fn session_reused(&self) -> bool {
        self.session_reused
    }

    pub const fn initialization_elapsed_ns(&self) -> u64 {
        self.initialization_elapsed_ns
    }

    #[cfg(feature = "webgpu-search")]
    fn ready(device_index: u8, device_name: String, session_reused: bool) -> Self {
        Self {
            connected: true,
            device_index: Some(device_index),
            device_name: Some(device_name),
            session_reused,
            initialization_elapsed_ns: 0,
            unavailable_reason: None,
        }
    }

    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            connected: false,
            device_index: None,
            device_name: None,
            session_reused: false,
            initialization_elapsed_ns: 0,
            unavailable_reason: Some(reason.into()),
        }
    }

    fn with_elapsed(mut self, started_at: Instant) -> Self {
        self.initialization_elapsed_ns =
            u64::try_from(started_at.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self
    }
}

/// Prepares reusable device-wide GPU state without allocating search buffers.
///
/// Adapter/device discovery, the queue, shader module, and compute pipeline are
/// retained by the session cache. Problem-specific immutable buffers and layer
/// scratch stay deferred because their contents or sizes are unknown until a
/// concrete packing request exists.
pub fn prewarm_gpu_search(device: GpuDeviceSelection) -> GpuSearchWarmupReport {
    #[cfg(feature = "webgpu-search")]
    {
        return pollster::block_on(prewarm_gpu_search_async(device));
    }

    #[cfg(not(feature = "webgpu-search"))]
    {
        let started_at = Instant::now();
        let _ = device;
        GpuSearchWarmupReport::unavailable("webgpu_search_not_connected").with_elapsed(started_at)
    }
}

pub async fn prewarm_gpu_search_async(device: GpuDeviceSelection) -> GpuSearchWarmupReport {
    let started_at = Instant::now();
    #[cfg(feature = "webgpu-search")]
    {
        return match clearra_webgpu::WebGpuGeometryExactCoverBackend::connect_selected(
            webgpu_selection(device),
        )
        .await
        {
            clearra_webgpu::WebGpuGeometryExactCoverSessionOutcome::Connected(session) => {
                let report = GpuSearchWarmupReport::ready(
                    session.adapter().index(),
                    session.adapter().name().to_owned(),
                    session.reused(),
                );
                session.recycle();
                report.with_elapsed(started_at)
            }
            clearra_webgpu::WebGpuGeometryExactCoverSessionOutcome::Unavailable(unavailable) => {
                GpuSearchWarmupReport::unavailable(unavailable.reason()).with_elapsed(started_at)
            }
        };
    }

    #[cfg(not(feature = "webgpu-search"))]
    {
        let _ = device;
        GpuSearchWarmupReport::unavailable("webgpu_search_not_connected").with_elapsed(started_at)
    }
}

#[cfg(feature = "webgpu-search")]
pub(crate) fn connect_webgpu_session_for_device(
    device: GpuDeviceSelection,
) -> clearra_webgpu::WebGpuGeometryExactCoverSessionOutcome {
    connect_webgpu_session(webgpu_selection(device))
}

#[cfg(feature = "webgpu-search")]
pub(crate) fn connect_webgpu_session(
    selection: clearra_webgpu::WebGpuAdapterSelection,
) -> clearra_webgpu::WebGpuGeometryExactCoverSessionOutcome {
    use std::sync::{Mutex, OnceLock};

    static CONNECTION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _connection = CONNECTION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    pollster::block_on(clearra_webgpu::WebGpuGeometryExactCoverBackend::connect_selected(selection))
}

#[cfg(feature = "webgpu-search")]
pub(crate) fn take_prepared_webgpu_session_for_device(
    device: GpuDeviceSelection,
) -> Option<clearra_webgpu::WebGpuGeometryExactCoverSession> {
    clearra_webgpu::WebGpuGeometryExactCoverBackend::take_prepared_session(webgpu_selection(device))
}

#[cfg(feature = "webgpu-search")]
fn webgpu_selection(device: GpuDeviceSelection) -> clearra_webgpu::WebGpuAdapterSelection {
    match device {
        GpuDeviceSelection::Auto => clearra_webgpu::WebGpuAdapterSelection::Auto,
        GpuDeviceSelection::Index(index) => clearra_webgpu::WebGpuAdapterSelection::Index(index),
    }
}
