use std::{
    cmp::Ordering,
    fmt,
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WebGpuAdapterSelection {
    #[default]
    Auto,
    Index(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebGpuAdapterDeviceType {
    DiscreteGpu,
    IntegratedGpu,
    VirtualGpu,
    Other,
    Cpu,
}

impl WebGpuAdapterDeviceType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DiscreteGpu => "discrete-gpu",
            Self::IntegratedGpu => "integrated-gpu",
            Self::VirtualGpu => "virtual-gpu",
            Self::Other => "other",
            Self::Cpu => "cpu",
        }
    }

    const fn performance_tier(self) -> u8 {
        match self {
            Self::DiscreteGpu => 5,
            Self::IntegratedGpu => 4,
            Self::Other => 3,
            Self::VirtualGpu => 2,
            Self::Cpu => 0,
        }
    }
}

impl From<wgpu::DeviceType> for WebGpuAdapterDeviceType {
    fn from(value: wgpu::DeviceType) -> Self {
        match value {
            wgpu::DeviceType::DiscreteGpu => Self::DiscreteGpu,
            wgpu::DeviceType::IntegratedGpu => Self::IntegratedGpu,
            wgpu::DeviceType::VirtualGpu => Self::VirtualGpu,
            wgpu::DeviceType::Cpu => Self::Cpu,
            wgpu::DeviceType::Other => Self::Other,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebGpuAdapterSummary {
    index: u8,
    name: String,
    vendor: u32,
    device: u32,
    device_type: WebGpuAdapterDeviceType,
    backend: String,
    pci_bus_id: String,
    max_compute_invocations_per_workgroup: u32,
    max_compute_workgroups_per_dimension: u32,
    max_storage_buffer_binding_size: u64,
    max_buffer_size: u64,
}

impl WebGpuAdapterSummary {
    pub const fn index(&self) -> u8 {
        self.index
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn vendor(&self) -> u32 {
        self.vendor
    }

    pub const fn device(&self) -> u32 {
        self.device
    }

    pub const fn device_type(&self) -> WebGpuAdapterDeviceType {
        self.device_type
    }

    pub fn backend(&self) -> &str {
        &self.backend
    }

    pub fn pci_bus_id(&self) -> &str {
        &self.pci_bus_id
    }

    pub const fn max_storage_buffer_binding_size(&self) -> u64 {
        self.max_storage_buffer_binding_size
    }

    fn performance_cmp(&self, other: &Self) -> Ordering {
        self.device_type
            .performance_tier()
            .cmp(&other.device_type.performance_tier())
            .then_with(|| {
                self.max_compute_invocations_per_workgroup
                    .cmp(&other.max_compute_invocations_per_workgroup)
            })
            .then_with(|| {
                self.max_compute_workgroups_per_dimension
                    .cmp(&other.max_compute_workgroups_per_dimension)
            })
            .then_with(|| {
                self.max_storage_buffer_binding_size
                    .cmp(&other.max_storage_buffer_binding_size)
            })
            .then_with(|| self.max_buffer_size.cmp(&other.max_buffer_size))
            .then_with(|| other.index.cmp(&self.index))
    }
}

#[derive(Clone)]
pub(crate) struct SelectedWebGpuAdapter {
    pub(crate) adapter: Arc<wgpu::Adapter>,
    pub(crate) summary: WebGpuAdapterSummary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebGpuAdapterSelectionError {
    NoAdapters,
    IndexNotFound(u8),
    SoftwareAdapterSelected(u8),
    AdapterCountOverflow,
}

impl fmt::Display for WebGpuAdapterSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for WebGpuAdapterSelectionError {}

pub(crate) async fn select_adapter(
    selection: WebGpuAdapterSelection,
) -> Result<SelectedWebGpuAdapter, WebGpuAdapterSelectionError> {
    let selected = match selection {
        WebGpuAdapterSelection::Auto => preferred_adapter().await?,
        WebGpuAdapterSelection::Index(index) => adapter_inventory()
            .await?
            .into_iter()
            .find(|candidate| candidate.summary.index == index)
            .ok_or(WebGpuAdapterSelectionError::IndexNotFound(index))?,
    };
    if selected.summary.device_type == WebGpuAdapterDeviceType::Cpu {
        return Err(WebGpuAdapterSelectionError::SoftwareAdapterSelected(
            selected.summary.index,
        ));
    }
    Ok(selected)
}

pub async fn enumerate_adapter_summaries(
) -> Result<Vec<WebGpuAdapterSummary>, WebGpuAdapterSelectionError> {
    Ok(adapter_inventory()
        .await?
        .into_iter()
        .map(|candidate| candidate.summary)
        .collect())
}

pub async fn select_adapter_summary(
    selection: WebGpuAdapterSelection,
) -> Result<WebGpuAdapterSummary, WebGpuAdapterSelectionError> {
    Ok(select_adapter(selection).await?.summary)
}

fn runtime_instance() -> &'static wgpu::Instance {
    static INSTANCE: OnceLock<wgpu::Instance> = OnceLock::new();
    INSTANCE.get_or_init(|| {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = product_backends();
        wgpu::Instance::new(descriptor)
    })
}

const fn product_backends() -> wgpu::Backends {
    #[cfg(target_arch = "wasm32")]
    {
        return wgpu::Backends::BROWSER_WEBGPU;
    }
    #[cfg(target_os = "windows")]
    {
        return wgpu::Backends::DX12;
    }
    #[cfg(target_os = "macos")]
    {
        return wgpu::Backends::METAL;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return wgpu::Backends::VULKAN;
    }
    #[allow(unreachable_code)]
    wgpu::Backends::PRIMARY
}

fn adapter_inventory_cache() -> &'static Mutex<Option<Vec<SelectedWebGpuAdapter>>> {
    static INVENTORY: OnceLock<Mutex<Option<Vec<SelectedWebGpuAdapter>>>> = OnceLock::new();
    INVENTORY.get_or_init(|| Mutex::new(None))
}

fn preferred_adapter_cache() -> &'static Mutex<Option<SelectedWebGpuAdapter>> {
    static ADAPTER: OnceLock<Mutex<Option<SelectedWebGpuAdapter>>> = OnceLock::new();
    ADAPTER.get_or_init(|| Mutex::new(None))
}

async fn preferred_adapter() -> Result<SelectedWebGpuAdapter, WebGpuAdapterSelectionError> {
    if let Some(cached) = preferred_adapter_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .cloned()
    {
        return Ok(cached);
    }
    let selected = adapter_inventory()
        .await?
        .into_iter()
        .find(|candidate| candidate.summary.device_type != WebGpuAdapterDeviceType::Cpu)
        .ok_or(WebGpuAdapterSelectionError::NoAdapters)?;
    cache_preferred_adapter(&selected);
    Ok(selected)
}

fn cache_preferred_adapter(adapter: &SelectedWebGpuAdapter) {
    let mut cached = preferred_adapter_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cached.get_or_insert_with(|| adapter.clone());
}

fn selected_adapter(index: u8, adapter: wgpu::Adapter) -> SelectedWebGpuAdapter {
    let info = adapter.get_info();
    let limits = adapter.limits();
    SelectedWebGpuAdapter {
        adapter: Arc::new(adapter),
        summary: WebGpuAdapterSummary {
            index,
            name: info.name,
            vendor: info.vendor,
            device: info.device,
            device_type: info.device_type.into(),
            backend: format!("{:?}", info.backend).to_ascii_lowercase(),
            pci_bus_id: info.device_pci_bus_id,
            max_compute_invocations_per_workgroup: limits.max_compute_invocations_per_workgroup,
            max_compute_workgroups_per_dimension: limits.max_compute_workgroups_per_dimension,
            max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
            max_buffer_size: limits.max_buffer_size,
        },
    }
}

async fn adapter_inventory() -> Result<Vec<SelectedWebGpuAdapter>, WebGpuAdapterSelectionError> {
    if let Some(cached) = adapter_inventory_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .cloned()
    {
        return Ok(cached);
    }

    let mut adapters = runtime_instance()
        .enumerate_adapters(product_backends())
        .await;
    if adapters.is_empty() {
        if let Ok(adapter) = runtime_instance()
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
                ..Default::default()
            })
            .await
        {
            adapters.push(adapter);
        }
    }
    if adapters.is_empty() {
        return Err(WebGpuAdapterSelectionError::NoAdapters);
    }
    let mut discovered = adapters
        .into_iter()
        .enumerate()
        .map(|(index, adapter)| {
            let index = u8::try_from(index)
                .map_err(|_| WebGpuAdapterSelectionError::AdapterCountOverflow)?;
            Ok(selected_adapter(index, adapter))
        })
        .collect::<Result<Vec<_>, _>>()?;
    discovered.sort_by(|left, right| right.summary.performance_cmp(&left.summary));
    for (index, candidate) in discovered.iter_mut().enumerate() {
        candidate.summary.index =
            u8::try_from(index).map_err(|_| WebGpuAdapterSelectionError::AdapterCountOverflow)?;
    }
    let mut cached = adapter_inventory_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let inventory = cached.get_or_insert_with(|| discovered.clone());
    if let Some(preferred) = inventory
        .iter()
        .find(|candidate| candidate.summary.device_type != WebGpuAdapterDeviceType::Cpu)
    {
        cache_preferred_adapter(preferred);
    }
    Ok(inventory.clone())
}

