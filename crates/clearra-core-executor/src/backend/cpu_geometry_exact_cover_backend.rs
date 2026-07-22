use super::BackendCapability;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CpuGeometryExactCoverBackend;

impl CpuGeometryExactCoverBackend {
    pub const BACKEND_ID: &'static str = "cpu-geometry-exact-cover";
}
impl CpuGeometryExactCoverBackend {
    pub fn capability() -> BackendCapability {
        BackendCapability::supported(Self::BACKEND_ID)
    }
}
