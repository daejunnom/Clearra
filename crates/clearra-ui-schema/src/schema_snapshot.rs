use crate::{BuildEditorSchema, RuleEditorSchema, ScoreProfileEditorSchema, SetupExplorerSchema};

pub const UI_SCHEMA_SNAPSHOT_VERSION: u16 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiSchemaSnapshot {
    version: u16,
    rule_preset_count: usize,
    kick_preview_count: usize,
    score_profile_count: usize,
    score_field_count: usize,
    spin_target_option_count: usize,
    special_spin_case_count: usize,
    score_expectation_scope_count: usize,
    spin_probability_column_count: usize,
    setup_result_column_count: usize,
    scenario_fixture_count: usize,
    scenario_result_column_count: usize,
    execution_backend_option_count: usize,
    execution_backend_preset_count: usize,
    language_option_count: usize,
    backend_result_contract_field_count: usize,
    problem_preset_option_count: usize,
    scenario_editor_field_count: usize,
    build_field_count: usize,
    build_slot_count: usize,
    custom_domains_enabled: bool,
}

impl UiSchemaSnapshot {
    pub fn mvp2() -> Self {
        let rule_editor = RuleEditorSchema::mvp2();
        let score_editor = ScoreProfileEditorSchema::mvp2();
        let setup_explorer = SetupExplorerSchema::mvp2();
        let build_editor = BuildEditorSchema::mvp_template_slots(2);

        Self {
            version: UI_SCHEMA_SNAPSHOT_VERSION,
            rule_preset_count: rule_editor.presets().len(),
            kick_preview_count: rule_editor.kick_table().previews().len(),
            score_profile_count: score_editor.profiles().len(),
            score_field_count: score_editor.fields().len(),
            spin_target_option_count: score_editor.spin_target_schema().options().len(),
            special_spin_case_count: score_editor.special_spin_case_schema().cases().len(),
            score_expectation_scope_count: score_editor
                .score_expectation_schema()
                .aggregation_scope_options()
                .len(),
            spin_probability_column_count: setup_explorer.spin_probability_columns().len(),
            setup_result_column_count: setup_explorer.result_columns().len(),
            scenario_fixture_count: setup_explorer.scenario_fixtures().len(),
            scenario_result_column_count: setup_explorer.scenario_result_columns().len(),
            execution_backend_option_count: setup_explorer
                .execution_options()
                .backend_options()
                .len(),
            execution_backend_preset_count: setup_explorer
                .execution_options()
                .backend_presets()
                .len(),
            language_option_count: setup_explorer.language_selector().options().len(),
            backend_result_contract_field_count: setup_explorer
                .backend_options()
                .result_contract_fields()
                .len(),
            problem_preset_option_count: setup_explorer.problem_preset_options().options().len(),
            scenario_editor_field_count: setup_explorer.scenario_editor().fields().len(),
            build_field_count: build_editor.fields().len(),
            build_slot_count: build_editor.slots().len(),
            custom_domains_enabled: build_editor.custom_domains_enabled(),
        }
    }
}
impl UiSchemaSnapshot {
    pub fn version(self) -> u16 {
        self.version
    }
}
impl UiSchemaSnapshot {
    pub fn rule_preset_count(self) -> usize {
        self.rule_preset_count
    }
}
impl UiSchemaSnapshot {
    pub fn kick_preview_count(self) -> usize {
        self.kick_preview_count
    }
}
impl UiSchemaSnapshot {
    pub fn score_profile_count(self) -> usize {
        self.score_profile_count
    }
}
impl UiSchemaSnapshot {
    pub fn score_field_count(self) -> usize {
        self.score_field_count
    }
}
impl UiSchemaSnapshot {
    pub fn spin_target_option_count(self) -> usize {
        self.spin_target_option_count
    }
}
impl UiSchemaSnapshot {
    pub fn special_spin_case_count(self) -> usize {
        self.special_spin_case_count
    }
}
impl UiSchemaSnapshot {
    pub fn score_expectation_scope_count(self) -> usize {
        self.score_expectation_scope_count
    }
}
impl UiSchemaSnapshot {
    pub fn spin_probability_column_count(self) -> usize {
        self.spin_probability_column_count
    }
}
impl UiSchemaSnapshot {
    pub fn setup_result_column_count(self) -> usize {
        self.setup_result_column_count
    }
}
impl UiSchemaSnapshot {
    pub fn scenario_fixture_count(self) -> usize {
        self.scenario_fixture_count
    }
}
impl UiSchemaSnapshot {
    pub fn scenario_result_column_count(self) -> usize {
        self.scenario_result_column_count
    }
}
impl UiSchemaSnapshot {
    pub fn execution_backend_option_count(self) -> usize {
        self.execution_backend_option_count
    }
}
impl UiSchemaSnapshot {
    pub fn execution_backend_preset_count(self) -> usize {
        self.execution_backend_preset_count
    }
}
impl UiSchemaSnapshot {
    pub fn language_option_count(self) -> usize {
        self.language_option_count
    }
}
impl UiSchemaSnapshot {
    pub fn backend_result_contract_field_count(self) -> usize {
        self.backend_result_contract_field_count
    }
}
impl UiSchemaSnapshot {
    pub fn problem_preset_option_count(self) -> usize {
        self.problem_preset_option_count
    }
}
impl UiSchemaSnapshot {
    pub fn scenario_editor_field_count(self) -> usize {
        self.scenario_editor_field_count
    }
}
impl UiSchemaSnapshot {
    pub fn build_field_count(self) -> usize {
        self.build_field_count
    }
}
impl UiSchemaSnapshot {
    pub fn build_slot_count(self) -> usize {
        self.build_slot_count
    }
}
impl UiSchemaSnapshot {
    pub fn custom_domains_enabled(self) -> bool {
        self.custom_domains_enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_schema_snapshot_pins_mvp2_surface_counts() {
        let snapshot = UiSchemaSnapshot::mvp2();

        assert_eq!(snapshot.version(), UI_SCHEMA_SNAPSHOT_VERSION);
        assert_eq!(snapshot.rule_preset_count(), 8);
        assert_eq!(snapshot.kick_preview_count(), 6);
        assert_eq!(snapshot.score_profile_count(), 6);
        assert_eq!(snapshot.score_field_count(), 6);
        assert_eq!(snapshot.spin_target_option_count(), 7);
        assert_eq!(snapshot.special_spin_case_count(), 3);
        assert_eq!(snapshot.score_expectation_scope_count(), 3);
        assert_eq!(snapshot.spin_probability_column_count(), 15);
        assert_eq!(snapshot.setup_result_column_count(), 49);
        assert_eq!(snapshot.scenario_fixture_count(), 2);
        assert_eq!(snapshot.scenario_result_column_count(), 40);
        assert_eq!(snapshot.execution_backend_option_count(), 4);
        assert_eq!(snapshot.execution_backend_preset_count(), 4);
        assert_eq!(snapshot.language_option_count(), 2);
        assert_eq!(snapshot.backend_result_contract_field_count(), 28);
        assert_eq!(snapshot.problem_preset_option_count(), 4);
        assert_eq!(snapshot.scenario_editor_field_count(), 14);
        assert_eq!(snapshot.build_field_count(), 4);
        assert_eq!(snapshot.build_slot_count(), 2);
        assert!(snapshot.custom_domains_enabled());
    }
}