#[cfg(test)]
mod tests {
    use super::{WebGpuAdapterDeviceType, WebGpuAdapterSummary};

    fn summary(index: u8, device_type: WebGpuAdapterDeviceType) -> WebGpuAdapterSummary {
        WebGpuAdapterSummary {
            index,
            name: format!("adapter-{index}"),
            vendor: 0,
            device: 0,
            device_type,
            backend: "test".to_owned(),
            pci_bus_id: String::new(),
            max_compute_invocations_per_workgroup: 256,
            max_compute_workgroups_per_dimension: 65_535,
            max_storage_buffer_binding_size: 1 << 30,
            max_buffer_size: 1 << 30,
        }
    }

    #[test]
    fn auto_ranking_prefers_discrete_gpu_over_integrated_gpu() {
        let discrete = summary(1, WebGpuAdapterDeviceType::DiscreteGpu);
        let integrated = summary(0, WebGpuAdapterDeviceType::IntegratedGpu);

        assert!(discrete.performance_cmp(&integrated).is_gt());
    }

    #[test]
    fn auto_ranking_uses_lower_adapter_index_as_stable_tiebreaker() {
        let lower = summary(0, WebGpuAdapterDeviceType::DiscreteGpu);
        let higher = summary(1, WebGpuAdapterDeviceType::DiscreteGpu);

        assert!(lower.performance_cmp(&higher).is_gt());
    }
}
