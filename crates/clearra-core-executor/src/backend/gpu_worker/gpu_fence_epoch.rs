#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct GpuFenceEpoch(u64);

impl GpuFenceEpoch {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}
impl GpuFenceEpoch {
    pub const fn value(self) -> u64 {
        self.0
    }
}
