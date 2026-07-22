use crate::score_editor::SpinTargetSchema;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinTargetFilterSchema {
    enabled: bool,
    spin_target_schema: SpinTargetSchema,
    probability_threshold_enabled: bool,
    accuracy_filter_options: Vec<&'static str>,
}

impl SpinTargetFilterSchema {
    pub fn mvp2() -> Self {
        Self {
            enabled: true,
            spin_target_schema: SpinTargetSchema::mvp2(),
            probability_threshold_enabled: true,
            accuracy_filter_options: vec!["exact-only", "allow-estimate", "require-kick-evidence"],
        }
    }
}
impl SpinTargetFilterSchema {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
}
impl SpinTargetFilterSchema {
    pub fn spin_target_schema(&self) -> &SpinTargetSchema {
        &self.spin_target_schema
    }
}
impl SpinTargetFilterSchema {
    pub fn probability_threshold_enabled(&self) -> bool {
        self.probability_threshold_enabled
    }
}
impl SpinTargetFilterSchema {
    pub fn accuracy_filter_options(&self) -> &[&'static str] {
        &self.accuracy_filter_options
    }
}

impl Default for SpinTargetFilterSchema {
    fn default() -> Self {
        Self::mvp2()
    }
}
