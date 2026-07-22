#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetStatus {
    state: String,
    used: u64,
    limit: Option<u64>,
}

impl BudgetStatus {
    pub fn within_budget() -> Self {
        Self {
            state: "within-budget".to_owned(),
            used: 0,
            limit: None,
        }
    }
}
impl BudgetStatus {
    pub fn state(&self) -> &str {
        &self.state
    }
}
impl BudgetStatus {
    pub const fn used(&self) -> u64 {
        self.used
    }
}
impl BudgetStatus {
    pub const fn limit(&self) -> Option<u64> {
        self.limit
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendStatus {
    backend_requested: String,
    backend_selected: String,
    fallback_used: bool,
}

impl BackendStatus {
    pub fn app_boundary() -> Self {
        Self {
            backend_requested: "auto".to_owned(),
            backend_selected: "clearra-app".to_owned(),
            fallback_used: false,
        }
    }
}
impl BackendStatus {
    pub fn backend_requested(&self) -> &str {
        &self.backend_requested
    }
}
impl BackendStatus {
    pub fn backend_selected(&self) -> &str {
        &self.backend_selected
    }
}
impl BackendStatus {
    pub const fn fallback_used(&self) -> bool {
        self.fallback_used
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryStatus {
    state: String,
    leak_report_clean: Option<bool>,
    raw_pointer_exposed: bool,
}

impl MemoryStatus {
    pub fn app_owned() -> Self {
        Self {
            state: "app-owned".to_owned(),
            leak_report_clean: None,
            raw_pointer_exposed: false,
        }
    }
}
impl MemoryStatus {
    pub fn cancelled_scope_release_pending() -> Self {
        Self {
            state: "cancelled-scope-release-pending".to_owned(),
            leak_report_clean: None,
            raw_pointer_exposed: false,
        }
    }
}
impl MemoryStatus {
    pub fn state(&self) -> &str {
        &self.state
    }
}
impl MemoryStatus {
    pub const fn leak_report_clean(&self) -> Option<bool> {
        self.leak_report_clean
    }
}
impl MemoryStatus {
    pub const fn raw_pointer_exposed(&self) -> bool {
        self.raw_pointer_exposed
    }
}
