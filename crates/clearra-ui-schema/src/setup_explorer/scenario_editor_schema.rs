use clearra_pc_graph::request::PcCompletionGoal;
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::{disabled_reason::UiDisabledReason, dropdown::DropdownOption};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioEditorSchema {
    fields: Vec<ScenarioEditorFieldSchema>,
    result_contract_fields: Vec<String>,
    unsupported_reason_field: String,
}

impl ScenarioEditorSchema {
    pub fn m28() -> Self {
        Self {
            fields: scenario_fields(),
            result_contract_fields: scenario_result_contract_fields(),
            unsupported_reason_field: "search_unsupported_reason".to_owned(),
        }
    }
}
impl ScenarioEditorSchema {
    pub fn fields(&self) -> &[ScenarioEditorFieldSchema] {
        &self.fields
    }
}
impl ScenarioEditorSchema {
    pub fn result_contract_fields(&self) -> &[String] {
        &self.result_contract_fields
    }
}
impl ScenarioEditorSchema {
    pub fn unsupported_reason_field(&self) -> &str {
        &self.unsupported_reason_field
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioEditorFieldSchema {
    id: String,
    label: String,
    field_type: ScenarioEditorFieldType,
    required: bool,
    default_value: Option<String>,
    options: Vec<DropdownOption>,
    unsupported_reason: Option<UiDisabledReason>,
}

impl ScenarioEditorFieldSchema {
    fn new(id: &str, label: &str, field_type: ScenarioEditorFieldType, required: bool) -> Self {
        Self {
            id: id.to_owned(),
            label: label.to_owned(),
            field_type,
            required,
            default_value: None,
            options: Vec::new(),
            unsupported_reason: None,
        }
    }
}
impl ScenarioEditorFieldSchema {
    fn with_default(mut self, value: impl Into<String>) -> Self {
        self.default_value = Some(value.into());
        self
    }
}
impl ScenarioEditorFieldSchema {
    fn with_options(mut self, options: Vec<DropdownOption>) -> Self {
        self.options = options;
        self
    }
}
impl ScenarioEditorFieldSchema {
    fn unsupported_for(mut self, code: DiagnosticCode, reason: impl Into<String>) -> Self {
        self.unsupported_reason = Some(UiDisabledReason::new(code, reason));
        self
    }
}
impl ScenarioEditorFieldSchema {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl ScenarioEditorFieldSchema {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl ScenarioEditorFieldSchema {
    pub fn field_type(&self) -> ScenarioEditorFieldType {
        self.field_type
    }
}
impl ScenarioEditorFieldSchema {
    pub fn required(&self) -> bool {
        self.required
    }
}
impl ScenarioEditorFieldSchema {
    pub fn default_value(&self) -> Option<&str> {
        self.default_value.as_deref()
    }
}
impl ScenarioEditorFieldSchema {
    pub fn options(&self) -> &[DropdownOption] {
        &self.options
    }
}
impl ScenarioEditorFieldSchema {
    pub fn unsupported_reason(&self) -> Option<&UiDisabledReason> {
        self.unsupported_reason.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioEditorFieldType {
    Fixture,
    BoardMask,
    PieceSequence,
    Select,
    Number,
    Toggle,
}

fn scenario_fields() -> Vec<ScenarioEditorFieldSchema> {
    vec![
        ScenarioEditorFieldSchema::new(
            "fixture",
            "Fixture",
            ScenarioEditorFieldType::Fixture,
            false,
        ),
        ScenarioEditorFieldSchema::new(
            "board_width",
            "Board width",
            ScenarioEditorFieldType::Number,
            true,
        )
        .with_default("10"),
        ScenarioEditorFieldSchema::new(
            "initial_board_mask",
            "Initial board mask",
            ScenarioEditorFieldType::BoardMask,
            true,
        )
        .with_default("0x0000000000000000"),
        ScenarioEditorFieldSchema::new(
            "remaining_queue",
            "Remaining queue",
            ScenarioEditorFieldType::PieceSequence,
            true,
        ),
        ScenarioEditorFieldSchema::new("hold", "Hold", ScenarioEditorFieldType::Select, false)
            .with_default("none")
            .with_options(piece_or_none_options()),
        ScenarioEditorFieldSchema::new(
            "completion_goal",
            "Goal",
            ScenarioEditorFieldType::Select,
            true,
        )
        .with_default(PcCompletionGoal::ClearToEmpty.as_str())
        .with_options(vec![DropdownOption::new(
            PcCompletionGoal::ClearToEmpty.as_str(),
            "Clear to empty",
        )]),
        ScenarioEditorFieldSchema::new(
            "max_pieces",
            "Max pieces",
            ScenarioEditorFieldType::Number,
            true,
        )
        .with_default("6"),
        ScenarioEditorFieldSchema::new(
            "exact_pieces",
            "Exact pieces",
            ScenarioEditorFieldType::Number,
            false,
        ),
        ScenarioEditorFieldSchema::new(
            "min_remaining_queue",
            "Min remaining queue",
            ScenarioEditorFieldType::Number,
            false,
        )
        .with_default("0"),
        ScenarioEditorFieldSchema::new(
            "allow_hold",
            "Allow hold",
            ScenarioEditorFieldType::Toggle,
            false,
        )
        .with_default("true"),
        ScenarioEditorFieldSchema::new(
            "requires_180",
            "Requires 180",
            ScenarioEditorFieldType::Toggle,
            false,
        )
        .with_default("false")
        .unsupported_for(
            DiagnosticCode::EPcQueryInvalid,
            "scenario_requires_180_unsupported",
        ),
        ScenarioEditorFieldSchema::new(
            "rule_profile",
            "Rule",
            ScenarioEditorFieldType::Select,
            true,
        )
        .with_default(RuleProfileId::SrsPlus.as_str())
        .with_options(rule_options()),
        ScenarioEditorFieldSchema::new(
            "count_policy",
            "Count policy",
            ScenarioEditorFieldType::Select,
            true,
        )
        .with_default("count-all")
        .with_options(count_policy_options()),
        ScenarioEditorFieldSchema::new(
            "retained_trace_limit",
            "Retained trace limit",
            ScenarioEditorFieldType::Number,
            false,
        )
        .with_default("64"),
    ]
}

fn scenario_result_contract_fields() -> Vec<String> {
    [
        "solution_found",
        "total_solution_count",
        "unique_solution_count",
        "retained_trace_count",
        "coverage_probability",
        "packing_candidate_count",
        "build_variant_count",
        "backend_fallback_reason",
        "score_evaluation_basis",
        "search_unsupported_reason",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn rule_options() -> Vec<DropdownOption> {
    [
        RuleProfileId::SrsPlus,
        RuleProfileId::Srs,
        RuleProfileId::NoKick,
        RuleProfileId::SrsX,
        RuleProfileId::Asc,
        RuleProfileId::Ars,
    ]
    .into_iter()
    .map(|rule| DropdownOption::new(rule.as_str(), rule.as_str()))
    .collect()
}

fn count_policy_options() -> Vec<DropdownOption> {
    ["first-solution", "count-all", "count-unique"]
        .into_iter()
        .map(|policy| DropdownOption::new(policy, policy))
        .collect()
}

fn piece_or_none_options() -> Vec<DropdownOption> {
    ["none", "I", "O", "T", "S", "Z", "J", "L"]
        .into_iter()
        .map(|piece| DropdownOption::new(piece, piece))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_editor_schema_exposes_scenario_query_contract_and_result_fields() {
        let schema = ScenarioEditorSchema::m28();
        let fields = schema
            .fields()
            .iter()
            .map(ScenarioEditorFieldSchema::id)
            .collect::<Vec<_>>();

        for field in [
            "initial_board_mask",
            "remaining_queue",
            "max_pieces",
            "exact_pieces",
            "min_remaining_queue",
            "allow_hold",
            "requires_180",
            "rule_profile",
            "count_policy",
            "retained_trace_limit",
        ] {
            assert!(fields.contains(&field), "missing scenario field {field}");
        }

        let requires_180 = schema
            .fields()
            .iter()
            .find(|field| field.id() == "requires_180")
            .expect("requires_180 field");
        assert_eq!(
            requires_180
                .unsupported_reason()
                .expect("unsupported reason")
                .reason(),
            "scenario_requires_180_unsupported"
        );
        assert_eq!(
            schema.unsupported_reason_field(),
            "search_unsupported_reason"
        );
        assert!(schema
            .result_contract_fields()
            .iter()
            .any(|field| field == "coverage_probability"));
        assert!(schema
            .result_contract_fields()
            .iter()
            .any(|field| field == "packing_candidate_count"));
    }
}
