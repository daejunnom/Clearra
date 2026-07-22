#[cfg(not(feature = "webgpu-search"))]
use clearra_core_ffi::NativeGpuCapabilityQuery;
use clearra_core_ffi::{NativeGpuCapabilityQueryError, NativeGpuUnavailableReason};
use clearra_pc_graph::request::GpuDeviceSelection;

use super::GpuUnavailableReason;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuSearchCapability {
    Available {
        device_index: u8,
        auto_selectable: bool,
    },
    Unavailable(GpuUnavailableReason),
}

impl GpuSearchCapability {
    pub const fn available(device_index: u8) -> Self {
        Self::Available {
            device_index,
            auto_selectable: true,
        }
    }

    pub const fn available_explicit_only(device_index: u8) -> Self {
        Self::Available {
            device_index,
            auto_selectable: false,
        }
    }

    pub const fn unavailable(reason: GpuUnavailableReason) -> Self {
        Self::Unavailable(reason)
    }

    pub const fn is_available(self) -> bool {
        matches!(self, Self::Available { .. })
    }

    pub const fn is_auto_selectable(self) -> bool {
        matches!(
            self,
            Self::Available {
                auto_selectable: true,
                ..
            }
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityQueryError {
    BindingUnavailable,
    AbiMismatch { expected: i32, actual: i32 },
    InvalidArgument,
    InvalidNativeStatus(i32),
    InvalidNativeCapability,
}

pub trait SearchBackendCapabilityProvider: Sync {
    fn gpu_capability(
        &self,
        device: GpuDeviceSelection,
    ) -> Result<GpuSearchCapability, CapabilityQueryError>;

    fn prepared_gpu_capability(
        &self,
        device: GpuDeviceSelection,
    ) -> Result<GpuSearchCapability, CapabilityQueryError>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeSearchBackendCapabilityProvider;

impl SearchBackendCapabilityProvider for NativeSearchBackendCapabilityProvider {
    fn gpu_capability(
        &self,
        device: GpuDeviceSelection,
    ) -> Result<GpuSearchCapability, CapabilityQueryError> {
        #[cfg(all(feature = "webgpu-search", feature = "native-c-core"))]
        {
            return Ok(
                match super::search_backend_warmup::connect_webgpu_session_for_device(device) {
                    clearra_webgpu::WebGpuGeometryExactCoverSessionOutcome::Connected(session) => {
                        let device_index = session.adapter().index();
                        session.recycle();
                        GpuSearchCapability::available(device_index)
                    }
                    clearra_webgpu::WebGpuGeometryExactCoverSessionOutcome::Unavailable(_) => {
                        GpuSearchCapability::unavailable(GpuUnavailableReason::DeviceNotFound)
                    }
                },
            );
        }

        #[cfg(all(feature = "webgpu-search", not(feature = "native-c-core")))]
        {
            let _ = device;
            return Ok(GpuSearchCapability::unavailable(
                GpuUnavailableReason::BackendNotConnected,
            ));
        }

        #[cfg(not(feature = "webgpu-search"))]
        {
            let requested_index = match device {
                GpuDeviceSelection::Auto => None,
                GpuDeviceSelection::Index(index) => Some(index),
            };
            let native_capability = NativeGpuCapabilityQuery::query(requested_index);

            let capability = native_capability.map_err(CapabilityQueryError::from)?;

            if !capability.is_available() {
                let reason = capability
                    .unavailable_reason()
                    .ok_or(CapabilityQueryError::InvalidNativeCapability)?;
                return Ok(GpuSearchCapability::unavailable(reason.into()));
            }
            if !capability.is_connected() {
                return Ok(GpuSearchCapability::unavailable(
                    GpuUnavailableReason::BackendNotConnected,
                ));
            }
            if !capability.supports_exact_search() {
                return Ok(GpuSearchCapability::unavailable(
                    GpuUnavailableReason::ExactSearchUnsupported,
                ));
            }

            Ok(GpuSearchCapability::available(capability.device_index()))
        }
    }

    fn prepared_gpu_capability(
        &self,
        device: GpuDeviceSelection,
    ) -> Result<GpuSearchCapability, CapabilityQueryError> {
        #[cfg(all(feature = "webgpu-search", feature = "native-c-core"))]
        {
            return Ok(
                match super::search_backend_warmup::take_prepared_webgpu_session_for_device(device)
                {
                    Some(session) => {
                        let device_index = session.adapter().index();
                        session.recycle();
                        GpuSearchCapability::available(device_index)
                    }
                    None => {
                        GpuSearchCapability::unavailable(GpuUnavailableReason::BackendNotConnected)
                    }
                },
            );
        }

        #[cfg(all(feature = "webgpu-search", not(feature = "native-c-core")))]
        {
            let _ = device;
            return Ok(GpuSearchCapability::unavailable(
                GpuUnavailableReason::BackendNotConnected,
            ));
        }

        #[cfg(not(feature = "webgpu-search"))]
        {
            self.gpu_capability(device)
        }
    }
}

impl From<NativeGpuUnavailableReason> for GpuUnavailableReason {
    fn from(reason: NativeGpuUnavailableReason) -> Self {
        match reason {
            NativeGpuUnavailableReason::FeatureDisabled => Self::FeatureDisabled,
            NativeGpuUnavailableReason::DeviceNotFound => Self::DeviceNotFound,
            NativeGpuUnavailableReason::KernelUnavailable => Self::KernelUnavailable,
        }
    }
}

impl From<NativeGpuCapabilityQueryError> for CapabilityQueryError {
    fn from(error: NativeGpuCapabilityQueryError) -> Self {
        match error {
            NativeGpuCapabilityQueryError::BindingUnavailable => Self::BindingUnavailable,
            NativeGpuCapabilityQueryError::AbiMismatch { expected, actual } => {
                Self::AbiMismatch { expected, actual }
            }
            NativeGpuCapabilityQueryError::InvalidArgument => Self::InvalidArgument,
            NativeGpuCapabilityQueryError::InvalidNativeStatus(status) => {
                Self::InvalidNativeStatus(status)
            }
            NativeGpuCapabilityQueryError::InvalidNativeCapability => Self::InvalidNativeCapability,
        }
    }
}
