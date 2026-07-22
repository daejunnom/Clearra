use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
use clearra_core_ffi::{CPackingProblem, CoreCNative, NativeCoreError, NativePackingOutcome};

pub(crate) fn native_packing_outcome(
    compact_problem: &CPackingProblem,
    cancellation: &ExecutionCancellationToken,
) -> Result<Option<NativePackingOutcome>, NativeCoreError> {
    if !CoreCNative::linked() {
        return Ok(None);
    }
    let catalog =
        CoreCNative::compile_geometry_catalog_with_cancellation(compact_problem, cancellation)?;
    catalog
        .generate_partition(
            compact_problem,
            0,
            compact_problem.piece_multiset_family.count,
            0,
            1,
            1,
            cancellation,
        )
        .map(Some)
}
