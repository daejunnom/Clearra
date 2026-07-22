mod buildup;
pub(crate) mod buildup_geometry_language;
mod geometry_catalog;
mod geometry_solution_graph;
#[cfg(any(test, feature = "test-support"))]
mod packing;
mod packing_candidate_sink;
#[cfg(any(test, feature = "test-support"))]
mod packing_geometry_materializer;
#[cfg(any(test, feature = "test-support"))]
mod packing_outcome;
mod pruning;
mod resource;
#[cfg(feature = "search-stage-profiling")]
mod search_profile;

use crate::problem::{CBuildUpProblem, CPackingProblem};

pub use buildup::{
    CNativeBuildUpCountLimits, CNativeBuildUpCountReport, CNativeBuildUpEnumerationLimits,
    CNativeBuildUpSearchMetrics, CNativeBuildUpVerification, CNativeBuildVariantBuffer,
    CNativeBuildVariantView, NativeBuildUpCountOutcome, NativeBuildUpOutcome,
    NativeBuildUpWorkspace, NativeBuildUpWorkspaceOutcome, CLR_BUILDUP_CAPACITY_EXCEEDED,
    CLR_BUILDUP_ENUMERATION_TRUNCATED, CLR_BUILDUP_MODE_COUNT_VARIANTS,
    CLR_BUILDUP_MODE_ENUMERATE_VARIANTS, CLR_BUILDUP_MODE_VERIFY_FIRST,
    CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING, CLR_KICK_EVIDENCE_BUFFER_EXHAUSTED,
    C_BUILDUP_STATUS_CANCELLED, C_BUILDUP_STATUS_CAPACITY_EXCEEDED,
    C_BUILDUP_STATUS_ENUMERATION_TRUNCATED, C_BUILDUP_STATUS_HOLD_DISABLED_IMPOSSIBLE,
    C_BUILDUP_STATUS_INVALID_ARGUMENT, C_BUILDUP_STATUS_INVALID_ORDER,
    C_BUILDUP_STATUS_INVALID_PROBLEM, C_BUILDUP_STATUS_KICK_EVIDENCE_BUFFER_EXHAUSTED,
    C_BUILDUP_STATUS_LOGICAL_REJECT_MAX, C_BUILDUP_STATUS_LOGICAL_REJECT_MIN, C_BUILDUP_STATUS_OK,
    C_BUILDUP_STATUS_UNSUPPORTED_RUNTIME_SCOPE,
};
pub use buildup_geometry_language::{
    BuildUpGeometryLanguage, BuildUpGeometryLanguageEdge, BuildUpGeometryLanguageNode,
};
#[cfg(feature = "native-c-core")]
pub(crate) use geometry_catalog::CNativeGeometryCatalogView;
pub use geometry_catalog::{
    NativeGeometryCatalog, NativeGeometryCatalogIdentity, NativeGeometryCatalogView,
};
pub use geometry_solution_graph::{
    CNativeBuildableGeometryStreamReport, NativeBuildableGeometryTaskOutcome,
    NativeGeometryGraphSearchOutcome, NativeGeometryPathConsumer, NativeGeometryPathSinkError,
    NativeGeometrySolutionGraph, NativeGeometrySolutionTask, C_NATIVE_GEOMETRY_PATH_MAX_OPERATIONS,
    C_NATIVE_GEOMETRY_TASK_MAX_OPERATIONS,
};
#[cfg(any(test, feature = "test-support"))]
pub use packing::CNativePackingCandidateBuffer;
pub use packing_candidate_sink::{
    NativeCandidateReducer, NativePackingCandidateConsumer, NativePackingCandidateContext,
    NativePackingCandidateSinkError,
};
#[cfg(any(test, feature = "test-support"))]
pub use packing_geometry_materializer::{
    CNativePackingGeometryPath, NativeGeometryMaterializationOutcome, NativeGeometryStreamOutcome,
    C_NATIVE_PACKING_GEOMETRY_PATH_MAX_OPERATIONS,
};
#[cfg(any(test, feature = "test-support"))]
pub use packing_outcome::{NativePackingOutcome, NativePackingStreamOutcome};
pub use pruning::{
    CNativePruningMinimalRecord, CNativePruningProofLedger, CNativePruningProofLedgerEntry,
    NativePruningEvidence, NativePruningLedger, NativePruningLedgerError,
    NativePruningMinimalRecord, C_NATIVE_PRUNING_LEDGER_MAX_ENTRIES,
    C_NATIVE_PRUNING_MINIMAL_RECORD_MAX_ENTRIES, C_NATIVE_PRUNING_REASON_COUNT,
};
pub use resource::CNativeResourceReport;
#[cfg(feature = "search-stage-profiling")]
pub use search_profile::{
    NativeSearchProfileError, NativeSearchProfileSession, NativeSearchProfileStage,
};

