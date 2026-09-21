use std::collections::HashSet;

use ed25519_dalek::Signature;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    error::{ActivationError, Result},
    trusted_keyring::{decode_hex_array, exact_keys},
    StaticPublicKeyring, GENERATION_SIGNATURE_DOMAIN, GENERATION_STATEMENT_SCHEMA,
    HOST_GENERATION_SCHEMA, MAX_ROLLBACK_GENERATIONS, PRODUCTION_CHANNEL, ROLLOUT_SIGNATURE_DOMAIN,
    ROLLOUT_STATEMENT_SCHEMA, RULE_PROFILES, SIGNATURE_ALGORITHM,
    SIGNED_GENERATION_ENVELOPE_SCHEMA, SIGNED_ROLLOUT_ENVELOPE_SCHEMA,
};

const MAX_ENVELOPE_BYTES: usize = 196_608;
const MAX_GENERATION_BYTES: usize = 65_536;

#[derive(Clone, Debug)]
pub struct VerifiedGenerationAuthority {
    authority_identity: String,
    generation_identity: String,
    generation_json: String,
    generation: Value,
    channel: String,
    compatibility_identity: String,
}

impl VerifiedGenerationAuthority {
    pub fn authority_identity(&self) -> &str {
        &self.authority_identity
    }

    pub fn generation_identity(&self) -> &str {
        &self.generation_identity
    }

    pub fn generation_json(&self) -> &str {
        &self.generation_json
    }

    pub fn channel(&self) -> &str {
        &self.channel
    }

    pub fn compatibility_identity(&self) -> &str {
        &self.compatibility_identity
    }

