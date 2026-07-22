use clearra_app::AppResponse;

use crate::{GuiJobId, GuiJobStatus};

#[derive(Clone, Debug, PartialEq)]
pub struct GuiJobResult {
    job_id: GuiJobId,
    status: GuiJobStatus,
    response: Option<AppResponse>,
}

impl GuiJobResult {
    pub fn completed(job_id: GuiJobId, response: AppResponse) -> Self {
        Self {
            job_id,
            status: GuiJobStatus::Completed,
            response: Some(response),
        }
    }
}
impl GuiJobResult {
    pub fn failed(job_id: GuiJobId, response: AppResponse) -> Self {
        Self {
            job_id,
            status: GuiJobStatus::Failed,
            response: Some(response),
        }
    }
}
impl GuiJobResult {
    pub fn cancelled(job_id: GuiJobId) -> Self {
        Self {
            job_id,
            status: GuiJobStatus::Cancelled,
            response: None,
        }
    }
}
impl GuiJobResult {
    pub const fn job_id(&self) -> GuiJobId {
        self.job_id
    }
}
impl GuiJobResult {
    pub const fn status(&self) -> GuiJobStatus {
        self.status
    }
}
impl GuiJobResult {
    pub fn response(&self) -> Option<&AppResponse> {
        self.response.as_ref()
    }
}
