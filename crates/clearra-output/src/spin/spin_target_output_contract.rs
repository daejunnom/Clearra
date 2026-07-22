#[derive(Clone, Debug, PartialEq)]
pub struct SpinTargetOutputContract {
    spin_target_id: String,
    score_profile_id: Option<String>,
    target_probability_threshold: Option<f64>,
    trace_requirement: String,
    classifier_id: String,
    exact: bool,
    trace_completeness: String,
    probability_complete: bool,
    diagnostic_code: Option<String>,
    coverage_reducer: String,
}

impl SpinTargetOutputContract {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        spin_target_id: impl Into<String>,
        score_profile_id: Option<String>,
        target_probability_threshold: Option<f64>,
        trace_requirement: impl Into<String>,
        classifier_id: impl Into<String>,
        exact: bool,
        trace_completeness: impl Into<String>,
        probability_complete: bool,
        diagnostic_code: Option<String>,
    ) -> Self {
        Self {
            spin_target_id: spin_target_id.into(),
            score_profile_id,
            target_probability_threshold,
            trace_requirement: trace_requirement.into(),
            classifier_id: classifier_id.into(),
            exact,
            trace_completeness: trace_completeness.into(),
            probability_complete,
            diagnostic_code,
            coverage_reducer: "PatternBitSet OR union".to_owned(),
        }
    }
}
impl SpinTargetOutputContract {
    pub fn missing_kick_evidence(
        spin_target_id: impl Into<String>,
        classifier_id: impl Into<String>,
    ) -> Self {
        Self::new(
            spin_target_id,
            None,
            None,
            "kick-evidence-required",
            classifier_id,
            false,
            "missing-kick-evidence",
            false,
            Some("W_SPIN_TARGET_PROBABILITY_INCOMPLETE".to_owned()),
        )
    }
}
impl SpinTargetOutputContract {
    pub fn spin_target_id(&self) -> &str {
        &self.spin_target_id
    }
}
impl SpinTargetOutputContract {
    pub fn score_profile_id(&self) -> Option<&str> {
        self.score_profile_id.as_deref()
    }
}
impl SpinTargetOutputContract {
    pub fn target_probability_threshold(&self) -> Option<f64> {
        self.target_probability_threshold
    }
}
impl SpinTargetOutputContract {
    pub fn trace_requirement(&self) -> &str {
        &self.trace_requirement
    }
}
impl SpinTargetOutputContract {
    pub fn classifier_id(&self) -> &str {
        &self.classifier_id
    }
}
impl SpinTargetOutputContract {
    pub fn exact(&self) -> bool {
        self.exact
    }
}
impl SpinTargetOutputContract {
    pub fn trace_completeness(&self) -> &str {
        &self.trace_completeness
    }
}
impl SpinTargetOutputContract {
    pub fn probability_complete(&self) -> bool {
        self.probability_complete
    }
}
impl SpinTargetOutputContract {
    pub fn diagnostic_code(&self) -> Option<&str> {
        self.diagnostic_code.as_deref()
    }
}
impl SpinTargetOutputContract {
    pub fn coverage_reducer(&self) -> &str {
        &self.coverage_reducer
    }
}

#[cfg(test)]
#[path = "spin_target_output_contract_tests.rs"]
mod tests;
