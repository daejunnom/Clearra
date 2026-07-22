use crate::model::GuiJobId;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GuiExecutionPhase {
    #[default]
    Idle,
    PreviewReady,
    Running,
    Completed,
    Failed,
}

impl GuiExecutionPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::PreviewReady => "preview-ready",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiExecutionState {
    phase: GuiExecutionPhase,
    active_job_id: Option<GuiJobId>,
    recent_result_status: Option<String>,
}

impl GuiExecutionState {
    pub const fn new(
        phase: GuiExecutionPhase,
        active_job_id: Option<GuiJobId>,
        recent_result_status: Option<String>,
    ) -> Self {
        Self {
            phase,
            active_job_id,
            recent_result_status,
        }
    }
}
impl GuiExecutionState {
    pub const fn idle() -> Self {
        Self::new(GuiExecutionPhase::Idle, None, None)
    }
}
impl GuiExecutionState {
    pub const fn preview_ready() -> Self {
        Self::new(GuiExecutionPhase::PreviewReady, None, None)
    }
}
impl GuiExecutionState {
    pub fn running(job_id: GuiJobId) -> Self {
        Self::new(GuiExecutionPhase::Running, Some(job_id), None)
    }
}
impl GuiExecutionState {
    pub fn completed(status: impl Into<String>) -> Self {
        Self::new(GuiExecutionPhase::Completed, None, Some(status.into()))
    }
}
impl GuiExecutionState {
    pub fn failed(status: impl Into<String>) -> Self {
        Self::new(GuiExecutionPhase::Failed, None, Some(status.into()))
    }
}
impl GuiExecutionState {
    pub const fn phase(&self) -> GuiExecutionPhase {
        self.phase
    }
}
impl GuiExecutionState {
    pub const fn active_job_id(&self) -> Option<GuiJobId> {
        self.active_job_id
    }
}
impl GuiExecutionState {
    pub fn recent_result_status(&self) -> Option<&str> {
        self.recent_result_status.as_deref()
    }
}

impl Default for GuiExecutionState {
    fn default() -> Self {
        Self::idle()
    }
}
