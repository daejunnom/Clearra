use crate::{BackendStatus, BudgetStatus, MemoryStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiJobProgress {
    done: u32,
    total: u32,
    label: String,
    budget_status: BudgetStatus,
    backend_status: BackendStatus,
    memory_status: MemoryStatus,
}

impl GuiJobProgress {
    pub fn new(done: u32, total: u32, label: impl Into<String>) -> Self {
        Self {
            done,
            total,
            label: label.into(),
            budget_status: BudgetStatus::within_budget(),
            backend_status: BackendStatus::app_boundary(),
            memory_status: MemoryStatus::app_owned(),
        }
    }
}
impl GuiJobProgress {
    pub const fn done(&self) -> u32 {
        self.done
    }
}
impl GuiJobProgress {
    pub const fn total(&self) -> u32 {
        self.total
    }
}
impl GuiJobProgress {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl GuiJobProgress {
    pub fn budget_status(&self) -> &BudgetStatus {
        &self.budget_status
    }
}
impl GuiJobProgress {
    pub fn backend_status(&self) -> &BackendStatus {
        &self.backend_status
    }
}
impl GuiJobProgress {
    pub fn memory_status(&self) -> &MemoryStatus {
        &self.memory_status
    }
}