    pub(crate) fn generation(&self) -> &Value {
        &self.generation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedGeneration {
    generation_identity: String,
    authority_identity: String,
}

impl RetainedGeneration {
    pub fn generation_identity(&self) -> &str {
        &self.generation_identity
    }

    pub fn authority_identity(&self) -> &str {
        &self.authority_identity
    }
}

#[derive(Clone, Debug)]
pub struct VerifiedRolloutPointer {
    pointer_identity: String,
    sequence: u64,
    selected_generation_identity: String,
    selected_authority_identity: String,
    previous_pointer_identity: Option<String>,
    retained_generations: Vec<RetainedGeneration>,
    issued_at_unix_seconds: u64,
    expires_at_unix_seconds: u64,
    channel: String,
}

impl VerifiedRolloutPointer {
    pub fn pointer_identity(&self) -> &str {
        &self.pointer_identity
    }

    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn selected_generation_identity(&self) -> &str {
        &self.selected_generation_identity
    }

    pub fn selected_authority_identity(&self) -> &str {
        &self.selected_authority_identity
    }

    pub fn previous_pointer_identity(&self) -> Option<&str> {
        self.previous_pointer_identity.as_deref()
    }

    pub fn retained_generations(&self) -> &[RetainedGeneration] {
        &self.retained_generations
    }

    pub const fn issued_at_unix_seconds(&self) -> u64 {
        self.issued_at_unix_seconds
    }

    pub const fn expires_at_unix_seconds(&self) -> u64 {
        self.expires_at_unix_seconds
    }

    pub fn channel(&self) -> &str {
        &self.channel
    }
}

struct VerifiedStatement {
    value: Value,
    identity: String,
}

pub fn verify_generation_envelope(
    envelope_json: &str,
    keyring: &StaticPublicKeyring,
) -> Result<VerifiedGenerationAuthority> {
    let verified = verify_statement(
        envelope_json,
        SIGNED_GENERATION_ENVELOPE_SCHEMA,
        GENERATION_STATEMENT_SCHEMA,
        GENERATION_SIGNATURE_DOMAIN,
        keyring,
    )?;
    exact_keys(
        &verified.value,
        &[
            "algorithm",
            "channel",
            "compatibility_identity",
            "generation_id",
            "generation_identity",
            "generation_json",
            "key_id",
            "repository",
            "revision",
            "schema",
        ],
        "pc4_activation_generation_statement_shape",
    )?;
    let generation_json = string(&verified.value, "generation_json")?;
    let generation_identity = sha256_identity(generation_json.as_bytes());
    if string(&verified.value, "generation_identity")? != generation_identity {
        return Err(ActivationError::new("pc4_activation_generation_identity"));
    }
    let generation = parse_canonical_text(
        generation_json,
        MAX_GENERATION_BYTES,
        "pc4_activation_generation_text",
        "pc4_activation_generation_json",
    )?;
    validate_authority_generation(&generation)?;
    if string(&generation, "repository")? != string(&verified.value, "repository")?
        || string(&generation, "revision")? != string(&verified.value, "revision")?
        || string(&generation["admission"], "generation_id")?
            != string(&verified.value, "generation_id")?
    {
        return Err(ActivationError::new("pc4_activation_generation_binding"));
    }
    let channel = string(&verified.value, "channel")?;
    if channel != PRODUCTION_CHANNEL {
        return Err(ActivationError::new("pc4_activation_channel"));
    }
    let compatibility_identity = string(&verified.value, "compatibility_identity")?;
    require_identity(
        compatibility_identity,
        "pc4_activation_compatibility_identity",
    )?;
    Ok(VerifiedGenerationAuthority {
        authority_identity: verified.identity,
        generation_identity,
        generation_json: generation_json.to_owned(),
        generation,
        channel: channel.to_owned(),
        compatibility_identity: compatibility_identity.to_owned(),
    })
}

pub fn verify_rollout_pointer(
    envelope_json: &str,
    keyring: &StaticPublicKeyring,
) -> Result<VerifiedRolloutPointer> {
    let verified = verify_statement(
        envelope_json,
        SIGNED_ROLLOUT_ENVELOPE_SCHEMA,
        ROLLOUT_STATEMENT_SCHEMA,
        ROLLOUT_SIGNATURE_DOMAIN,
        keyring,
    )?;
    exact_keys(
        &verified.value,
        &[
            "algorithm",
            "channel",
            "expires_at_unix_seconds",
            "issued_at_unix_seconds",
            "key_id",
            "previous_pointer_identity",
            "retained_generations",
            "rollback_limit",
            "rollout_sequence",
            "schema",
            "selected_authority_identity",
            "selected_generation_identity",
        ],
        "pc4_activation_rollout_statement_shape",
    )?;
    let channel = string(&verified.value, "channel")?;
    if channel != PRODUCTION_CHANNEL {
        return Err(ActivationError::new("pc4_activation_channel"));
    }
    let sequence = canonical_u64(
        string(&verified.value, "rollout_sequence")?,
        "pc4_activation_rollout_sequence",
    )?;
    if sequence == 0 {
        return Err(ActivationError::new("pc4_activation_rollout_sequence"));
    }
    let rollback_limit = canonical_u64(
        string(&verified.value, "rollback_limit")?,
        "pc4_activation_rollback_limit",
    )? as usize;
    if !(1..=MAX_ROLLBACK_GENERATIONS).contains(&rollback_limit) {
        return Err(ActivationError::new("pc4_activation_rollback_limit"));
    }
    let selected_generation_identity = string(&verified.value, "selected_generation_identity")?;
    let selected_authority_identity = string(&verified.value, "selected_authority_identity")?;
    require_identity(
        selected_generation_identity,
        "pc4_activation_selected_generation_identity",
    )?;
    require_identity(
        selected_authority_identity,
        "pc4_activation_selected_authority_identity",
    )?;
    let previous_pointer_identity = match &verified.value["previous_pointer_identity"] {
        Value::Null if sequence == 1 => None,
        Value::String(value) if sequence > 1 => {
            require_identity(value, "pc4_activation_previous_pointer_identity")?;
            Some(value.clone())
        }
        _ => {
            return Err(ActivationError::new(
                "pc4_activation_previous_pointer_identity",
            ))
        }
    };
    let retained = verified.value["retained_generations"]
        .as_array()
        .ok_or(ActivationError::new("pc4_activation_retained_generations"))?;
    if retained.is_empty() || retained.len() > rollback_limit + 1 {
        return Err(ActivationError::new("pc4_activation_retained_generations"));
    }
    let mut seen = HashSet::new();
    let mut retained_generations = Vec::with_capacity(retained.len());
    for value in retained {
        exact_keys(
            value,
            &["authority_identity", "generation_identity"],
            "pc4_activation_retained_generation_shape",
        )?;
        let generation_identity = string(value, "generation_identity")?;
        let authority_identity = string(value, "authority_identity")?;
        require_identity(
            generation_identity,
            "pc4_activation_retained_generation_identity",
        )?;
        require_identity(
            authority_identity,
            "pc4_activation_retained_authority_identity",
        )?;
        if !seen.insert((
            generation_identity.to_owned(),
            authority_identity.to_owned(),
        )) {
            return Err(ActivationError::new(
                "pc4_activation_retained_generation_duplicate",
            ));
        }
        retained_generations.push(RetainedGeneration {
            generation_identity: generation_identity.to_owned(),
            authority_identity: authority_identity.to_owned(),
        });
    }
    let selected = retained_generations
        .first()
        .ok_or(ActivationError::new("pc4_activation_retained_generations"))?;
    if selected.generation_identity != selected_generation_identity
        || selected.authority_identity != selected_authority_identity
    {
        return Err(ActivationError::new("pc4_activation_selected_not_first"));
    }
    let issued_at_unix_seconds = canonical_u64(
        string(&verified.value, "issued_at_unix_seconds")?,
        "pc4_activation_issued_at",
    )?;
    let expires_at_unix_seconds = canonical_u64(
        string(&verified.value, "expires_at_unix_seconds")?,
        "pc4_activation_expires_at",
    )?;
    if issued_at_unix_seconds >= expires_at_unix_seconds {
        return Err(ActivationError::new("pc4_activation_validity_window"));
    }
    Ok(VerifiedRolloutPointer {
        pointer_identity: verified.identity,
        sequence,
        selected_generation_identity: selected_generation_identity.to_owned(),
        selected_authority_identity: selected_authority_identity.to_owned(),
        previous_pointer_identity,
        retained_generations,
        issued_at_unix_seconds,
        expires_at_unix_seconds,
        channel: channel.to_owned(),
    })
}

fn verify_statement(
    envelope_json: &str,
    envelope_schema: &str,
    statement_schema: &str,
    domain: &[u8],
    keyring: &StaticPublicKeyring,
) -> Result<VerifiedStatement> {
    let envelope = parse_canonical_text(
        envelope_json,
        MAX_ENVELOPE_BYTES,
        "pc4_activation_envelope_text",
        "pc4_activation_envelope_json",
    )?;
    exact_keys(
        &envelope,
        &["schema", "signature_hex", "statement"],
        "pc4_activation_envelope_shape",
    )?;
    if envelope["schema"].as_str() != Some(envelope_schema) {
        return Err(ActivationError::new("pc4_activation_envelope_schema"));
    }
    let statement = string(&envelope, "statement")?;
    let value = parse_canonical_text(
        statement,
        MAX_ENVELOPE_BYTES,
        "pc4_activation_statement_text",
        "pc4_activation_statement_json",
    )?;
    if value["schema"].as_str() != Some(statement_schema)
        || value["algorithm"].as_str() != Some(SIGNATURE_ALGORITHM)
    {
        return Err(ActivationError::new("pc4_activation_statement_schema"));
    }
    let key_id = string(&value, "key_id")?;
    let verifying_key = keyring.verifying_key(key_id)?;
    let signature_bytes = decode_hex_array::<64>(
        string(&envelope, "signature_hex")?,
        "pc4_activation_signature_encoding",
    )?;
    let signature = Signature::from_bytes(&signature_bytes);
    let mut preimage = Vec::with_capacity(domain.len() + statement.len());
    preimage.extend_from_slice(domain);
    preimage.extend_from_slice(statement.as_bytes());
    verifying_key
        .verify_strict(&preimage, &signature)
        .map_err(|_| ActivationError::new("pc4_activation_signature"))?;
    Ok(VerifiedStatement {
        value,
        identity: sha256_identity(statement.as_bytes()),
    })
}

fn validate_authority_generation(value: &Value) -> Result<()> {
    exact_keys(
        value,
        &[
            "admission",
            "profiles",
            "repository",
            "revision",
            "schema",
            "transferred_bytes",
        ],
        "pc4_activation_generation_shape",
    )?;
    if value["schema"].as_str() != Some(HOST_GENERATION_SCHEMA)
        || value["transferred_bytes"].as_u64() != Some(0)
    {
        return Err(ActivationError::new("pc4_activation_generation_contract"));
    }
    exact_keys(
        &value["admission"],
        &["generation_id", "manifest_content_identity"],
        "pc4_activation_admission_shape",
    )?;
    require_identity(
        string(&value["admission"], "manifest_content_identity")?,
        "pc4_activation_admission_identity",
    )?;
    if string(&value["admission"], "generation_id")?.is_empty() {
        return Err(ActivationError::new("pc4_activation_generation_id"));
    }
    let profiles = value["profiles"]
        .as_array()
        .ok_or(ActivationError::new("pc4_activation_profile_slots"))?;
    if profiles.len() != RULE_PROFILES.len() {
        return Err(ActivationError::new("pc4_activation_profile_slots"));
    }
    let mut ready = 0_usize;
    for (index, expected) in RULE_PROFILES.iter().enumerate() {
        let slot = &profiles[index];
        if slot["profile"].as_str() != Some(expected) {
            return Err(ActivationError::new("pc4_activation_profile_order"));
        }
        match slot["status"].as_str() {
            Some("unavailable") => validate_unavailable_slot(slot)?,
            Some("ready") => {
                validate_ready_authority_slot(slot, expected)?;
                ready += 1;
            }
            _ => return Err(ActivationError::new("pc4_activation_profile_status")),
        }
    }
    if ready == 0 {
        return Err(ActivationError::new(
            "pc4_activation_zero_qualified_profiles",
        ));
    }
    Ok(())
}

fn validate_unavailable_slot(slot: &Value) -> Result<()> {
    exact_keys(
        slot,
        &["profile", "reason", "status", "upstream_complete"],
        "pc4_activation_unavailable_profile_shape",
    )?;
    if slot["upstream_complete"].as_bool() != Some(false) || string(slot, "reason")?.is_empty() {
        return Err(ActivationError::new("pc4_activation_unavailable_profile"));
    }
    Ok(())
}

fn validate_ready_authority_slot(slot: &Value, profile: &str) -> Result<()> {
    exact_keys(
        slot,
        &[
            "admission",
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
        "pc4_activation_ready_profile_shape",
    )?;
    if slot["upstream_complete"].as_bool() != Some(true)
        || string(slot, "reader_contract")?.is_empty()
        || slot["field_count"].as_u64().is_none()
        || slot["terminal_id"].as_u64().is_none()
        || slot["target_width"].as_u64().is_none()
        || slot["target_lines"] != serde_json::json!([4])
    {
        return Err(ActivationError::new("pc4_activation_ready_profile"));
    }
    let pc_lines = target_lines(&slot["pc_search_target_lines"])?;
    let setup_lines = target_lines(&slot["setup_search_target_lines"])?;
    if pc_lines != HashSet::from([4]) {
        return Err(ActivationError::new("pc4_activation_pc_target_missing"));
    }
    let receipts = slot["target_qualification_receipts"]
        .as_array()
        .ok_or(ActivationError::new("pc4_activation_target_receipts"))?;
    let mut covered_pc = HashSet::new();
    let mut covered_setup = HashSet::new();
    for receipt in receipts {
        let receipt_profile = string(receipt, "profile")?;
        let use_case = string(receipt, "use_case")?;
        let lines = receipt["target_lines"]
            .as_u64()
            .ok_or(ActivationError::new("pc4_activation_target_receipt"))?;
        if receipt_profile != profile || lines != 4 {
            return Err(ActivationError::new(
                "pc4_activation_target_receipt_binding",
            ));
        }
        let schema = string(receipt, "schema")?;
        match use_case {
            "pc-search" if schema == "clearra.pc4.exact-target-qualification.v1" => {
                if !covered_pc.insert(lines) {
                    return Err(ActivationError::new(
                        "pc4_activation_target_receipt_duplicate",
                    ));
                }
            }
            "setup-search" if schema == "clearra.pc4.exact-setup-target-qualification.v1" => {
                if !covered_setup.insert(lines) {
                    return Err(ActivationError::new(
                        "pc4_activation_target_receipt_duplicate",
                    ));
                }
            }
            _ => return Err(ActivationError::new("pc4_activation_target_receipt_schema")),
        }
    }
    if pc_lines != covered_pc || setup_lines != covered_setup {
        return Err(ActivationError::new(
            "pc4_activation_target_receipt_coverage",
        ));
    }
    Ok(())
}

fn target_lines(value: &Value) -> Result<HashSet<u64>> {
    let values = value
        .as_array()
        .ok_or(ActivationError::new("pc4_activation_target_lines"))?;
    let mut lines = HashSet::new();
    for value in values {
        let line = value
            .as_u64()
            .ok_or(ActivationError::new("pc4_activation_target_lines"))?;
        if line != 4 || !lines.insert(line) {
            return Err(ActivationError::new("pc4_activation_target_lines"));
        }
    }
    Ok(lines)
}

pub(crate) fn validate_text(value: &str, max: usize, code: &'static str) -> Result<()> {
    let bytes = value.as_bytes();
    if bytes.is_empty()
        || bytes.len() > max
        || bytes.starts_with(&[0xef, 0xbb, 0xbf])
        || bytes.contains(&b'\r')
        || !bytes.ends_with(b"\n")
        || bytes.get(bytes.len().saturating_sub(2)) == Some(&b'\n')
    {
        return Err(ActivationError::new(code));
    }
    Ok(())
}

pub(crate) fn parse_canonical_text(
    text: &str,
    max: usize,
    text_code: &'static str,
    json_code: &'static str,
) -> Result<Value> {
    validate_text(text, max, text_code)?;
    let value: Value = serde_json::from_str(text).map_err(|_| ActivationError::new(json_code))?;
    if format!("{}\n", canonical_json(&value)?) != text {
        return Err(ActivationError::new(text_code));
    }
    Ok(value)
}

fn canonical_json(value: &Value) -> Result<String> {
    match value {
        Value::Null => Ok("null".to_owned()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) if value.as_i64().is_some() || value.as_u64().is_some() => {
            Ok(value.to_string())
        }
        Value::Number(_) => Err(ActivationError::new("pc4_activation_canonical_number")),
        Value::String(value) => serde_json::to_string(value)
            .map_err(|_| ActivationError::new("pc4_activation_canonical_string")),
        Value::Array(values) => {
            let entries = values
                .iter()
                .map(canonical_json)
                .collect::<Result<Vec<_>>>()?;
            Ok(format!("[{}]", entries.join(",")))
        }
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            let fields = keys
                .into_iter()
                .map(|key| {
                    let encoded = serde_json::to_string(key)
                        .map_err(|_| ActivationError::new("pc4_activation_canonical_string"))?;
                    Ok(format!("{encoded}:{}", canonical_json(&object[key])?))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(format!("{{{}}}", fields.join(",")))
        }
    }
}

pub(crate) fn sha256_identity(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

pub(crate) fn require_identity(value: &str, code: &'static str) -> Result<()> {
    let digest = value
        .strip_prefix("sha256:")
        .ok_or(ActivationError::new(code))?;
    if digest.len() != 64
        || digest
            .bytes()
            .any(|byte| !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase())
        || digest.bytes().all(|byte| byte == b'0')
    {
        return Err(ActivationError::new(code));
    }
    Ok(())
}

fn canonical_u64(value: &str, code: &'static str) -> Result<u64> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || value.bytes().any(|byte| !byte.is_ascii_digit())
    {
        return Err(ActivationError::new(code));
    }
    let parsed = value.parse().map_err(|_| ActivationError::new(code))?;
    if parsed > 9_007_199_254_740_991 {
        return Err(ActivationError::new(code));
    }
    Ok(parsed)
}

pub(crate) fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or(ActivationError::new("pc4_activation_missing_string"))
}
