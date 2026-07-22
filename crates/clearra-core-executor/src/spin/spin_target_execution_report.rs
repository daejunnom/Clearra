#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinTargetExecutionReport {
    build_variant_count: usize,
    evaluated_build_variant_count: usize,
    satisfied_build_variant_count: usize,
    coverage_row_count: usize,
    classifier_used: bool,
    replay_basis: &'static str,
    probability_complete: bool,
    exact: bool,
    trace_completeness: &'static str,
    diagnostic_code: Option<&'static str>,
}

impl SpinTargetExecutionReport {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        build_variant_count: usize,
        evaluated_build_variant_count: usize,
        satisfied_build_variant_count: usize,
        coverage_row_count: usize,
        classifier_used: bool,
        replay_basis: &'static str,
        probability_complete: bool,
        exact: bool,
        trace_completeness: &'static str,
        diagnostic_code: Option<&'static str>,
    ) -> Self {
        Self {
            build_variant_count,
            evaluated_build_variant_count,
            satisfied_build_variant_count,
            coverage_row_count,
            classifier_used,
            replay_basis,
            probability_complete,
            exact,
            trace_completeness,
            diagnostic_code,
        }
    }
}
impl SpinTargetExecutionReport {
    pub fn build_variant_count(&self) -> usize {
        self.build_variant_count
    }
}
impl SpinTargetExecutionReport {
    pub fn evaluated_build_variant_count(&self) -> usize {
        self.evaluated_build_variant_count
    }
}
impl SpinTargetExecutionReport {
    pub fn satisfied_build_variant_count(&self) -> usize {
        self.satisfied_build_variant_count
    }
}
impl SpinTargetExecutionReport {
    pub fn coverage_row_count(&self) -> usize {
        self.coverage_row_count
    }
}
impl SpinTargetExecutionReport {
    pub fn classifier_used(&self) -> bool {
        self.classifier_used
    }
}
impl SpinTargetExecutionReport {
    pub fn replay_basis(&self) -> &'static str {
        self.replay_basis
    }
}
impl SpinTargetExecutionReport {
    pub fn probability_complete(&self) -> bool {
        self.probability_complete
    }
}
impl SpinTargetExecutionReport {
    pub fn exact(&self) -> bool {
        self.exact
    }
}
impl SpinTargetExecutionReport {
    pub fn trace_completeness(&self) -> &'static str {
        self.trace_completeness
    }
}
impl SpinTargetExecutionReport {
    pub fn diagnostic_code(&self) -> Option<&'static str> {
        self.diagnostic_code
    }
}
