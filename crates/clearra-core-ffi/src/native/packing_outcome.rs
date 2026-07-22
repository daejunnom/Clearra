use clearra_core_domain::resource::ResourceReport;

use crate::PackingCandidateBatch;

use super::NativePruningLedger;

const C_PACKING_STATUS_OK: i32 = 0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativePackingOutcome {
    pub status: i32,
    pub candidates: PackingCandidateBatch,
    pub resource_report: ResourceReport,
    pub pruning_ledger: NativePruningLedger,
}

impl NativePackingOutcome {
    pub fn count_complete(&self) -> bool {
        self.status == C_PACKING_STATUS_OK && !self.resource_report.truncated
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativePackingStreamOutcome {
    pub status: i32,
    pub resource_report: ResourceReport,
    pub pruning_ledger: NativePruningLedger,
}

impl NativePackingStreamOutcome {
    pub fn count_complete(&self) -> bool {
        self.status == C_PACKING_STATUS_OK && !self.resource_report.truncated
    }
}
