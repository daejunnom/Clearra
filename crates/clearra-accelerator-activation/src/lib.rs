//! Host-owned release authority for solver accelerator data products.
//!
//! Structural parsing belongs to each data product.  This crate only proves
//! that a canonical statement was signed by a pinned release key and exposes
//! an opaque authority object.  Callers cannot construct that object from a
//! boolean or an unsigned catalog.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const SIGNED_ASSET_ENVELOPE_SCHEMA: &str = "clearra.accelerator.signed-asset-envelope.v1";
pub const ASSET_STATEMENT_SCHEMA: &str = "clearra.accelerator.asset-statement.v1";
pub const SIGNATURE_ALGORITHM: &str = "ed25519";
const SIGNATURE_DOMAIN: &[u8] = b"clearra.accelerator.asset-statement.v1\0";
const MAX_ENVELOPE_BYTES: usize = 131_072;
const MAX_STATEMENT_BYTES: usize = 65_536;
const PROFILES: [&str; 5] = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcceleratorProduct {
    ExactLegalBoard,
    BoardConditionedReachability,
}

impl AcceleratorProduct {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExactLegalBoard => "exact-legal-board",
            Self::BoardConditionedReachability => "board-conditioned-reachability",
        }
    }

    fn parse(value: &str) -> Result<Self, ActivationError> {
        match value {
            "exact-legal-board" => Ok(Self::ExactLegalBoard),
            "board-conditioned-reachability" => Ok(Self::BoardConditionedReachability),
            _ => Err(ActivationError::new("accelerator_product_unsupported")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinnedPublicKey {
    pub key_id: &'static str,
    pub public_key: [u8; 32],
}

#[derive(Clone, Copy, Debug)]
pub struct StaticPublicKeyring<'a> {
    keys: &'a [PinnedPublicKey],
}

impl<'a> StaticPublicKeyring<'a> {
    pub const fn new(keys: &'a [PinnedPublicKey]) -> Self {
        Self { keys }
    }

    fn find(self, key_id: &str) -> Option<[u8; 32]> {
        self.keys
            .iter()
            .find(|candidate| candidate.key_id == key_id)
            .map(|candidate| candidate.public_key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivationError {
    code: &'static str,
}

impl ActivationError {
    const fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub const fn code(self) -> &'static str {
        self.code
    }
}

impl core::fmt::Display for ActivationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for ActivationError {}

/// Opaque proof that a release key signed the exact canonical statement.
#[derive(Clone, Debug)]
pub struct VerifiedAcceleratorAuthority {
    statement_identity: [u8; 32],
    product: AcceleratorProduct,
    profile: String,
    generation_identity: [u8; 32],
    rule_identity: [u8; 32],
    payload_identity: [u8; 32],
    payload_bytes: u64,
    qualification_identity: [u8; 32],
    completeness_scope: String,
    asset_url: String,
    repository: String,
    revision: String,
}

impl VerifiedAcceleratorAuthority {
    pub const fn statement_identity(&self) -> [u8; 32] {
        self.statement_identity
    }

    pub const fn product(&self) -> AcceleratorProduct {
        self.product
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub const fn generation_identity(&self) -> [u8; 32] {
        self.generation_identity
    }

    pub const fn rule_identity(&self) -> [u8; 32] {
        self.rule_identity
    }

    pub const fn payload_identity(&self) -> [u8; 32] {
        self.payload_identity
    }

    pub const fn payload_bytes(&self) -> u64 {
        self.payload_bytes
    }

    pub const fn qualification_identity(&self) -> [u8; 32] {
        self.qualification_identity
    }

    pub fn completeness_scope(&self) -> &str {
        &self.completeness_scope
    }

    pub fn asset_url(&self) -> &str {
        &self.asset_url
    }

    pub fn repository(&self) -> &str {
        &self.repository
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }
}

pub fn verify_accelerator_envelope(
    envelope_json: &str,
    keyring: StaticPublicKeyring<'_>,
) -> Result<VerifiedAcceleratorAuthority, ActivationError> {
    let envelope = parse_canonical(
        envelope_json,
        MAX_ENVELOPE_BYTES,
        "accelerator_envelope_size",
        "accelerator_envelope_json",
    )?;
    exact_keys(
        &envelope,
        &["schema", "signature_hex", "statement_json"],
        "accelerator_envelope_shape",
    )?;
    if string(&envelope, "schema")? != SIGNED_ASSET_ENVELOPE_SCHEMA {
        return Err(ActivationError::new("accelerator_envelope_schema"));
    }
    let statement_json = string(&envelope, "statement_json")?;
    let statement = parse_canonical(
        statement_json,
        MAX_STATEMENT_BYTES,
        "accelerator_statement_size",
        "accelerator_statement_json",
    )?;
    exact_keys(
        &statement,
        &[
            "algorithm",
            "asset_url",
            "completeness_scope",
            "generation_identity",
            "key_id",
            "payload_bytes",
            "payload_identity",
            "product",
            "profile",
            "qualification_identity",
            "repository",
            "revision",
            "rule_identity",
            "schema",
        ],
        "accelerator_statement_shape",
    )?;
    if string(&statement, "schema")? != ASSET_STATEMENT_SCHEMA
        || string(&statement, "algorithm")? != SIGNATURE_ALGORITHM
    {
        return Err(ActivationError::new("accelerator_statement_schema"));
    }
    let key_id = string(&statement, "key_id")?;
    let public_key = keyring
        .find(key_id)
        .ok_or(ActivationError::new("accelerator_signing_key_unknown"))?;
    let signature = decode_hex::<64>(string(&envelope, "signature_hex")?)
        .map_err(|_| ActivationError::new("accelerator_signature_encoding"))?;
    let mut signed = Vec::with_capacity(SIGNATURE_DOMAIN.len() + statement_json.len());
    signed.extend_from_slice(SIGNATURE_DOMAIN);
    signed.extend_from_slice(statement_json.as_bytes());
    VerifyingKey::from_bytes(&public_key)
        .map_err(|_| ActivationError::new("accelerator_public_key_invalid"))?
        .verify(&signed, &Signature::from_bytes(&signature))
        .map_err(|_| ActivationError::new("accelerator_signature_invalid"))?;

    let product = AcceleratorProduct::parse(string(&statement, "product")?)?;
    let profile = string(&statement, "profile")?;
    if !PROFILES.contains(&profile) {
        return Err(ActivationError::new("accelerator_profile_unsupported"));
    }
    let generation_identity = identity(&statement, "generation_identity")?;
    let rule_identity = identity(&statement, "rule_identity")?;
    let payload_identity = identity(&statement, "payload_identity")?;
    let qualification_identity = identity(&statement, "qualification_identity")?;
    let payload_bytes = canonical_u64(string(&statement, "payload_bytes")?)?;
    if payload_bytes == 0 {
        return Err(ActivationError::new("accelerator_payload_bytes"));
    }
    let completeness_scope = nonempty_string(&statement, "completeness_scope")?;
    let asset_url = nonempty_string(&statement, "asset_url")?;
    if !asset_url.starts_with("https://") {
        return Err(ActivationError::new("accelerator_asset_url"));
    }
    let repository = nonempty_string(&statement, "repository")?;
    let revision = nonempty_string(&statement, "revision")?;
    if revision.len() != 40 || !revision.bytes().all(|value| value.is_ascii_hexdigit()) {
        return Err(ActivationError::new("accelerator_revision"));
    }
    Ok(VerifiedAcceleratorAuthority {
        statement_identity: Sha256::digest(statement_json.as_bytes()).into(),
        product,
        profile: profile.to_owned(),
        generation_identity,
        rule_identity,
        payload_identity,
        payload_bytes,
        qualification_identity,
        completeness_scope: completeness_scope.to_owned(),
        asset_url: asset_url.to_owned(),
        repository: repository.to_owned(),
        revision: revision.to_ascii_lowercase(),
    })
}

fn parse_canonical(
    text: &str,
    maximum_bytes: usize,
    size_code: &'static str,
    json_code: &'static str,
) -> Result<Value, ActivationError> {
    if text.is_empty() || text.len() > maximum_bytes {
        return Err(ActivationError::new(size_code));
    }
    let value: Value = serde_json::from_str(text).map_err(|_| ActivationError::new(json_code))?;
    let canonical = serde_json::to_string(&value).map_err(|_| ActivationError::new(json_code))?;
    if canonical != text {
        return Err(ActivationError::new(json_code));
    }
    Ok(value)
}

fn exact_keys(value: &Value, expected: &[&str], code: &'static str) -> Result<(), ActivationError> {
    let object = value.as_object().ok_or(ActivationError::new(code))?;
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        return Err(ActivationError::new(code));
    }
    Ok(())
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, ActivationError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(ActivationError::new("accelerator_statement_value"))
}

fn nonempty_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, ActivationError> {
    let text = string(value, key)?;
    if text.is_empty() || text.len() > 4_096 || text.chars().any(char::is_control) {
        return Err(ActivationError::new("accelerator_statement_value"));
    }
    Ok(text)
}

fn identity(value: &Value, key: &str) -> Result<[u8; 32], ActivationError> {
    decode_hex::<32>(string(value, key)?).map_err(|_| ActivationError::new("accelerator_identity"))
}

fn canonical_u64(value: &str) -> Result<u64, ActivationError> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| ActivationError::new("accelerator_payload_bytes"))?;
    if parsed.to_string() != value {
        return Err(ActivationError::new("accelerator_payload_bytes"));
    }
    Ok(parsed)
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], ()> {
    if value.len() != N * 2 {
        return Err(());
    }
    let mut output = [0_u8; N];
    for (index, destination) in output.iter_mut().enumerate() {
        let offset = index * 2;
        *destination = u8::from_str_radix(&value[offset..offset + 2], 16).map_err(|_| ())?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;

    const SECRET: [u8; 32] = [23; 32];

    fn envelope(product: &str, scope: &str, corrupt: bool) -> String {
        let signing = SigningKey::from_bytes(&SECRET);
        let statement = serde_json::to_string(&json!({
            "algorithm": SIGNATURE_ALGORITHM,
            "asset_url": "https://github.com/daejunnom/Clearra/releases/download/v0.8.1/legal.cllb",
            "completeness_scope": scope,
            "generation_identity": "11".repeat(32),
            "key_id": "test-release",
            "payload_bytes": "1024",
            "payload_identity": "22".repeat(32),
            "product": product,
            "profile": "srs-plus",
            "qualification_identity": "33".repeat(32),
            "repository": "daejunnom/Clearra",
            "revision": "44".repeat(20),
            "rule_identity": "55".repeat(32),
            "schema": ASSET_STATEMENT_SCHEMA
        }))
        .unwrap();
        let mut material = SIGNATURE_DOMAIN.to_vec();
        material.extend_from_slice(statement.as_bytes());
        let mut signature = signing.sign(&material).to_bytes();
        if corrupt {
            signature[0] ^= 1;
        }
        serde_json::to_string(&json!({
            "schema": SIGNED_ASSET_ENVELOPE_SCHEMA,
            "signature_hex": signature.iter().map(|value| format!("{value:02x}")).collect::<String>(),
            "statement_json": statement
        }))
        .unwrap()
    }

    #[test]
    fn signed_authority_is_opaque_and_product_scoped() {
        let keys = [PinnedPublicKey {
            key_id: "test-release",
            public_key: SigningKey::from_bytes(&SECRET).verifying_key().to_bytes(),
        }];
        let verified = verify_accelerator_envelope(
            &envelope(
                "exact-legal-board",
                "empty-origin-10x4-four-lines-f-intersection-r",
                false,
            ),
            StaticPublicKeyring::new(&keys),
        )
        .unwrap();
        assert_eq!(verified.product(), AcceleratorProduct::ExactLegalBoard);
        assert_eq!(verified.profile(), "srs-plus");
        assert_eq!(verified.payload_bytes(), 1024);
        assert_eq!(
            verify_accelerator_envelope(
                &envelope(
                    "exact-legal-board",
                    "empty-origin-10x4-four-lines-f-intersection-r",
                    true,
                ),
                StaticPublicKeyring::new(&keys),
            )
            .unwrap_err()
            .code(),
            "accelerator_signature_invalid"
        );
    }
}
