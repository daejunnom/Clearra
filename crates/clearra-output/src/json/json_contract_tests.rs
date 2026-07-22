use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_replay::{BuildVariantOperation, BuildVariantReplayInput, ReplayEngine};

use crate::model::{RenderField, RenderFieldValue};

use crate::json::{JsonContract, JsonMember, JsonValue};

pub(super) fn member_value<'a>(members: &'a [JsonMember], key: &str) -> &'a JsonValue {
    members
        .iter()
        .find_map(|member| (member.key() == key).then_some(member.value()))
        .expect("member exists")
}

pub(super) fn object_member<'a>(members: &'a [JsonMember], key: &str) -> &'a [JsonMember] {
    let JsonValue::Object(nested) = member_value(members, key) else {
        panic!("object member");
    };
    nested
}

pub(super) fn array_member<'a>(members: &'a [JsonMember], key: &str) -> &'a [JsonValue] {
    let JsonValue::Array(values) = member_value(members, key) else {
        panic!("array member");
    };
    values
}

#[path = "json_contract_behavior/diagnostic_replay_contract.rs"]
mod diagnostic_replay_contract;
#[path = "json_contract_behavior/pc_backend_contract.rs"]
mod pc_backend_contract;
#[path = "json_contract_behavior/pc_score_contract.rs"]
mod pc_score_contract;
#[path = "json_contract_behavior/pc_trace_contract.rs"]
mod pc_trace_contract;
#[path = "json_contract_behavior/render_contract.rs"]
mod render_contract;
#[path = "json_contract_behavior/resource_contract.rs"]
mod resource_contract;
