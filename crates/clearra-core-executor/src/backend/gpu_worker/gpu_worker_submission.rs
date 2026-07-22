use super::{GpuFenceEpoch, GpuWorkerError, GpuWorkerRequest};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GpuWorkerSubmissionStatus {
    #[default]
    Queued,
    Cancelled,
    Aborted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuWorkerSubmission {
    request_id: u64,
    submission_epoch: GpuFenceEpoch,
    queue_index: u16,
    status: GpuWorkerSubmissionStatus,
}

impl GpuWorkerSubmission {
    pub const fn new(request_id: u64, submission_epoch: GpuFenceEpoch, queue_index: u16) -> Self {
        Self {
            request_id,
            submission_epoch,
            queue_index,
            status: GpuWorkerSubmissionStatus::Queued,
        }
    }
}
impl GpuWorkerSubmission {
    pub const fn request_id(self) -> u64 {
        self.request_id
    }
}
impl GpuWorkerSubmission {
    pub const fn submission_epoch(self) -> GpuFenceEpoch {
        self.submission_epoch
    }
}
impl GpuWorkerSubmission {
    pub const fn queue_index(self) -> u16 {
        self.queue_index
    }
}
impl GpuWorkerSubmission {
    pub const fn status(self) -> GpuWorkerSubmissionStatus {
        self.status
    }

    pub fn validate_request(&self, request: &GpuWorkerRequest) -> Result<(), GpuWorkerError> {
        if request.request_id() != self.request_id {
            return Err(GpuWorkerError::SubmissionRequestMismatch {
                expected: request.request_id(),
                actual: self.request_id,
            });
        }
        let expected = request.memory_ticket().scope_epoch();
        if expected != self.submission_epoch {
            return Err(GpuWorkerError::MemoryTicketMismatch {
                expected: expected.value(),
                actual: self.submission_epoch.value(),
            });
        }
        Ok(())
    }
}
