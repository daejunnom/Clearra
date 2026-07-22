use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::{dropdown::DropdownOption, i18n::LanguageSelectorSchema};

use super::{
    backend_options_schema::BackendOptionsSchema,
    execution_options_schema::ExecutionOptionsSchema,
    problem_preset_options_schema::ProblemPresetOptionsSchema,
    scenario_editor_schema::ScenarioEditorSchema,
    setup_filter_schema::SetupFilterSchema,
    setup_raw_metrics_schema::{SetupRawCoverageExportSchema, SetupRawMetricsSchema},
    setup_result_column_schema::SetupResultColumnSchema,
    setup_result_columns::{scenario_result_columns, setup_result_columns},
    spin_probability_columns::{spin_probability_columns, SpinProbabilityColumnSchema},
    spin_target_filter_schema::SpinTargetFilterSchema,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupExplorerSchema {
    filters: SetupFilterSchema,
    spin_target_filter: SpinTargetFilterSchema,
    execution_options: ExecutionOptionsSchema,
    backend_options: BackendOptionsSchema,
    language_selector: LanguageSelectorSchema,
    problem_preset_options: ProblemPresetOptionsSchema,
    scenario_editor: ScenarioEditorSchema,
    scenario_fixtures: Vec<DropdownOption>,
    result_columns: Vec<SetupResultColumnSchema>,
    scenario_result_columns: Vec<SetupResultColumnSchema>,
    spin_probability_columns: Vec<SpinProbabilityColumnSchema>,
    setup_raw_metrics_schema: SetupRawMetricsSchema,
    setup_raw_coverage_export_schema: SetupRawCoverageExportSchema,
}

impl SetupExplorerSchema {
    pub fn mvp() -> Self {
        Self::mvp2()
    }
}
impl SetupExplorerSchema {
    pub fn mvp2() -> Self {
        let execution_options = ExecutionOptionsSchema::mvp2();
        Self {
            filters: SetupFilterSchema::mvp(),
            spin_target_filter: SpinTargetFilterSchema::mvp2(),
            backend_options: BackendOptionsSchema::from_execution_options(&execution_options),
            execution_options,
            language_selector: LanguageSelectorSchema::mvp(),
            problem_preset_options: ProblemPresetOptionsSchema::m28(),
            scenario_editor: ScenarioEditorSchema::m28(),
            scenario_fixtures: scenario_fixture_options(),
            result_columns: setup_result_columns(),
            scenario_result_columns: scenario_result_columns(),
            spin_probability_columns: spin_probability_columns(),
            setup_raw_metrics_schema: SetupRawMetricsSchema::v2(),
            setup_raw_coverage_export_schema: SetupRawCoverageExportSchema::v2(),
        }
    }
}
impl SetupExplorerSchema {
    pub fn filters(&self) -> &SetupFilterSchema {
        &self.filters
    }
}
impl SetupExplorerSchema {
    pub fn spin_target_filter(&self) -> &SpinTargetFilterSchema {
        &self.spin_target_filter
    }
}
impl SetupExplorerSchema {
    pub fn execution_options(&self) -> &ExecutionOptionsSchema {
        &self.execution_options
    }
}
impl SetupExplorerSchema {
    pub fn backend_options(&self) -> &BackendOptionsSchema {
        &self.backend_options
    }
}
impl SetupExplorerSchema {
    pub fn language_selector(&self) -> &LanguageSelectorSchema {
        &self.language_selector
    }
}
impl SetupExplorerSchema {
    pub fn problem_preset_options(&self) -> &ProblemPresetOptionsSchema {
        &self.problem_preset_options
    }
}
impl SetupExplorerSchema {
    pub fn scenario_editor(&self) -> &ScenarioEditorSchema {
        &self.scenario_editor
    }
}
impl SetupExplorerSchema {
    pub fn scenario_fixtures(&self) -> &[DropdownOption] {
        &self.scenario_fixtures
    }
}
impl SetupExplorerSchema {
    pub fn result_columns(&self) -> &[SetupResultColumnSchema] {
        &self.result_columns
    }
}
impl SetupExplorerSchema {
    pub fn scenario_result_columns(&self) -> &[SetupResultColumnSchema] {
        &self.scenario_result_columns
    }
}
impl SetupExplorerSchema {
    pub fn spin_probability_columns(&self) -> &[SpinProbabilityColumnSchema] {
        &self.spin_probability_columns
    }
}
impl SetupExplorerSchema {
    pub fn setup_raw_metrics_schema(&self) -> &SetupRawMetricsSchema {
        &self.setup_raw_metrics_schema
    }
}
impl SetupExplorerSchema {
    pub fn setup_raw_coverage_export_schema(&self) -> &SetupRawCoverageExportSchema {
        &self.setup_raw_coverage_export_schema
    }
}

impl Default for SetupExplorerSchema {
    fn default() -> Self {
        Self::mvp2()
    }
}

fn scenario_fixture_options() -> Vec<DropdownOption> {
    vec![
        DropdownOption::new("tests/fixtures/pc/example.json", "Example setup PC"),
        DropdownOption::new(
            "tests/fixtures/pc/requires_180_unsupported.json",
            "Requires 180 setup PC",
        )
        .disabled_for(
            DiagnosticCode::EPcQueryInvalid,
            "scenario_requires_180_unsupported",
        ),
    ]
}
