use serde_json::{json, Value};

use crate::{
    error::{ActivationError, Result},
    replay_state::{ReplayDecision, ReplayState},
    signed_envelope::{parse_canonical_text, string},
    trusted_keyring::exact_keys,
    verify_generation_envelope, verify_rollout_pointer, StaticPublicKeyring,
    HOST_GENERATION_SCHEMA, RULE_PROFILES,
};

const MAX_GENERATION_BYTES: usize = 65_536;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileActivation {
    Ready {
        pc_search_target_lines: Vec<u8>,
        setup_search_target_lines: Vec<u8>,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveProfile {
    profile: String,
    activation: ProfileActivation,
}

impl EffectiveProfile {
    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub fn activation(&self) -> &ProfileActivation {
        &self.activation
    }
}

#[derive(Clone, Debug)]
pub struct LinkedActivation {
    host_generation_json: String,
    generation_identity: String,
    authority_identity: String,
    pointer_identity: String,
    profiles: Vec<EffectiveProfile>,
    replay_decision: ReplayDecision,
    next_replay_state: ReplayState,
}

impl LinkedActivation {
    pub fn host_generation_json(&self) -> &str {
        &self.host_generation_json
    }

    pub fn generation_identity(&self) -> &str {
        &self.generation_identity
    }

    pub fn authority_identity(&self) -> &str {
        &self.authority_identity
    }

    pub fn pointer_identity(&self) -> &str {
        &self.pointer_identity
    }

    pub fn profiles(&self) -> &[EffectiveProfile] {
        &self.profiles
    }

    pub const fn replay_decision(&self) -> ReplayDecision {
        self.replay_decision
    }

    pub fn next_replay_state(&self) -> &ReplayState {
        &self.next_replay_state
    }
}

#[allow(clippy::too_many_arguments)]
pub fn verify_and_link(
    reader_generation_json: &str,
    signed_generation_envelope_json: &str,
    signed_rollout_envelope_json: &str,
    keyring: &StaticPublicKeyring,
    replay_state: &ReplayState,
    now_unix_seconds: u64,
    bootstrap_min_sequence: u64,
    expected_channel: &str,
    expected_compatibility_identity: &str,
) -> Result<LinkedActivation> {
    let authority = verify_generation_envelope(signed_generation_envelope_json, keyring)?;
    let pointer = verify_rollout_pointer(signed_rollout_envelope_json, keyring)?;
    if authority.channel() != expected_channel
        || pointer.channel() != expected_channel
        || authority.compatibility_identity() != expected_compatibility_identity
    {
        return Err(ActivationError::new("pc4_activation_product_binding"));
    }
    if pointer.selected_generation_identity() != authority.generation_identity()
        || pointer.selected_authority_identity() != authority.authority_identity()
    {
        return Err(ActivationError::new(
            "pc4_activation_rollout_generation_binding",
        ));
    }
    let (replay_decision, next_replay_state) =
        replay_state.preview(&pointer, now_unix_seconds, bootstrap_min_sequence)?;
    let reader = validate_reader_generation(reader_generation_json)?;
    let authority_value = authority.generation();
    if reader["repository"] != authority_value["repository"]
        || reader["revision"] != authority_value["revision"]
    {
        return Err(ActivationError::new(
            "pc4_activation_reader_generation_binding",
        ));
    }
    let reader_profiles = reader["profiles"]
        .as_array()
        .ok_or(ActivationError::new("pc4_activation_reader_profile_slots"))?;
    let authority_profiles = authority_value["profiles"]
        .as_array()
        .ok_or(ActivationError::new("pc4_activation_profile_slots"))?;
    let mut effective_values = Vec::with_capacity(RULE_PROFILES.len());
    let mut profiles = Vec::with_capacity(RULE_PROFILES.len());
    let mut ready_count = 0_usize;
    for (index, profile) in RULE_PROFILES.iter().enumerate() {
        let reader_slot = &reader_profiles[index];
        let authority_slot = &authority_profiles[index];
        if authority_slot["status"] == "unavailable" {
            let reason = string(authority_slot, "reason")?.to_owned();
            effective_values.push(authority_slot.clone());
            profiles.push(EffectiveProfile {
                profile: (*profile).to_owned(),
                activation: ProfileActivation::Unavailable { reason },
            });
            continue;
        }
        if reader_slot["status"] == "unavailable" {
            let reason = format!("reader-unavailable:{}", string(reader_slot, "reason")?);
            effective_values.push(json!({
                "profile": profile,
                "upstream_complete": false,
                "status": "unavailable",
                "reason": reason,
            }));
            profiles.push(EffectiveProfile {
                profile: (*profile).to_owned(),
                activation: ProfileActivation::Unavailable { reason },
            });
            continue;
        }
        require_reader_binding(reader_slot, authority_slot)?;
        let pc_search_target_lines = u8_lines(&authority_slot["pc_search_target_lines"])?;
        let setup_search_target_lines = u8_lines(&authority_slot["setup_search_target_lines"])?;
        effective_values.push(authority_slot.clone());
        profiles.push(EffectiveProfile {
            profile: (*profile).to_owned(),
            activation: ProfileActivation::Ready {
                pc_search_target_lines,
                setup_search_target_lines,
            },
        });
        ready_count += 1;
    }
    if ready_count == 0 {
        return Err(ActivationError::new("pc4_activation_no_effective_profile"));
    }
    let mut effective = authority_value.clone();
    effective["profiles"] = Value::Array(effective_values);
    let host_generation_json = format!(
        "{}\n",
        serde_json::to_string(&effective)
            .map_err(|_| ActivationError::new("pc4_activation_effective_generation"))?
    );
    Ok(LinkedActivation {
        host_generation_json,
        generation_identity: authority.generation_identity().to_owned(),
        authority_identity: authority.authority_identity().to_owned(),
        pointer_identity: pointer.pointer_identity().to_owned(),
        profiles,
        replay_decision,
        next_replay_state,
    })
}

fn validate_reader_generation(json: &str) -> Result<Value> {
    let value = parse_canonical_text(
        json,
        MAX_GENERATION_BYTES,
        "pc4_activation_reader_text",
        "pc4_activation_reader_json",
    )?;
    exact_keys(
        &value,
        &[
            "profiles",
            "repository",
            "revision",
            "schema",
            "transferred_bytes",
        ],
        "pc4_activation_reader_shape",
    )?;
    if value["schema"].as_str() != Some(HOST_GENERATION_SCHEMA)
        || value["transferred_bytes"].as_u64().is_none()
    {
        return Err(ActivationError::new("pc4_activation_reader_contract"));
    }
    let profiles = value["profiles"]
        .as_array()
        .ok_or(ActivationError::new("pc4_activation_reader_profile_slots"))?;
    if profiles.len() != RULE_PROFILES.len() {
        return Err(ActivationError::new("pc4_activation_reader_profile_slots"));
    }
    for (index, profile) in RULE_PROFILES.iter().enumerate() {
        let slot = &profiles[index];
        if slot["profile"].as_str() != Some(profile) {
            return Err(ActivationError::new("pc4_activation_reader_profile_order"));
        }
        match slot["status"].as_str() {
            Some("unavailable") => {
                exact_keys(
                    slot,
                    &["profile", "reason", "status", "upstream_complete"],
                    "pc4_activation_reader_unavailable_shape",
                )?;
                if slot["upstream_complete"].as_bool() != Some(false)
                    || string(slot, "reason")?.is_empty()
                {
                    return Err(ActivationError::new("pc4_activation_reader_unavailable"));
                }
            }
            Some("ready") => validate_ready_reader_slot(slot)?,
            _ => return Err(ActivationError::new("pc4_activation_reader_profile_status")),
        }
    }
    Ok(value)
}

fn validate_ready_reader_slot(slot: &Value) -> Result<()> {
    exact_keys(
        slot,
        &[
            "artifacts",
            "evidence",
            "field_count",
            "pc_search_target_lines",
            "profile",
            "reader_contract",
            "setup_search_target_lines",
            "status",
            "target_lines",
            "target_qualification_receipts",
            "target_width",
            "terminal_id",
            "upstream_complete",
        ],
        "pc4_activation_reader_ready_shape",
    )?;
    if slot["upstream_complete"].as_bool() != Some(true)
        || slot["pc_search_target_lines"] != json!([])
        || slot["setup_search_target_lines"] != json!([])
        || slot["target_qualification_receipts"] != json!([])
    {
        return Err(ActivationError::new(
            "pc4_activation_reader_minted_authority",
        ));
    }
    Ok(())
}

fn require_reader_binding(reader: &Value, authority: &Value) -> Result<()> {
    for field in [
        "profile",
        "upstream_complete",
        "status",
        "reader_contract",
        "field_count",
        "target_width",
        "target_lines",
        "terminal_id",
        "artifacts",
        "evidence",
    ] {
        if reader[field] != authority[field] {
            return Err(ActivationError::new(
                "pc4_activation_reader_profile_binding",
            ));
        }
    }
    Ok(())
}
fn u8_lines(value: &Value) -> Result<Vec<u8>> {
    value
        .as_array()
        .ok_or(ActivationError::new("pc4_activation_target_lines"))?
        .iter()
        .map(|line| {
            u8::try_from(
                line.as_u64()
                    .ok_or(ActivationError::new("pc4_activation_target_lines"))?,
            )
            .map_err(|_| ActivationError::new("pc4_activation_target_lines"))
        })
        .collect()
}
