use clearra_core_ffi::CBuildVariantViewError;
use clearra_coverage::matrix::coverage_matrix::CoverageMatrixError;
use clearra_replay::ReplayEngineError;

use crate::spin::{
    spin_target_coverage_bridge::SpinTargetCoverageBridgeError,
    spin_target_result_reducer::SpinTargetResultReducerError,
};

#[derive(Clone, Debug, PartialEq)]
pub enum SpinTargetRunnerError {
    MissingSpinClassifier,
    BuildVariant(CBuildVariantViewError),
    Replay(ReplayEngineError),
    MissingSpinBasis,
    CoverageBridge(SpinTargetCoverageBridgeError),
    CoverageMatrix(CoverageMatrixError),
    ResultReducer(SpinTargetResultReducerError),
}
