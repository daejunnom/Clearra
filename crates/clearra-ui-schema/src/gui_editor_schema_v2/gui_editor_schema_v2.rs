use crate::{
    BackendOptionsSchema, BuildEditorSchema, ProblemPresetOptionsSchema, RuleEditorSchema,
    ScenarioEditorSchema, ScoreProfileEditorSchema, SetupExplorerSchema,
};

use super::{
    diagnostic_panel_schema::DiagnosticPanelSchema,
    gui_contract_field_schema::GuiContractFieldSchema, render_options_schema::RenderOptionsSchema,
};

#[derive(Clone, Debug, PartialEq)]
pub struct GuiEditorSchemaV2 {
    backend_options: BackendOptionsSchema,
    problem_preset_options: ProblemPresetOptionsSchema,
    scenario_editor: ScenarioEditorSchema,
    setup_explorer: SetupExplorerSchema,
    build_editor: BuildEditorSchema,
    rule_editor: RuleEditorSchema,
    score_editor: ScoreProfileEditorSchema,
    render_options: RenderOptionsSchema,
    diagnostic_panel: DiagnosticPanelSchema,
    required_display_fields: Vec<GuiContractFieldSchema>,
    json_contract_keys_localized: bool,
}

impl GuiEditorSchemaV2 {
    pub fn v2() -> Self {
        let setup_explorer = SetupExplorerSchema::mvp2();
        Self {
            backend_options: setup_explorer.backend_options().clone(),
            problem_preset_options: setup_explorer.problem_preset_options().clone(),
            scenario_editor: setup_explorer.scenario_editor().clone(),
            setup_explorer,
            build_editor: BuildEditorSchema::mvp_template_slots(2),
            rule_editor: RuleEditorSchema::mvp2(),
            score_editor: ScoreProfileEditorSchema::mvp2(),
            render_options: RenderOptionsSchema::v2(),
            diagnostic_panel: DiagnosticPanelSchema::v2(),
            required_display_fields: required_display_fields(),
            json_contract_keys_localized: false,
        }
    }
}
impl GuiEditorSchemaV2 {
    pub fn backend_options(&self) -> &BackendOptionsSchema {
        &self.backend_options
    }
}
impl GuiEditorSchemaV2 {
    pub fn problem_preset_options(&self) -> &ProblemPresetOptionsSchema {
        &self.problem_preset_options
    }
}
impl GuiEditorSchemaV2 {
    pub fn scenario_editor(&self) -> &ScenarioEditorSchema {
        &self.scenario_editor
    }
}
impl GuiEditorSchemaV2 {
    pub fn setup_explorer(&self) -> &SetupExplorerSchema {
        &self.setup_explorer
    }
}
impl GuiEditorSchemaV2 {
    pub fn build_editor(&self) -> &BuildEditorSchema {
        &self.build_editor
    }
}
impl GuiEditorSchemaV2 {
    pub fn rule_editor(&self) -> &RuleEditorSchema {
        &self.rule_editor
    }
}
impl GuiEditorSchemaV2 {
    pub fn score_editor(&self) -> &ScoreProfileEditorSchema {
        &self.score_editor
    }
}
impl GuiEditorSchemaV2 {
    pub fn render_options(&self) -> &RenderOptionsSchema {
        &self.render_options
    }
}
impl GuiEditorSchemaV2 {
    pub fn diagnostic_panel(&self) -> &DiagnosticPanelSchema {
        &self.diagnostic_panel
    }
}
impl GuiEditorSchemaV2 {
    pub fn required_display_fields(&self) -> &[GuiContractFieldSchema] {
        &self.required_display_fields
    }
}
impl GuiEditorSchemaV2 {
    pub const fn json_contract_keys_localized(&self) -> bool {
        self.json_contract_keys_localized
    }
}
impl GuiEditorSchemaV2 {
    pub fn exposes_required_field(&self, field: &str) -> bool {
        self.required_display_fields
            .iter()
            .any(|schema| schema.contract_key() == field)
    }
}
impl GuiEditorSchemaV2 {
    pub fn backend_result_exposes(&self, field: &str) -> bool {
        self.backend_options
            .result_contract_fields()
            .iter()
            .any(|known| known == field)
    }
}
impl GuiEditorSchemaV2 {
    pub fn score_result_exposes(&self, field: &str) -> bool {
        self.score_editor
            .result_contract_fields()
            .iter()
            .any(|known| known == field)
    }
}

impl Default for GuiEditorSchemaV2 {
    fn default() -> Self {
        Self::v2()
    }
}

fn required_display_fields() -> Vec<GuiContractFieldSchema> {
    vec![
        GuiContractFieldSchema::new("backend_requested", "Backend requested", "backend option"),
        GuiContractFieldSchema::new("backend_selected", "Backend selected", "backend option"),
        GuiContractFieldSchema::new(
            "backend_fallback_reason",
            "Fallback reason",
            "fallback reason",
        ),
        GuiContractFieldSchema::new("gpu_trust_state", "GPU trust", "gpu trust state"),
        GuiContractFieldSchema::new(
            "packing_candidate_count",
            "Packing candidates",
            "packing candidate count",
        ),
        GuiContractFieldSchema::new(
            "build_variant_count",
            "Build variants",
            "build variant count",
        ),
        GuiContractFieldSchema::new("total_solution_count", "Total solutions", "solution count"),
        GuiContractFieldSchema::new("retained_trace_count", "Retained traces", "trace retention"),
        GuiContractFieldSchema::new(
            "coverage_probability",
            "Coverage probability",
            "coverage probability",
        ),
        GuiContractFieldSchema::new(
            "raw_coverage_export_path",
            "Raw metrics export",
            "raw metrics export",
        ),
        GuiContractFieldSchema::new("score_basis", "Score basis", "score basis"),
        GuiContractFieldSchema::new(
            "score_accuracy_level",
            "Score accuracy",
            "score accuracy level",
        ),
        GuiContractFieldSchema::new(
            "unsupported_reason",
            "Unsupported reason",
            "unsupported reason",
        ),
        GuiContractFieldSchema::new(
            "renderer_capability",
            "Renderer capability",
            "renderer capability",
        ),
    ]
}

#[cfg(test)]
#[path = "gui_editor_schema_v2_tests.rs"]
mod tests;
