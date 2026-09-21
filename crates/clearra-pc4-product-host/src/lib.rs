//! Product-host boundary for signed PC4 activation.
//!
//! Network discovery and persistence stay in CLI/Web/Desktop/Discord hosts.
//! This crate owns only the checked-in public trust anchor, the product
//! compatibility identity, signed-authority verification, and conversion of
//! the verified effective generation into the App's opaque snapshot.

use clearra_pc4_activation::{
    verify_and_link, ActivationError, LinkedActivation, ReplayState, StaticPublicKeyring,
    PRODUCTION_CHANNEL,
};
use clearra_pc4_tablebase::ActivatedSnapshot;
use serde_json::Value;
use sha2::{Digest, Sha256};

const PUBLIC_KEYRING_JSON: &str = include_str!("../../../config/pc4-activation-keyring.v1.json");
const PRODUCT_COMPATIBILITY_JSON: &str =
    include_str!("../../../config/pc4-product-compatibility.v1.json");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductActivationError {
    code: &'static str,
}

impl ProductActivationError {
    const fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub const fn code(self) -> &'static str {
        self.code
    }
}

impl core::fmt::Display for ProductActivationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for ProductActivationError {}

impl From<ActivationError> for ProductActivationError {
    fn from(error: ActivationError) -> Self {
        Self::new(error.code())
    }
}

#[derive(Clone, Debug)]
pub struct VerifiedProductActivation {
    linked: LinkedActivation,
    snapshot: ActivatedSnapshot,
}

impl VerifiedProductActivation {
    pub fn linked(&self) -> &LinkedActivation {
        &self.linked
    }

    pub fn snapshot(&self) -> &ActivatedSnapshot {
        &self.snapshot
    }

    pub fn into_parts(self) -> (LinkedActivation, ActivatedSnapshot) {
        (self.linked, self.snapshot)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn verify_product_activation(
    reader_generation_json: &str,
    signed_generation_envelope_json: &str,
    signed_rollout_envelope_json: &str,
    replay_state: &ReplayState,
    now_unix_seconds: u64,
    bootstrap_min_sequence: u64,
) -> Result<VerifiedProductActivation, ProductActivationError> {
    let keyring = production_keyring()?;
    let compatibility = product_compatibility_identity()?;
    verify_and_activate_with_keyring(
        reader_generation_json,
        signed_generation_envelope_json,
        signed_rollout_envelope_json,
        &keyring,
        replay_state,
        now_unix_seconds,
        bootstrap_min_sequence,
        &compatibility,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn verify_and_activate_with_keyring(
    reader_generation_json: &str,
    signed_generation_envelope_json: &str,
    signed_rollout_envelope_json: &str,
    keyring: &StaticPublicKeyring,
    replay_state: &ReplayState,
    now_unix_seconds: u64,
    bootstrap_min_sequence: u64,
    expected_compatibility_identity: &str,
) -> Result<VerifiedProductActivation, ProductActivationError> {
    let linked = verify_and_link(
        reader_generation_json,
        signed_generation_envelope_json,
        signed_rollout_envelope_json,
        keyring,
        replay_state,
        now_unix_seconds,
        bootstrap_min_sequence,
        PRODUCTION_CHANNEL,
        expected_compatibility_identity,
    )?;
    let snapshot = clearra_app::activate_pc4_host_generation(linked.host_generation_json())
        .map_err(ProductActivationError::new)?
        .ok_or(ProductActivationError::new(
            "pc4_product_activation_empty_generation",
        ))?;
    Ok(VerifiedProductActivation { linked, snapshot })
}

pub fn production_keyring() -> Result<StaticPublicKeyring, ProductActivationError> {
    StaticPublicKeyring::from_static_json(PUBLIC_KEYRING_JSON).map_err(Into::into)
}

pub fn product_compatibility_identity() -> Result<String, ProductActivationError> {
    validate_product_compatibility_document(PRODUCT_COMPATIBILITY_JSON)?;
    Ok(format!(
        "sha256:{:x}",
        Sha256::digest(PRODUCT_COMPATIBILITY_JSON.as_bytes())
    ))
}

fn validate_product_compatibility_document(json: &str) -> Result<(), ProductActivationError> {
    if json.len() > 4_096
        || json.as_bytes().contains(&b'\r')
        || !json.ends_with('\n')
        || json.ends_with("\n\n")
    {
        return Err(ProductActivationError::new(
            "pc4_product_compatibility_text",
        ));
    }
    let value: Value = serde_json::from_str(json)
        .map_err(|_| ProductActivationError::new("pc4_product_compatibility_json"))?;
    let object = value.as_object().ok_or(ProductActivationError::new(
        "pc4_product_compatibility_shape",
    ))?;
    let mut keys = object.keys().map(String::as_str).collect::<Vec<_>>();
    keys.sort_unstable();
    if keys
        != [
            "activation_contract",
            "host_generation_schema",
            "product_adapter",
            "reader_contract",
            "schema",
        ]
    {
        return Err(ProductActivationError::new(
            "pc4_product_compatibility_shape",
        ));
    }
    if value["schema"] != "clearra.pc4.product-compatibility.v1"
        || value["activation_contract"] != "clearra.pc4.production-activation.v1"
        || value["host_generation_schema"] != "clearra.pc4.host-generation.v1"
        || value["product_adapter"] != "clearra.pc4.product-host.v1"
        || value["reader_contract"] != "hydra-jstris-180-complete-graph-v1"
    {
        return Err(ProductActivationError::new(
            "pc4_product_compatibility_contract",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_public_keyring_and_compatibility_document_are_valid() {
        production_keyring().expect("checked-in production keyring");
        let identity = product_compatibility_identity().expect("compatibility identity");
        assert!(identity.starts_with("sha256:"));
        assert_eq!(identity.len(), 71);
    }

    #[test]
    fn unsigned_or_malformed_authority_never_reaches_the_app_parser() {
        let error =
            verify_product_activation("{}", "{}", "{}", &ReplayState::empty(), 1, 1).unwrap_err();
        assert!(error.code().starts_with("pc4_activation_"));
    }
}
