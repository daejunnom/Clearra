use crate::{
    AppResponse, BackendReport, Diagnostic, DiagnosticReport, ResourceBudget, ResourceReport,
};

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
// Keep the stable, unboxed event API while serde and every host share this contract.
#[allow(clippy::large_enum_variant)]
pub enum JobEvent {
    Started(JobStarted),
    Progress(JobProgress),
    BackendStatus(BackendStatusReport),
    ResourceStatus(ResourceReport),
    PartialResult(PartialResult),
    Diagnostic(DiagnosticEvent),
    Completed(AppResponse),
    Cancelled(CancelledReport),
    Failed(DiagnosticReport),
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct JobStarted {
    job_id: u64,
}

impl JobStarted {
    pub const fn new(job_id: u64) -> Self {
        Self { job_id }
    }
}
impl JobStarted {
    pub const fn job_id(&self) -> u64 {
        self.job_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BackendStatusReport {
    job_id: u64,
    search_backend: String,
    post_backend: String,
    backend_report: BackendReport,
}

impl BackendStatusReport {
    pub fn new(
        job_id: u64,
        search_backend: impl Into<String>,
        post_backend: impl Into<String>,
        backend_report: BackendReport,
    ) -> Self {
        Self {
            job_id,
            search_backend: search_backend.into(),
            post_backend: post_backend.into(),
            backend_report,
        }
    }
}
impl BackendStatusReport {
    pub fn wasm_cpu(job_id: u64) -> Self {
        Self::new(
            job_id,
            "wasm-cpu",
            "wasm-cpu",
            BackendReport::new("auto", "clearra-wasm", None::<String>),
        )
    }
}
impl BackendStatusReport {
    pub const fn job_id(&self) -> u64 {
        self.job_id
    }
}
impl BackendStatusReport {
    pub fn search_backend(&self) -> &str {
        &self.search_backend
    }
}
impl BackendStatusReport {
    pub fn post_backend(&self) -> &str {
        &self.post_backend
    }
}
impl BackendStatusReport {
    pub fn backend_report(&self) -> &BackendReport {
        &self.backend_report
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct JobProgress {
    job_id: u64,
    done: u32,
    total: u32,
    label: String,
    resource_budget: ResourceBudget,
    backend_status: BackendStatusReport,
}

impl JobProgress {
    pub fn new(
        job_id: u64,
        done: u32,
        total: u32,
        label: impl Into<String>,
        resource_budget: ResourceBudget,
        backend_status: BackendStatusReport,
    ) -> Self {
        Self {
            job_id,
            done,
            total,
            label: label.into(),
            resource_budget,
            backend_status,
        }
    }
}
impl JobProgress {
    pub const fn job_id(&self) -> u64 {
        self.job_id
    }
}
impl JobProgress {
    pub const fn done(&self) -> u32 {
        self.done
    }
}
impl JobProgress {
    pub const fn total(&self) -> u32 {
        self.total
    }
}
impl JobProgress {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl JobProgress {
    pub const fn resource_budget(&self) -> ResourceBudget {
        self.resource_budget
    }
}
impl JobProgress {
    pub fn backend_status(&self) -> &BackendStatusReport {
        &self.backend_status
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PartialResult {
    job_id: u64,
    label: String,
    partial: bool,
    final_result: bool,
}

impl PartialResult {
    pub fn new(job_id: u64, label: impl Into<String>, partial: bool, final_result: bool) -> Self {
        Self {
            job_id,
            label: label.into(),
            partial,
            final_result,
        }
    }
}
impl PartialResult {
    pub const fn job_id(&self) -> u64 {
        self.job_id
    }
}
impl PartialResult {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl PartialResult {
    pub const fn partial(&self) -> bool {
        self.partial
    }
}
impl PartialResult {
    pub const fn final_result(&self) -> bool {
        self.final_result
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticEvent {
    job_id: u64,
    diagnostic: Diagnostic,
}

impl DiagnosticEvent {
    pub fn new(job_id: u64, diagnostic: Diagnostic) -> Self {
        Self { job_id, diagnostic }
    }
}
impl DiagnosticEvent {
    pub const fn job_id(&self) -> u64 {
        self.job_id
    }
}
impl DiagnosticEvent {
    pub fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CancelledReport {
    job_id: u64,
    c_scope_released: bool,
}

impl CancelledReport {
    pub const fn new(job_id: u64, c_scope_released: bool) -> Self {
        Self {
            job_id,
            c_scope_released,
        }
    }
}
impl CancelledReport {
    pub const fn job_id(&self) -> u64 {
        self.job_id
    }
}
impl CancelledReport {
    pub const fn c_scope_released(&self) -> bool {
        self.c_scope_released
    }
}

#[cfg(test)]
#[path = "job_event_tests.rs"]
mod tests;
