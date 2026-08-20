use clearra_host_contract::ProductBuildIdentity;
use clearra_replay::ReplayTrace;

pub use crate::json::{
    json_schema_version::JSON_SCHEMA_VERSION,
    json_value::{JsonField, JsonMember, JsonValue},
};

use crate::{
    json::{
        product_json_contract::{contract_object, fields_object},
        replay_json_contract::replay_trace_object,
        resource_report::resource_report_object,
    },
    model::render_field_value::RenderField,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JsonContract {
    fields: Vec<JsonField>,
    root: Option<JsonValue>,
}

impl JsonContract {
    pub fn new(fields: Vec<JsonField>) -> Self {
        Self { fields, root: None }
    }
}
impl JsonContract {
    pub fn from_render_message(kind: &str, fields: &[RenderField]) -> Self {
        Self::from_render_message_with_runtime_identity(kind, fields, None)
    }

    pub fn from_render_message_with_runtime_identity(
        kind: &str,
        fields: &[RenderField],
        runtime_identity: Option<&ProductBuildIdentity>,
    ) -> Self {
        let fields = fields
            .iter()
            .map(|field| JsonField::typed(field.key(), field.value().to_json_value()))
            .collect::<Vec<_>>();
        let solution_data_requested = fields.iter().any(|field| {
            field.key() == "solution_data_requested"
                && matches!(field.value(), JsonValue::Bool(true))
        });
        let summary = fields
            .iter()
            .filter(|field| {
                !matches!(
                    field.key(),
                    "solution_data_requested" | "solution_data_status" | "solution_data_reason"
                ) && (!solution_data_requested
                    || !matches!(
                        field.key(),
                        "solution_keys"
                            | "solution_classes"
                            | "solution_probabilities"
                            | "finesse_report"
                            | "finesse_score_data"
                            | "hold_conditions"
                            | "outcomes"
                            | "regular"
                            | "mini"
                            | "forward_solution_data"
                    ))
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut root_members = vec![
            (
                "schema_version".to_owned(),
                JsonValue::number(JSON_SCHEMA_VERSION.to_string()),
            ),
            ("kind".to_owned(), JsonValue::string(kind)),
            ("summary".to_owned(), fields_object(&summary)),
            ("contract".to_owned(), contract_object(kind, &fields)),
        ];
        if let Some(identity) = runtime_identity {
            root_members.push((
                "runtime_identity".to_owned(),
                product_build_identity_object(identity),
            ));
        }
        if let Some(report) = resource_report_object(&fields) {
            root_members.push(("resource_report".to_owned(), report));
        }
        if let Some(report) = fields
            .iter()
            .find(|field| field.key() == "finesse_report")
            .map(|field| field.value().clone())
        {
            root_members.push(("finesse_report".to_owned(), report));
        }
        let root = JsonValue::object(root_members);

        Self {
            fields,
            root: Some(root),
        }
    }
}

fn product_build_identity_object(identity: &ProductBuildIdentity) -> JsonValue {
    JsonValue::object([
        (
            "engine_build_id",
            JsonValue::string(identity.engine_build_id()),
        ),
        ("source_commit", JsonValue::string(identity.source_commit())),
        (
            "contract_schema_version",
            JsonValue::string(identity.contract_schema_version()),
        ),
        (
            "supply_semantics_id",
            JsonValue::string(identity.supply_semantics_id()),
        ),
        (
            "artifact_schema_version",
            JsonValue::string(identity.artifact_schema_version()),
        ),
    ])
}
impl JsonContract {
    pub fn from_replay_trace(trace: &ReplayTrace) -> Self {
        let summary = vec![
            JsonField::typed("variant_id", JsonValue::string(trace.variant_id())),
            JsonField::typed("representative", JsonValue::Bool(trace.representative())),
            JsonField::typed("sample", JsonValue::Bool(trace.sample())),
            JsonField::typed(
                "trace_steps",
                JsonValue::number(trace.trace_steps().to_string()),
            ),
            JsonField::typed("canonical_key", JsonValue::string(trace.canonical_key())),
        ];
        let root = JsonValue::object([
            (
                "schema_version",
                JsonValue::number(JSON_SCHEMA_VERSION.to_string()),
            ),
            ("kind", JsonValue::string("replay-trace")),
            ("summary", fields_object(&summary)),
            ("replay_trace", replay_trace_object(trace)),
        ]);

        Self {
            fields: summary,
            root: Some(root),
        }
    }
}
impl JsonContract {
    pub fn fields(&self) -> &[JsonField] {
        &self.fields
    }
}
impl JsonContract {
    pub fn root(&self) -> JsonValue {
        self.root
            .clone()
            .unwrap_or_else(|| fields_object(&self.fields))
    }
}
