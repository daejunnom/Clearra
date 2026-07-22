use super::BackendCapability;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CpuParallelGeometryExactCoverBackend;

impl CpuParallelGeometryExactCoverBackend {
    pub const BACKEND_ID: &'static str = "cpu-parallel-geometry-exact-cover";
    pub const EXECUTION_MODEL: &'static str = "immutable-geometry-graph-buildability-tasks";
    pub const SPLIT_POLICY: &'static str = "solution-family-balanced-task-split";
    pub const MEMO_POLICY: &'static str = "shared-suffix-family-worker-local-buildup-memo";
    pub const MERGE_POLICY: &'static str = "deterministic-merge";
}
impl CpuParallelGeometryExactCoverBackend {
    pub fn capability() -> BackendCapability {
        if cfg!(feature = "parallel") {
            BackendCapability::supported(Self::BACKEND_ID)
        } else {
            BackendCapability::disabled(Self::BACKEND_ID, "parallel_feature_disabled")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_exact_cover_uses_one_graph_and_deterministic_merge() {
        assert_eq!(
            CpuParallelGeometryExactCoverBackend::BACKEND_ID,
            "cpu-parallel-geometry-exact-cover"
        );
        assert_eq!(
            CpuParallelGeometryExactCoverBackend::EXECUTION_MODEL,
            "immutable-geometry-graph-buildability-tasks"
        );
        assert_eq!(
            CpuParallelGeometryExactCoverBackend::SPLIT_POLICY,
            "solution-family-balanced-task-split"
        );
        assert_eq!(
            CpuParallelGeometryExactCoverBackend::MEMO_POLICY,
            "shared-suffix-family-worker-local-buildup-memo"
        );
        assert_eq!(
            CpuParallelGeometryExactCoverBackend::MERGE_POLICY,
            "deterministic-merge"
        );
    }
}
