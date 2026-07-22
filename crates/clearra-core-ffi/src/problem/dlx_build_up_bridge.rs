pub use super::dlx_buildup_bridge::{
    DlxBuildUpBridge, DlxBuildUpBridgeError, DlxBuildUpOperationCandidate,
};

pub fn dlx_result_maps_to_buildup_problem() -> bool {
    DlxBuildUpBridge::dlx_solution_is_not_build_variant()
}
