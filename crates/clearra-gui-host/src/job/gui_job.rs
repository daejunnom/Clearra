use clearra_app::AppRequest;

use crate::{GuiJobCancelHandle, GuiJobCancelToken, GuiJobId, GuiJobStatus};

#[derive(Clone, Debug)]
pub struct GuiJob {
    job_id: GuiJobId,
    request: AppRequest,
    status: GuiJobStatus,
    cancel_token: GuiJobCancelToken,
}

impl GuiJob {
    pub fn new(job_id: GuiJobId, request: AppRequest) -> Self {
        Self {
            job_id,
            request,
            status: GuiJobStatus::Queued,
            cancel_token: GuiJobCancelToken::new(),
        }
    }
}
impl GuiJob {
    pub const fn job_id(&self) -> GuiJobId {
        self.job_id
    }
}
impl GuiJob {
    pub fn request(&self) -> &AppRequest {
        &self.request
    }
}
impl GuiJob {
    pub const fn status(&self) -> GuiJobStatus {
        self.status
    }
}
impl GuiJob {
    pub fn cancel_token(&self) -> GuiJobCancelToken {
        self.cancel_token.clone()
    }
}
impl GuiJob {
    pub fn cancel_handle(&self) -> GuiJobCancelHandle {
        self.cancel_token.handle()
    }
}
impl GuiJob {
    pub fn into_request(self) -> AppRequest {
        self.request
    }
}
impl GuiJob {
    pub(crate) fn mark_running(mut self) -> Self {
        self.status = GuiJobStatus::Running;
        self
    }
}
