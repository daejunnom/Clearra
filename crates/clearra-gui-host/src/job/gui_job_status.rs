#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GuiJobStatus {
    #[default]
    Idle,
    Queued,
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
}

impl GuiJobStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Cancelling => "cancelling",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}
