#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GpuWorkerState {
    #[default]
    Disabled,
    Available,
    Busy,
    Draining,
    Failed,
}

impl GpuWorkerState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Available => "available",
            Self::Busy => "busy",
            Self::Draining => "draining",
            Self::Failed => "failed",
        }
    }
}
