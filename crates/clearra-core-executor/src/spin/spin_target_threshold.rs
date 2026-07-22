use clearra_scoring::spin::SpinTarget;

use crate::spin::SpinProbabilityResult;

pub(crate) fn threshold_satisfied(
    spin_target: &SpinTarget,
    probability_result: &SpinProbabilityResult,
) -> Option<bool> {
    spin_target
        .target_probability_threshold()
        .map(|threshold| probability_result.probability() >= threshold)
}