pub const C_NATIVE_PACKING_MAX_PIECES: usize = 15;
pub const C_NATIVE_PACKING_MAX_CANDIDATES: usize = 8192;
pub const C_NATIVE_BUILDUP_MAX_VARIANTS: usize =
    crate::raw::buildup_types::C_NATIVE_BUILDUP_MAX_VARIANTS;
pub const C_NATIVE_BUILDUP_MAX_OPERATIONS: usize =
    crate::raw::buildup_types::C_NATIVE_BUILDUP_MAX_OPERATIONS;
pub const C_NATIVE_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT: usize =
    crate::raw::buildup_types::C_NATIVE_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT;
pub const C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT: usize =
    C_NATIVE_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT;
pub const CLEARRA_CORE_ABI_VERSION_EXPECTED: i32 = crate::version::CLEARRA_CORE_ABI_VERSION;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeCoreError {
    Unavailable,
    AbiMismatch {
        expected: i32,
        actual: i32,
    },
    PackingStatus(i32),
    PackingIncomplete {
        status: i32,
        resource_report: clearra_core_domain::resource::ResourceReport,
    },
    BuildUpStatus(i32),
    InvalidPruningLedger(NativePruningLedgerError),
    ExecutionControlStatus(i32),
    ExecutionCancelled,
}

impl NativeCoreError {
    pub(crate) fn packing_with_resource_report(
        status: i32,
        resource_report: clearra_core_domain::resource::ResourceReport,
    ) -> Self {
        if resource_report.truncated || !resource_report.probability_complete {
            Self::PackingIncomplete {
                status,
                resource_report,
            }
        } else {
            Self::PackingStatus(status)
        }
    }
}

pub struct CoreCNative;

impl CoreCNative {
    pub fn compile_geometry_catalog(
        problem: &CPackingProblem,
    ) -> Result<NativeGeometryCatalog, NativeCoreError> {
        Self::ensure_abi()?;
        geometry_catalog::compile(problem)
    }

    pub fn compile_geometry_catalog_with_cancellation(
        problem: &CPackingProblem,
        cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
    ) -> Result<NativeGeometryCatalog, NativeCoreError> {
        Self::ensure_abi()?;
        geometry_catalog::compile_with_cancellation(problem, cancellation)
    }
}

impl CoreCNative {
    pub fn search_geometry_solution_graph(
        catalog: &NativeGeometryCatalog,
        problem: &CPackingProblem,
        cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
        evidence_policy: clearra_core_domain::pruning::PruningEvidencePolicy,
    ) -> Result<NativeGeometryGraphSearchOutcome, NativeCoreError> {
        Self::ensure_abi()?;
        #[cfg(feature = "native-c-core")]
        {
            NativeGeometrySolutionGraph::search_with_pruning_policy(
                catalog.clone(),
                problem,
                cancellation,
                evidence_policy,
            )
        }
        #[cfg(not(feature = "native-c-core"))]
        {
            let _ = (catalog, problem, cancellation, evidence_policy);
            Err(NativeCoreError::Unavailable)
        }
    }
}
impl CoreCNative {
    pub fn linked() -> bool {
        cfg!(feature = "native-c-core")
    }
}
#[cfg(any(test, feature = "test-support"))]
impl CoreCNative {
    pub fn stream_packing_candidates_with_cancellation(
        problem: &CPackingProblem,
        cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
        consumer: &mut dyn NativePackingCandidateConsumer,
    ) -> Result<NativePackingStreamOutcome, NativeCoreError> {
        Self::ensure_abi()?;
        packing::stream_packing_candidates(problem, cancellation, consumer)
    }

