#[cfg(feature = "native-c-core")]
const C_GPU_OK: i32 = 0;
#[cfg(feature = "native-c-core")]
const C_GPU_INVALID_ARGUMENT: i32 = 1;
#[cfg(feature = "native-c-core")]
const C_GPU_UNAVAILABLE: i32 = 2;
#[cfg(feature = "native-c-core")]
const C_GPU_BACKEND_NATIVE_COMPUTE: u8 = 1;
#[cfg(feature = "native-c-core")]
const C_GPU_UNAVAILABLE_NONE: i32 = 0;
#[cfg(feature = "native-c-core")]
const C_GPU_UNAVAILABLE_FEATURE_DISABLED: i32 = 1;
#[cfg(feature = "native-c-core")]
const C_GPU_UNAVAILABLE_DEVICE_NOT_FOUND: i32 = 2;
#[cfg(feature = "native-c-core")]
const C_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE: i32 = 3;

#[cfg(feature = "native-c-core")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CNativeGpuDeviceRequest {
    pub(crate) device_kind: u8,
    pub(crate) device_index: u8,
}

#[cfg(feature = "native-c-core")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CNativeGpuBackendCapability {
    pub(crate) backend_kind: i32,
    pub(crate) available: u8,
    pub(crate) connected: u8,
    pub(crate) exact_supported: u8,
    pub(crate) accepts_user_shader_path: u8,
    pub(crate) unavailable_reason: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeGpuUnavailableReason {
    FeatureDisabled,
    DeviceNotFound,
    KernelUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeGpuCapabilityQueryError {
    BindingUnavailable,
    AbiMismatch { expected: i32, actual: i32 },
    InvalidArgument,
    InvalidNativeStatus(i32),
    InvalidNativeCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeGpuSearchCapability {
    backend_kind: i32,
    device_index: u8,
    available: bool,
    connected: bool,
    exact_supported: bool,
    unavailable_reason: Option<NativeGpuUnavailableReason>,
}

impl NativeGpuSearchCapability {
    pub fn backend_kind(self) -> i32 {
        self.backend_kind
    }

    pub fn device_index(self) -> u8 {
        self.device_index
    }

    pub fn is_available(self) -> bool {
        self.available
    }

    pub fn is_connected(self) -> bool {
        self.connected
    }

    pub fn supports_exact_search(self) -> bool {
        self.exact_supported
    }

    pub fn unavailable_reason(self) -> Option<NativeGpuUnavailableReason> {
        self.unavailable_reason
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeGpuCapabilityQuery;

impl NativeGpuCapabilityQuery {
    pub fn query(
        device_index: Option<u8>,
    ) -> Result<NativeGpuSearchCapability, NativeGpuCapabilityQueryError> {
        query_native_gpu_capability(device_index.unwrap_or(0))
    }
}

#[cfg(feature = "native-c-core")]
fn query_native_gpu_capability(
    device_index: u8,
) -> Result<NativeGpuSearchCapability, NativeGpuCapabilityQueryError> {
    let actual_abi = crate::raw::bindings::abi_version();
    let expected_abi = crate::version::CLEARRA_CORE_ABI_VERSION;
    if actual_abi != expected_abi {
        return Err(NativeGpuCapabilityQueryError::AbiMismatch {
            expected: expected_abi,
            actual: actual_abi,
        });
    }

    let request = CNativeGpuDeviceRequest {
        device_kind: C_GPU_BACKEND_NATIVE_COMPUTE,
        device_index,
    };
    let mut capability = CNativeGpuBackendCapability::default();
    let status = crate::raw::bindings::query_gpu_capability(request, &mut capability);
    match status {
        C_GPU_OK => capability_from_native(device_index, capability, true),
        C_GPU_UNAVAILABLE => capability_from_native(device_index, capability, false),
        C_GPU_INVALID_ARGUMENT => Err(NativeGpuCapabilityQueryError::InvalidArgument),
        other => Err(NativeGpuCapabilityQueryError::InvalidNativeStatus(other)),
    }
}

#[cfg(not(feature = "native-c-core"))]
fn query_native_gpu_capability(
    _device_index: u8,
) -> Result<NativeGpuSearchCapability, NativeGpuCapabilityQueryError> {
    Err(NativeGpuCapabilityQueryError::BindingUnavailable)
}

#[cfg(feature = "native-c-core")]
fn capability_from_native(
    device_index: u8,
    capability: CNativeGpuBackendCapability,
    status_reports_available: bool,
) -> Result<NativeGpuSearchCapability, NativeGpuCapabilityQueryError> {
    let unavailable_reason = match capability.unavailable_reason {
        C_GPU_UNAVAILABLE_NONE => None,
        C_GPU_UNAVAILABLE_FEATURE_DISABLED => Some(NativeGpuUnavailableReason::FeatureDisabled),
        C_GPU_UNAVAILABLE_DEVICE_NOT_FOUND => Some(NativeGpuUnavailableReason::DeviceNotFound),
        C_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE => Some(NativeGpuUnavailableReason::KernelUnavailable),
        _ => return Err(NativeGpuCapabilityQueryError::InvalidNativeCapability),
    };
    let available = capability.available != 0;
    if available != status_reports_available
        || available && unavailable_reason.is_some()
        || !available && unavailable_reason.is_none()
    {
        return Err(NativeGpuCapabilityQueryError::InvalidNativeCapability);
    }

    Ok(NativeGpuSearchCapability {
        backend_kind: capability.backend_kind,
        device_index,
        available,
        connected: capability.connected != 0,
        exact_supported: capability.exact_supported != 0,
        unavailable_reason,
    })
}
