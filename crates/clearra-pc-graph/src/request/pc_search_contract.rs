use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_supply::queue::queue_observation_policy::QueueObservationPolicy;

pub const VISIBLE_SEVEN_MINIMUM_COVER_ERROR_CODE: &str = "visible-seven-minimum-cover-unsupported";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcSearchContractError {
    VisibleSevenMinimumCoverUnsupported,
}

impl PcSearchContractError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::VisibleSevenMinimumCoverUnsupported => VISIBLE_SEVEN_MINIMUM_COVER_ERROR_CODE,
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::VisibleSevenMinimumCoverUnsupported => {
                "minimum-cover is unavailable with visible-7 queue knowledge"
            }
        }
    }
}

pub fn validate_pc_observation_objective(
    observation: QueueObservationPolicy,
    objective: ObjectiveKind,
) -> Result<(), PcSearchContractError> {
    if observation.requires_observation_policy() && objective == ObjectiveKind::MinimumCover {
        Err(PcSearchContractError::VisibleSevenMinimumCoverUnsupported)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::objective::objective_kind::ObjectiveKind;
    use clearra_supply::queue::queue_observation_policy::QueueObservationPolicy;

    use super::{
        validate_pc_observation_objective, PcSearchContractError,
        VISIBLE_SEVEN_MINIMUM_COVER_ERROR_CODE,
    };

    #[test]
    fn visible_seven_minimum_cover_is_the_only_rejected_pair() {
        for observation in [
            QueueObservationPolicy::FullQueueOracle,
            QueueObservationPolicy::VisibleSeven,
        ] {
            for objective in [
                ObjectiveKind::All,
                ObjectiveKind::Unique,
                ObjectiveKind::MinimumCover,
                ObjectiveKind::Tiling,
            ] {
                let result = validate_pc_observation_objective(observation, objective);
                if observation == QueueObservationPolicy::VisibleSeven
                    && objective == ObjectiveKind::MinimumCover
                {
                    let error = result.expect_err("the unsupported pair must fail closed");
                    assert_eq!(
                        error,
                        PcSearchContractError::VisibleSevenMinimumCoverUnsupported
                    );
                    assert_eq!(error.code(), VISIBLE_SEVEN_MINIMUM_COVER_ERROR_CODE);
                } else {
                    result.expect("all supported observation/objective pairs");
                }
            }
        }
    }
}
