// Every module here is gated behind `dx12` except `hdr` (the
// `DXGI_OUTPUT_DESC1` -> `DisplayHdrInfo` mapping), which the
// Vulkan-on-Windows backend also uses.
#[cfg(all(target_os = "windows", feature = "dx12"))]
pub mod conv;
#[cfg(all(target_os = "windows", feature = "dx12"))]
pub mod exception;
#[cfg(all(target_os = "windows", feature = "dx12"))]
pub mod factory;
pub mod hdr;
#[cfg(all(target_os = "windows", feature = "dx12"))]
pub mod name;
#[cfg(all(target_os = "windows", feature = "dx12"))]
pub mod result;
#[cfg(all(target_os = "windows", feature = "dx12"))]
pub mod time;
