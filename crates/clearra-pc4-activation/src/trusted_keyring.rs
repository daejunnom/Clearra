use std::collections::HashSet;

use ed25519_dalek::VerifyingKey;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    error::{ActivationError, Result},
    PUBLIC_KEYRING_SCHEMA, SIGNATURE_ALGORITHM,
};

#[derive(Clone, Copy, Debug)]
pub struct PinnedPublicKey {
    key_id: &'static str,
    public_key: [u8; 32],
}

impl PinnedPublicKey {
    pub const fn new(key_id: &'static str, public_key: [u8; 32]) -> Self {
        Self { key_id, public_key }
    }
}

#[derive(Clone, Debug)]
struct TrustedPublicKey {
    key_id: String,
    public_key: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct StaticPublicKeyring {
    keys: Vec<TrustedPublicKey>,
}

impl StaticPublicKeyring {
    pub fn from_pinned(keys: &'static [PinnedPublicKey]) -> Result<Self> {
        if keys.is_empty() || keys.len() > 8 {
            return Err(ActivationError::new("pc4_activation_keyring_size"));
        }
        let mut trusted = Vec::with_capacity(keys.len());
        let mut identities = HashSet::new();
        for key in keys {
            validate_key(key.key_id, &key.public_key)?;
            if !identities.insert(key.key_id) {
                return Err(ActivationError::new("pc4_activation_keyring_duplicate"));
            }
            trusted.push(TrustedPublicKey {
                key_id: key.key_id.to_owned(),
                public_key: key.public_key,
            });
        }
        Ok(Self { keys: trusted })
    }

    /// Parses checked-in public material only. Product adapters must call this
    /// with compile-time embedded source, never bytes obtained from a request.
    pub fn from_static_json(json: &'static str) -> Result<Self> {
        if json.len() > 16_384 || json.as_bytes().contains(&b'\r') {
            return Err(ActivationError::new("pc4_activation_keyring_text"));
        }
        let value: Value = serde_json::from_str(json)
            .map_err(|_| ActivationError::new("pc4_activation_keyring_json"))?;
        exact_keys(&value, &["keys", "schema"], "pc4_activation_keyring_shape")?;
        if value["schema"].as_str() != Some(PUBLIC_KEYRING_SCHEMA) {
            return Err(ActivationError::new("pc4_activation_keyring_schema"));
        }
        let entries = value["keys"]
            .as_array()
            .ok_or(ActivationError::new("pc4_activation_keyring_shape"))?;
        if entries.is_empty() || entries.len() > 8 {
            return Err(ActivationError::new("pc4_activation_keyring_size"));
        }
        let mut keys = Vec::with_capacity(entries.len());
        let mut identities = HashSet::new();
        for entry in entries {
            exact_keys(
                entry,
                &["algorithm", "key_id", "public_key_hex", "status"],
                "pc4_activation_keyring_entry_shape",
            )?;
            if entry["algorithm"].as_str() != Some(SIGNATURE_ALGORITHM)
                || !matches!(entry["status"].as_str(), Some("active" | "retiring"))
            {
                return Err(ActivationError::new("pc4_activation_keyring_entry"));
            }
            let key_id = entry["key_id"]
                .as_str()
                .ok_or(ActivationError::new("pc4_activation_keyring_entry"))?;
            let public_key = decode_hex_array::<32>(
                entry["public_key_hex"]
                    .as_str()
                    .ok_or(ActivationError::new("pc4_activation_keyring_entry"))?,
                "pc4_activation_public_key",
            )?;
            validate_key(key_id, &public_key)?;
            if !identities.insert(key_id.to_owned()) {
                return Err(ActivationError::new("pc4_activation_keyring_duplicate"));
            }
            keys.push(TrustedPublicKey {
                key_id: key_id.to_owned(),
                public_key,
            });
        }
        Ok(Self { keys })
    }

    pub(crate) fn verifying_key(&self, key_id: &str) -> Result<VerifyingKey> {
        let entry = self
            .keys
            .iter()
            .find(|entry| entry.key_id == key_id)
            .ok_or(ActivationError::new("pc4_activation_untrusted_key"))?;
        VerifyingKey::from_bytes(&entry.public_key)
            .map_err(|_| ActivationError::new("pc4_activation_public_key"))
    }
}

fn validate_key(key_id: &str, public_key: &[u8; 32]) -> Result<()> {
    if key_id != key_identity(public_key) {
        return Err(ActivationError::new("pc4_activation_key_identity"));
    }
    VerifyingKey::from_bytes(public_key)
        .map_err(|_| ActivationError::new("pc4_activation_public_key"))?;
    Ok(())
}

pub(crate) fn key_identity(public_key: &[u8; 32]) -> String {
    format!("ed25519-raw-sha256:{:x}", Sha256::digest(public_key))
}

pub(crate) fn decode_hex_array<const N: usize>(value: &str, code: &'static str) -> Result<[u8; N]> {
    if value.len() != N * 2
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase())
    {
        return Err(ActivationError::new(code));
    }
    let mut output = [0_u8; N];
    for (index, byte) in output.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(&value[offset..offset + 2], 16)
            .map_err(|_| ActivationError::new(code))?;
    }
    Ok(output)
}

pub(crate) fn exact_keys(value: &Value, expected: &[&str], code: &'static str) -> Result<()> {
    let object = value.as_object().ok_or(ActivationError::new(code))?;
    let mut actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    let mut wanted = expected.to_vec();
    actual.sort_unstable();
    wanted.sort_unstable();
    if actual != wanted {
        return Err(ActivationError::new(code));
    }
    Ok(())
}