    pub fn stream_packing_candidates_partition_with_cancellation(
        problem: &CPackingProblem,
        partition_index: u16,
        partition_count: u16,
        partition_depth: u8,
        cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
        consumer: &mut dyn NativePackingCandidateConsumer,
    ) -> Result<NativePackingStreamOutcome, NativeCoreError> {
        Self::ensure_abi()?;
        packing::stream_packing_candidates_partition(
            problem,
            partition_index,
            partition_count,
            partition_depth,
            cancellation,
            consumer,
        )
    }
}
#[cfg(any(test, feature = "test-support"))]
impl CoreCNative {
    pub fn generate_packing_candidates_partition_with_cancellation(
        problem: &CPackingProblem,
        partition_index: u16,
        partition_count: u16,
        partition_depth: u8,
        cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
    ) -> Result<NativePackingOutcome, NativeCoreError> {
        Self::ensure_abi()?;
        packing::generate_packing_candidates_partition(
            problem,
            partition_index,
            partition_count,
            partition_depth,
            cancellation,
        )
    }
}
impl CoreCNative {
    pub fn abi_version() -> Result<i32, NativeCoreError> {
        abi_version()
    }
}
impl CoreCNative {
    pub fn ensure_abi() -> Result<(), NativeCoreError> {
        let actual = Self::abi_version()?;
        if actual == CLEARRA_CORE_ABI_VERSION_EXPECTED {
            Ok(())
        } else {
            Err(NativeCoreError::AbiMismatch {
                expected: CLEARRA_CORE_ABI_VERSION_EXPECTED,
                actual,
            })
        }
    }
}
#[cfg(any(test, feature = "test-support"))]
impl CoreCNative {
    pub fn generate_packing_candidates(
        problem: &CPackingProblem,
    ) -> Result<NativePackingOutcome, NativeCoreError> {
        let cancellation =
            clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new();
        Self::generate_packing_candidates_with_cancellation(problem, &cancellation)
    }
}
#[cfg(any(test, feature = "test-support"))]
impl CoreCNative {
    pub fn generate_packing_candidates_with_cancellation(
        problem: &CPackingProblem,
        cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
    ) -> Result<NativePackingOutcome, NativeCoreError> {
        Self::ensure_abi()?;
        packing::generate_packing_candidates(problem, cancellation)
    }
}
#[cfg(any(test, feature = "test-support"))]
impl CoreCNative {
    pub fn generate_packing_candidates_with_pruning_policy(
        problem: &CPackingProblem,
        evidence_policy: clearra_core_domain::pruning::PruningEvidencePolicy,
    ) -> Result<NativePackingOutcome, NativeCoreError> {
        Self::ensure_abi()?;
        packing::generate_packing_candidates_with_pruning_policy(problem, evidence_policy)
    }
}
impl CoreCNative {
    pub fn verify_buildup_problem(
        problem: &CBuildUpProblem,
    ) -> Result<NativeBuildUpOutcome, NativeCoreError> {
        Self::ensure_abi()?;
        let cancellation =
            clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new();
        buildup::verify_buildup_problem(problem, &cancellation)
    }
}
impl CoreCNative {
    pub fn verify_first_buildup_problem(
        problem: &CBuildUpProblem,
    ) -> Result<NativeBuildUpOutcome, NativeCoreError> {
        Self::ensure_abi()?;
        let cancellation =
            clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new();
        buildup::verify_first_buildup_problem(problem, &cancellation)
    }
}
impl CoreCNative {
    pub fn verify_first_buildup_problem_with_cancellation(
        problem: &CBuildUpProblem,
        cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
    ) -> Result<NativeBuildUpOutcome, NativeCoreError> {
        Self::ensure_abi()?;
        buildup::verify_first_buildup_problem(problem, cancellation)
    }
}
impl CoreCNative {
    pub fn enumerate_buildup_variants(
        problem: &CBuildUpProblem,
        limits: &CNativeBuildUpEnumerationLimits,
    ) -> Result<NativeBuildUpOutcome, NativeCoreError> {
        Self::ensure_abi()?;
        let cancellation =
            clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new();
        Self::enumerate_buildup_variants_with_cancellation(problem, limits, &cancellation)
    }
}
impl CoreCNative {
    pub fn enumerate_buildup_variants_with_cancellation(
        problem: &CBuildUpProblem,
        limits: &CNativeBuildUpEnumerationLimits,
        cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
    ) -> Result<NativeBuildUpOutcome, NativeCoreError> {
        Self::ensure_abi()?;
        buildup::enumerate_buildup_variants(problem, limits, cancellation)
    }
}
impl CoreCNative {
    pub fn count_buildup_variants(
        problem: &CBuildUpProblem,
        limits: &CNativeBuildUpCountLimits,
    ) -> Result<NativeBuildUpCountOutcome, NativeCoreError> {
        Self::ensure_abi()?;
        let cancellation =
            clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new();
        buildup::count_buildup_variants(problem, limits, &cancellation)
    }
}
impl CoreCNative {
    pub fn count_buildup_variants_with_cancellation(
        problem: &CBuildUpProblem,
        limits: &CNativeBuildUpCountLimits,
        cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
    ) -> Result<NativeBuildUpCountOutcome, NativeCoreError> {
        Self::ensure_abi()?;
        buildup::count_buildup_variants(problem, limits, cancellation)
    }
}
#[cfg(feature = "native-c-core")]
mod linked {
    use super::*;

    pub(super) fn abi_version() -> Result<i32, NativeCoreError> {
        Ok(crate::raw::bindings::abi_version())
    }
}

#[cfg(feature = "native-c-core")]
use linked::abi_version;

#[cfg(not(feature = "native-c-core"))]
fn abi_version() -> Result<i32, NativeCoreError> {
    Err(NativeCoreError::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_core_reports_unavailable_without_link_feature() {
        if !CoreCNative::linked() {
            assert_eq!(
                CoreCNative::abi_version(),
                Err(NativeCoreError::Unavailable)
            );
        }
    }
}
