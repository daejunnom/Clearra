use std::fmt;

use clearra_app::AppRequest;

use crate::{GuiJob, GuiJobId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuiJobQueueErrorCode {
    JobAlreadyActive,
    NoQueuedJob,
    ActiveJobMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiJobQueueError {
    code: GuiJobQueueErrorCode,
    message: String,
}

impl GuiJobQueueError {
    pub fn new(code: GuiJobQueueErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl GuiJobQueueError {
    pub const fn code(&self) -> GuiJobQueueErrorCode {
        self.code
    }
}
impl GuiJobQueueError {
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for GuiJobQueueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for GuiJobQueueError {}

#[derive(Clone, Debug, Default)]
pub struct GuiJobQueue {
    next_job_id: u64,
    queued_job: Option<GuiJob>,
    active_job_id: Option<GuiJobId>,
}

impl GuiJobQueue {
    pub fn new() -> Self {
        Self {
            next_job_id: 1,
            queued_job: None,
            active_job_id: None,
        }
    }
}
impl GuiJobQueue {
    pub fn enqueue(&mut self, request: AppRequest) -> Result<GuiJob, GuiJobQueueError> {
        if self.queued_job.is_some() || self.active_job_id.is_some() {
            return Err(GuiJobQueueError::new(
                GuiJobQueueErrorCode::JobAlreadyActive,
                "GUI host MVP allows a single active or queued job",
            ));
        }

        let job = GuiJob::new(GuiJobId::new(self.next_job_id), request);
        self.next_job_id += 1;
        self.queued_job = Some(job.clone());
        Ok(job)
    }
}
impl GuiJobQueue {
    pub fn take_next(&mut self) -> Result<GuiJob, GuiJobQueueError> {
        let job = self.queued_job.take().ok_or_else(|| {
            GuiJobQueueError::new(GuiJobQueueErrorCode::NoQueuedJob, "no queued GUI job")
        })?;
        self.active_job_id = Some(job.job_id());
        Ok(job.mark_running())
    }
}
impl GuiJobQueue {
    pub fn finish(&mut self, job_id: GuiJobId) -> Result<(), GuiJobQueueError> {
        if self.active_job_id != Some(job_id) {
            return Err(GuiJobQueueError::new(
                GuiJobQueueErrorCode::ActiveJobMismatch,
                "completed GUI job does not match the active job",
            ));
        }
        self.active_job_id = None;
        Ok(())
    }
}
impl GuiJobQueue {
    pub const fn active_job_id(&self) -> Option<GuiJobId> {
        self.active_job_id
    }
}
impl GuiJobQueue {
    pub fn queued_job(&self) -> Option<&GuiJob> {
        self.queued_job.as_ref()
    }
}
impl GuiJobQueue {
    pub fn is_idle(&self) -> bool {
        self.queued_job.is_none() && self.active_job_id.is_none()
    }
}
