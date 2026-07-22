use super::CGpuPackingBatchDescriptorView;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CGpuWorkerRequestView {
    pub request_id: u64,
    pub batch: CGpuPackingBatchDescriptorView,
    pub memory_ticket_id: u64,
    pub fence_epoch: u64,
    pub scope_epoch: u64,
    pub byte_budget: u64,
    pub cpu_confirm_required: u8,
}

impl CGpuWorkerRequestView {
    pub fn new(
        request_id: u64,
        batch: CGpuPackingBatchDescriptorView,
        memory_ticket_id: u64,
        fence_epoch: u64,
        scope_epoch: u64,
        byte_budget: u64,
        cpu_confirm_required: bool,
    ) -> Self {
        Self {
            request_id,
            batch,
            memory_ticket_id,
            fence_epoch,
            scope_epoch,
            byte_budget,
            cpu_confirm_required: u8::from(cpu_confirm_required),
        }
    }
}
