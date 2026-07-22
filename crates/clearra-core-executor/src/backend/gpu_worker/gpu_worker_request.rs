use super::{GpuMemoryTicket, PackingBatchDescriptor};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuWorkerRequest {
    request_id: u64,
    batch: PackingBatchDescriptor,
    candidate_count_hint: u32,
    memory_ticket: GpuMemoryTicket,
    cpu_confirm_required: bool,
}

impl GpuWorkerRequest {
    pub fn new(
        request_id: u64,
        batch: PackingBatchDescriptor,
        candidate_count_hint: u32,
        memory_ticket: GpuMemoryTicket,
        cpu_confirm_required: bool,
    ) -> Result<Self, super::GpuWorkerError> {
        Self::from_optional_memory_ticket(
            request_id,
            batch,
            candidate_count_hint,
            Some(memory_ticket),
            cpu_confirm_required,
        )
    }
}
impl GpuWorkerRequest {
    pub fn from_optional_memory_ticket(
        request_id: u64,
        batch: PackingBatchDescriptor,
        candidate_count_hint: u32,
        memory_ticket: Option<GpuMemoryTicket>,
        cpu_confirm_required: bool,
    ) -> Result<Self, super::GpuWorkerError> {
        if !cpu_confirm_required {
            return Err(super::GpuWorkerError::CpuConfirmRequiredForGpuBatch);
        }
        let memory_ticket = memory_ticket.ok_or(super::GpuWorkerError::MissingMemoryTicket)?;

        Ok(Self {
            request_id,
            batch,
            candidate_count_hint,
            memory_ticket,
            cpu_confirm_required,
        })
    }
}
impl GpuWorkerRequest {
    pub fn request_id(&self) -> u64 {
        self.request_id
    }
}
impl GpuWorkerRequest {
    pub fn batch(&self) -> PackingBatchDescriptor {
        self.batch
    }
}
impl GpuWorkerRequest {
    pub fn candidate_count_hint(&self) -> u32 {
        self.candidate_count_hint
    }
}
impl GpuWorkerRequest {
    pub fn memory_ticket(&self) -> GpuMemoryTicket {
        self.memory_ticket
    }
}
impl GpuWorkerRequest {
    pub fn cpu_confirm_required(&self) -> bool {
        self.cpu_confirm_required
    }
}
