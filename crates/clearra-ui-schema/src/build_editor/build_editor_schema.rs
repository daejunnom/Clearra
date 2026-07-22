use clearra_build_coverage::template::{BuildSlot, BuildSlotId, BuildTemplate};
use clearra_core_domain::board::cell::CellCoord;
use clearra_profiles::pieces::standard_tetrominoes::standard_tetromino_piece_set_profile;
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use super::{
    build_coverage_summary_schema::BuildCoverageSummarySchema,
    build_field_schema::{BuildFieldSchema, BuildFieldType},
    build_preview_board_schema::BuildPreviewBoardSchema,
    build_slot_schema::BuildSlotSchema,
    build_validation_schema::BuildValidationDiagnosticSchema,
};

#[derive(Clone, Debug, PartialEq)]
pub struct BuildEditorSchema {
    template_id: String,
    template_label: Option<String>,
    board_width: u16,
    board_height: u16,
    fields: Vec<BuildFieldSchema>,
    slots: Vec<BuildSlotSchema>,
    preview_board: BuildPreviewBoardSchema,
    validation_diagnostics: Vec<BuildValidationDiagnosticSchema>,
    coverage_summary: BuildCoverageSummarySchema,
    result_contract_fields: Vec<String>,
    custom_domains_enabled: bool,
}

impl BuildEditorSchema {
    pub fn mvp_template_slots(slot_count: usize) -> Self {
        let standard = standard_tetromino_piece_set_profile().pieces().to_vec();
        let slots = (0..slot_count)
            .map(|index| {
                BuildSlot::new(
                    BuildSlotId::new(index as u32),
                    vec![CellCoord::new_unchecked(index as u16, 0)],
                )
                .with_allowed_pieces(standard.clone())
            })
            .collect::<Vec<_>>();

        Self::from_template(&BuildTemplate::new("mvp-build-template", slots))
    }
}
impl BuildEditorSchema {
    pub fn from_template(template: &BuildTemplate) -> Self {
        Self {
            template_id: template.id().to_owned(),
            template_label: template.label().map(str::to_owned),
            board_width: template.board_size().width(),
            board_height: template.board_size().height(),
            fields: template_fields(),
            slots: template
                .slots()
                .iter()
                .map(BuildSlotSchema::from_build_slot)
                .collect(),
            preview_board: BuildPreviewBoardSchema::from_template(template),
            validation_diagnostics: Vec::new(),
            coverage_summary: BuildCoverageSummarySchema::empty(),
            result_contract_fields: build_result_contract_fields(),
            custom_domains_enabled: true,
        }
    }
}
impl BuildEditorSchema {
    pub fn with_validation_report(mut self, report: &DiagnosticReport) -> Self {
        self.validation_diagnostics = BuildValidationDiagnosticSchema::from_report(report);
        self
    }
}
impl BuildEditorSchema {
    pub fn with_coverage_summary(mut self, coverage_summary: BuildCoverageSummarySchema) -> Self {
        self.coverage_summary = coverage_summary;
        self
    }
}
impl BuildEditorSchema {
    pub fn template_id(&self) -> &str {
        &self.template_id
    }
}
impl BuildEditorSchema {
    pub fn template_label(&self) -> Option<&str> {
        self.template_label.as_deref()
    }
}
impl BuildEditorSchema {
    pub fn board_width(&self) -> u16 {
        self.board_width
    }
}
impl BuildEditorSchema {
    pub fn board_height(&self) -> u16 {
        self.board_height
    }
}
impl BuildEditorSchema {
    pub fn fields(&self) -> &[BuildFieldSchema] {
        &self.fields
    }
}
impl BuildEditorSchema {
    pub fn slots(&self) -> &[BuildSlotSchema] {
        &self.slots
    }
}
impl BuildEditorSchema {
    pub fn preview_board(&self) -> &BuildPreviewBoardSchema {
        &self.preview_board
    }
}
impl BuildEditorSchema {
    pub fn validation_diagnostics(&self) -> &[BuildValidationDiagnosticSchema] {
        &self.validation_diagnostics
    }
}
impl BuildEditorSchema {
    pub fn coverage_summary(&self) -> &BuildCoverageSummarySchema {
        &self.coverage_summary
    }
}
impl BuildEditorSchema {
    pub fn result_contract_fields(&self) -> &[String] {
        &self.result_contract_fields
    }
}
impl BuildEditorSchema {
    pub fn custom_domains_enabled(&self) -> bool {
        self.custom_domains_enabled
    }
}

impl Default for BuildEditorSchema {
    fn default() -> Self {
        Self::mvp_template_slots(0)
    }
}

fn template_fields() -> Vec<BuildFieldSchema> {
    vec![
        BuildFieldSchema::new(
            "template_id",
            "Template id",
            BuildFieldType::Text,
            true,
            Vec::new(),
        ),
        BuildFieldSchema::new("label", "Label", BuildFieldType::Text, false, Vec::new()),
        BuildFieldSchema::new(
            "board_width",
            "Board width",
            BuildFieldType::Number,
            true,
            Vec::new(),
        ),
        BuildFieldSchema::new(
            "board_height",
            "Board height",
            BuildFieldType::Number,
            true,
            Vec::new(),
        ),
    ]
}

fn build_result_contract_fields() -> Vec<String> {
    [
        "packing_candidate_count",
        "build_variant_count",
        "total_solution_count",
        "retained_trace_count",
        "coverage_probability",
        "coverage_row_count",
        "raw_coverage_export_path",
        "backend_fallback_reason",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[cfg(test)]
#[path = "build_editor_schema_tests.rs"]
mod tests;
