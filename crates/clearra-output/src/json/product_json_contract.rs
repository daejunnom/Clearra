use crate::json::{
    diagnostic_json_contract::diagnostics_contract,
    json_contract_helpers::{field_value, push_existing},
    json_value::{JsonField, JsonMember, JsonValue},
    pc_json_contract::pc_contract,
    setup_json_contract::{
        score_expectation_contract, setup_contract, special_spin_diagnostic_contract,
        spin_probability_contract, supply_contract,
    },
};

pub(crate) fn fields_object(fields: &[JsonField]) -> JsonValue {
    JsonValue::Object(
        fields
            .iter()
            .map(|field| JsonMember::new(field.key(), field.value().clone()))
            .collect(),
    )
}

pub(crate) fn contract_object(kind: &str, fields: &[JsonField]) -> JsonValue {
    match kind {
        "pc" | "pc-scenario" | "continue" | "path" => {
            contract_with_optional_diagnostics(kind, fields, [("pc", pc_contract(fields))])
        }
        "setup" => {
            contract_with_optional_diagnostics(kind, fields, [("setup", setup_contract(fields))])
        }
        "percent" => {
            contract_with_optional_diagnostics(kind, fields, [("supply", supply_contract(fields))])
        }
        "spin-probability" => contract_with_optional_diagnostics(
            kind,
            fields,
            [("spin_probability", spin_probability_contract(fields))],
        ),
        "score-expectation" => contract_with_optional_diagnostics(
            kind,
            fields,
            [("score", score_expectation_contract(fields))],
        ),
        "special-spin-diagnostic" => contract_with_optional_diagnostics(
            kind,
            fields,
            [("special_spin", special_spin_diagnostic_contract(fields))],
        ),
        "diagnostic" => JsonValue::object([
            ("command", command_contract(kind, fields)),
            ("diagnostics", diagnostics_contract(fields)),
        ]),
        _ => contract_with_optional_diagnostics(kind, fields, []),
    }
}

fn contract_with_optional_diagnostics<'a, I>(
    kind: &str,
    fields: &[JsonField],
    sections: I,
) -> JsonValue
where
    I: IntoIterator<Item = (&'a str, JsonValue)>,
{
    let mut members = vec![("command".to_owned(), command_contract(kind, fields))];
    members.extend(
        sections
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value)),
    );
    if has_diagnostic_payload(fields) {
        members.push(("diagnostics".to_owned(), diagnostics_contract(fields)));
    }
    if matches!(
        field_value(fields, "solution_data_requested"),
        Some(JsonValue::Bool(true))
    ) {
        members.push(("artifacts".to_owned(), solution_artifacts_contract(fields)));
    }
    JsonValue::object(members)
}

fn solution_artifacts_contract(fields: &[JsonField]) -> JsonValue {
    let mut members = vec![(
        "schema_version".to_owned(),
        JsonValue::string("clearra.solution-data.v1"),
    )];
    push_existing(fields, &mut members, "solution_keys", "solution_keys");
    push_existing(
        fields,
        &mut members,
        "solution_probabilities",
        "solution_probabilities",
    );
    push_existing(fields, &mut members, "hold_conditions", "setup_conditions");
    push_existing(fields, &mut members, "forward_solution_data", "forward");
    JsonValue::object(members)
}

fn has_diagnostic_payload(fields: &[JsonField]) -> bool {
    [
        "diagnostics",
        "diagnostic_code",
        "diagnostic_severity",
        "diagnostic_message",
        "diagnostic_location",
        "diagnostic_evidence",
        "suggested_next_step",
    ]
    .iter()
    .any(|key| field_value(fields, key).is_some())
}

fn command_contract(kind: &str, fields: &[JsonField]) -> JsonValue {
    let mut members = vec![("kind".to_owned(), JsonValue::string(kind))];
    push_existing(fields, &mut members, "status", "status");
    push_existing(fields, &mut members, "action", "action");
    push_existing(fields, &mut members, "input_mode", "input_mode");
    push_existing(
        fields,
        &mut members,
        "interactive_prompt",
        "interactive_prompt",
    );
    JsonValue::object(members)
}
