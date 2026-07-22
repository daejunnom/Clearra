use super::{GpuFenceEpoch, GpuWorkerError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuMemoryTicket {
    id: u64,
    scope_epoch: GpuFenceEpoch,
    byte_budget: u64,
}

impl GpuMemoryTicket {
    pub fn try_new(
        id: u64,
        scope_epoch: GpuFenceEpoch,
        byte_budget: u64,
    ) -> Result<Self, GpuWorkerError> {
        if id == 0 {
            return Err(GpuWorkerError::InvalidMemoryTicket {
                reason: "memory ticket id must be nonzero",
            });
        }
        if scope_epoch.value() == 0 {
            return Err(GpuWorkerError::InvalidMemoryTicket {
                reason: "memory ticket scope epoch must be nonzero",
            });
        }
        if byte_budget == 0 {
            return Err(GpuWorkerError::InvalidMemoryTicket {
                reason: "memory ticket byte budget must be nonzero",
            });
        }

        Ok(Self {
            id,
            scope_epoch,
            byte_budget,
        })
    }
}
impl GpuMemoryTicket {
    pub fn new(id: u64, scope_epoch: GpuFenceEpoch, byte_budget: u64) -> Self {
        Self::try_new(id, scope_epoch, byte_budget).expect("valid GPU memory ticket")
    }
}
impl GpuMemoryTicket {
    pub const fn id(self) -> u64 {
        self.id
    }
}
impl GpuMemoryTicket {
    pub const fn scope_epoch(self) -> GpuFenceEpoch {
        self.scope_epoch
    }
}
impl GpuMemoryTicket {
    pub const fn byte_budget(self) -> u64 {
        self.byte_budget
    }
}
