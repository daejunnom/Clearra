#[cfg(test)]
use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
use clearra_core_domain::resource::ResourceReport;
#[cfg(test)]
use clearra_core_ffi::CPackingProblem;
use clearra_core_ffi::{NativeGeometryCatalog, NativePruningLedger, PackingCandidateBatch};
#[cfg(test)]
use clearra_pc_graph::request::PcExecutionPolicy;

#[cfg(test)]
use crate::packing::PackingRunnerError;

use super::{
    BackendFallback, GpuDeviceSummary, GpuExecutionFailureResolution, SelectedSearchBackend,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendTrustState {
    CpuExact,
    GpuComputedCpuConfirmed,
    DeterministicReferenceMatched,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingCandidateProvenance {
    RawGeometry,
    BuildabilityPrefiltered,
}

impl PackingCandidateProvenance {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RawGeometry => "raw-geometry",
            Self::BuildabilityPrefiltered => "buildability-prefiltered",
        }
    }

    pub const fn is_raw_geometry(self) -> bool {
        matches!(self, Self::RawGeometry)
    }

    pub const fn buildability_preverified(self) -> bool {
        matches!(self, Self::BuildabilityPrefiltered)
    }
}

impl BackendTrustState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CpuExact => "cpu-exact",
            Self::GpuComputedCpuConfirmed => "gpu-computed-cpu-confirmed",
            Self::DeterministicReferenceMatched => "deterministic-reference-matched",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendTrustReport {
    state: BackendTrustState,
    cpu_confirmed: bool,
    deterministic_reference_matched: bool,
    can_source_exact_probability: bool,
}

impl BackendTrustReport {
    pub const fn cpu_exact() -> Self {
        Self {
            state: BackendTrustState::CpuExact,
            cpu_confirmed: true,
            deterministic_reference_matched: true,
            can_source_exact_probability: true,
        }
    }

    pub const fn gpu_cpu_confirmed(deterministic_reference_matched: bool) -> Self {
        Self {
            state: if deterministic_reference_matched {
                BackendTrustState::DeterministicReferenceMatched
            } else {
                BackendTrustState::GpuComputedCpuConfirmed
            },
            cpu_confirmed: true,
            deterministic_reference_matched,
            can_source_exact_probability: true,
        }
    }

    pub const fn state(self) -> BackendTrustState {
        self.state
    }

    pub const fn cpu_confirmed(self) -> bool {
        self.cpu_confirmed
    }

    pub const fn deterministic_reference_matched(self) -> bool {
        self.deterministic_reference_matched
    }

    pub const fn can_source_exact_probability(self) -> bool {
        self.can_source_exact_probability
    }

    pub(crate) const fn is_valid_for(self, backend: SelectedSearchBackend) -> bool {
        match backend {
            SelectedSearchBackend::None => false,
            SelectedSearchBackend::CpuGeometryExactCover
            | SelectedSearchBackend::CpuParallelGeometryExactCover => {
                matches!(self.state, BackendTrustState::CpuExact)
            }
            SelectedSearchBackend::Gpu | SelectedSearchBackend::Hybrid => {
                (matches!(self.state, BackendTrustState::GpuComputedCpuConfirmed)
                    && self.cpu_confirmed
                    || matches!(self.state, BackendTrustState::DeterministicReferenceMatched)
                        && self.deterministic_reference_matched)
                    && self.can_source_exact_probability
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackingBackendOutcome {
    pub actual_backend: SelectedSearchBackend,
    pub candidates: PackingCandidateBatch,
    pub resource_report: ResourceReport,
    pub trust_report: BackendTrustReport,
    pub fallback: Option<BackendFallback>,
    pub gpu_failure: Option<GpuExecutionFailureResolution>,
    pub gpu_device: Option<GpuDeviceSummary>,
    pub workers_used: usize,
    pub geometry_catalog: Option<NativeGeometryCatalog>,
    pub pruning_ledger: Option<NativePruningLedger>,
    candidate_provenance: PackingCandidateProvenance,
}

impl PackingBackendOutcome {
    pub fn raw_geometry_exact(
        actual_backend: SelectedSearchBackend,
        candidates: PackingCandidateBatch,
        resource_report: ResourceReport,
        trust_report: BackendTrustReport,
    ) -> Self {
        Self::exact_with_provenance(
            actual_backend,
            candidates,
            resource_report,
            trust_report,
            PackingCandidateProvenance::RawGeometry,
        )
    }

    pub fn buildability_prefiltered_exact(
        actual_backend: SelectedSearchBackend,
        candidates: PackingCandidateBatch,
        resource_report: ResourceReport,
        trust_report: BackendTrustReport,
    ) -> Self {
        Self::exact_with_provenance(
            actual_backend,
            candidates,
            resource_report,
            trust_report,
            PackingCandidateProvenance::BuildabilityPrefiltered,
        )
    }

    fn exact_with_provenance(
        actual_backend: SelectedSearchBackend,
        mut candidates: PackingCandidateBatch,
        resource_report: ResourceReport,
        trust_report: BackendTrustReport,
        candidate_provenance: PackingCandidateProvenance,
    ) -> Self {
        candidates.canonicalize_identities();
        Self {
            actual_backend,
            candidates,
            resource_report,
            trust_report,
            fallback: None,
            gpu_failure: None,
            gpu_device: None,
            workers_used: 1,
            geometry_catalog: None,
            pruning_ledger: None,
            candidate_provenance,
        }
    }

    pub const fn candidate_provenance(&self) -> PackingCandidateProvenance {
        self.candidate_provenance
    }

    pub fn with_workers_used(mut self, workers_used: usize) -> Self {
        self.workers_used = workers_used.max(1);
        self
    }

    pub fn with_gpu_device(mut self, gpu_device: GpuDeviceSummary) -> Self {
        self.gpu_device = Some(gpu_device);
        self
    }

    pub fn with_geometry_catalog(mut self, catalog: NativeGeometryCatalog) -> Self {
        self.geometry_catalog = Some(catalog);
        self
    }

    pub fn with_pruning_ledger(mut self, pruning_ledger: NativePruningLedger) -> Self {
        self.pruning_ledger = Some(pruning_ledger);
        self
    }

    pub(crate) fn attach_fallback(
        &mut self,
        fallback: BackendFallback,
        gpu_failure: Option<GpuExecutionFailureResolution>,
    ) {
        self.fallback = Some(fallback);
        self.gpu_failure = gpu_failure;
    }
}

#[cfg(test)]
pub trait SearchBackendExecutor {
    fn execute_packing(
        &self,
        problem: &CPackingProblem,
        policy: &PcExecutionPolicy,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<PackingBackendOutcome, PackingRunnerError>;
}

pub trait SearchBackendExecutorResolver {
    #[cfg(test)]
    fn executor_for(&self, backend: SelectedSearchBackend) -> Option<&dyn SearchBackendExecutor>;

    #[cfg(test)]
    fn cpu_fallback_executor(&self) -> &dyn SearchBackendExecutor;

    #[cfg(test)]
    fn use_resolved_executor_for_test(&self) -> bool {
        false
    }

    fn supports_native_candidate_streaming(&self) -> bool {
        false
    }
}
