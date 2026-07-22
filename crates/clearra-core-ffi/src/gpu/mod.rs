mod native_search_capability;

#[cfg(feature = "experimental-native-gpu")]
pub mod gpu_packing_batch_descriptor_view;
#[cfg(feature = "experimental-native-gpu")]
pub mod gpu_worker_request_view;

#[cfg(feature = "experimental-native-gpu")]
pub use gpu_packing_batch_descriptor_view::{
    CGpuPackingBatchDescriptorDebugSnapshot, CGpuPackingBatchDescriptorView,
    CGpuPackingBatchDescriptorViewError, CGpuPieceMultisetWindow,
};
#[cfg(feature = "experimental-native-gpu")]
pub use gpu_worker_request_view::CGpuWorkerRequestView;
#[cfg(feature = "native-c-core")]
pub(crate) use native_search_capability::{CNativeGpuBackendCapability, CNativeGpuDeviceRequest};
pub use native_search_capability::{
    NativeGpuCapabilityQuery, NativeGpuCapabilityQueryError, NativeGpuSearchCapability,
    NativeGpuUnavailableReason,
};
