use clearra_core_domain::resource::ResourceReport;

#[cfg(any(test, feature = "test-support"))]
use crate::PackingCandidateBatch;

use super::NativePruningLedger;

const C_PACKING_STATUS_OK: i32 = 0;

#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativePackingOutcome {
    pub status: i32,
    pub candidates: PackingCandidateBatch,
    pub resource_report: ResourceReport,
    pub pruning_ledger: NativePruningLedger,
}

#[cfg(any(test, feature = "test-support"))]
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
