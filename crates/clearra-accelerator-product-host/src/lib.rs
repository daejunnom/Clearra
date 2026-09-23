//! Checked-in product authority for exact legal-board and conditioned
//! reachability accelerator assets.
//!
//! This crate deliberately performs no filesystem or network I/O. Hosts first
//! ask it to validate the source-controlled catalog and signed release
//! statement, then use the returned opaque authority to download, parse and
//! qualify the exact payload in the product-specific crate.

use clearra_accelerator_activation::{
    verify_accelerator_envelope, AcceleratorProduct, PinnedPublicKey, StaticPublicKeyring,
    VerifiedAcceleratorAuthority,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

const KEYRING_JSON: &str = include_str!("../../../config/accelerator-activation-keyring.v1.json");
const LEGAL_BOARD_CATALOG_JSON: &str =
    include_str!("../../../config/legal-board-product-catalog.v1.json");
const CONDITIONED_CATALOG_JSON: &str =
    include_str!("../../../config/conditioned-reachability-product-catalog.v1.json");

const KEY_ID: &str =
    "ed25519-raw-sha256:ff726633a4ac56866d2b2d66e4165254692686f6f06a2597f970277b0c04c6d4";
const PUBLIC_KEY: [u8; 32] = [
    0xa6, 0xca, 0x31, 0xd0, 0xfb, 0xfa, 0xc1, 0x25, 0xd5, 0x4f, 0x7b, 0x15, 0xbe, 0x5c, 0x37, 0xe2,
    0xb5, 0x43, 0x60, 0x88, 0x9c, 0xf4, 0x16, 0x2f, 0xaf, 0xe3, 0xd5, 0x36, 0xce, 0x27, 0x93, 0xfa,
];
const CONDITIONED_KEY_ID: &str =
    "ed25519-raw-sha256:764ecfc06e730760c3d6704c71dea629cc6895ba3e202d40ce9e6ad96ef49328";
const CONDITIONED_PUBLIC_KEY: [u8; 32] = [
    0x18, 0x70, 0xcd, 0xc0, 0x5d, 0x9d, 0xa3, 0x04, 0x1a, 0xe2, 0xe0, 0x76, 0xa8, 0x76, 0xd3, 0x28,
    0x85, 0x5a, 0xbd, 0x7f, 0x06, 0x85, 0x68, 0x23, 0xa3, 0x92, 0x56, 0xe4, 0x67, 0xd9, 0x5a, 0x8e,
];
const PRODUCTION_KEYS: [PinnedPublicKey; 2] = [
    PinnedPublicKey {
        key_id: KEY_ID,
        public_key: PUBLIC_KEY,
    },
    PinnedPublicKey {
        key_id: CONDITIONED_KEY_ID,
        public_key: CONDITIONED_PUBLIC_KEY,
    },
];
const PROFILES: [&str; 5] = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"];
const REPOSITORY: &str = "daejunnom/Clearra";
const RELEASE_URL_PREFIX: &str = "https://github.com/daejunnom/Clearra/releases/download/";
const MAX_CATALOG_BYTES: usize = 512 * 1024;
const LEGAL_BOARD_MAX_BYTES: u64 = 64 * 1024 * 1024;
const CONDITIONED_MAX_BYTES: u64 = 16 * 1024 * 1024;
const LEGAL_BOARD_SCOPE: &str = "empty-origin-10x4-four-lines-f-intersection-r";
const CONDITIONED_SCOPE: &str =
    "width10-height1to6-solver-sky-bottom8-56-contexts-entry-first-exit-boolean";

/// The historical sparse spawn-to-lock cache remains an unqualified local
/// fixture. The product parser and solver now use actual-entry/first-exit
/// relations; this enum keeps the release gate honest about that boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionedRelationContract {
    SparseSpawnToLock,
    ActualEntryToFirstExit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductCatalogKind {
    ExactLegalBoard,
    BoardConditionedReachability,
}

impl ProductCatalogKind {
    pub const fn product(self) -> AcceleratorProduct {
        match self {
            Self::ExactLegalBoard => AcceleratorProduct::ExactLegalBoard,
            Self::BoardConditionedReachability => AcceleratorProduct::BoardConditionedReachability,
        }
    }

    pub const fn as_str(self) -> &'static str {
        self.product().as_str()
    }

    pub const fn conditioned_relation_contract(self) -> Option<ConditionedRelationContract> {
        match self {
            Self::ExactLegalBoard => None,
            Self::BoardConditionedReachability => {
                Some(ConditionedRelationContract::ActualEntryToFirstExit)
            }
        }
    }

    const fn schema(self) -> &'static str {
        match self {
            Self::ExactLegalBoard => "clearra.legal-board.product-catalog.v1",
            Self::BoardConditionedReachability => {
                "clearra.conditioned-reachability.product-catalog.v1"
            }
        }
    }

    pub const fn completeness_scope(self) -> &'static str {
        match self {
            Self::ExactLegalBoard => LEGAL_BOARD_SCOPE,
            Self::BoardConditionedReachability => CONDITIONED_SCOPE,
        }
    }

    const fn maximum_payload_bytes(self) -> u64 {
        match self {
            Self::ExactLegalBoard => LEGAL_BOARD_MAX_BYTES,
            Self::BoardConditionedReachability => CONDITIONED_MAX_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductCatalogError {
    code: &'static str,
}

impl ProductCatalogError {
    const fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub const fn code(self) -> &'static str {
        self.code
    }
}

impl core::fmt::Display for ProductCatalogError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for ProductCatalogError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QualifiedProductMetadata {
    ExactLegalBoard {
        active_session_shared_bytes: u64,
        chain_identity: [u8; 32],
        layer_counts: [u64; 11],
        layer_payload_identities: [[u8; 32]; 11],
    },
    BoardConditionedReachability {
        active_session_shared_bytes: u64,
        query_set_identity: [u8; 32],
        record_count: u64,
    },
}

impl QualifiedProductMetadata {
    pub const fn active_session_shared_bytes(&self) -> u64 {
        match self {
            Self::ExactLegalBoard {
                active_session_shared_bytes,
                ..
            }
            | Self::BoardConditionedReachability {
                active_session_shared_bytes,
                ..
            } => *active_session_shared_bytes,
        }
    }
}

#[derive(Clone, Debug)]
pub struct QualifiedCatalogAsset {
    authority: VerifiedAcceleratorAuthority,
    metadata: QualifiedProductMetadata,
    envelope_json: String,
}

impl QualifiedCatalogAsset {
    pub fn authority(&self) -> &VerifiedAcceleratorAuthority {
        &self.authority
    }

    pub fn metadata(&self) -> &QualifiedProductMetadata {
        &self.metadata
    }

    pub fn envelope_json(&self) -> &str {
        &self.envelope_json
    }
}

#[derive(Clone, Debug)]
pub enum CatalogProfileStatus {
    NotQualified,
    Qualified(QualifiedCatalogAsset),
}

impl CatalogProfileStatus {
    pub const fn is_qualified(&self) -> bool {
        matches!(self, Self::Qualified(_))
    }
}

#[derive(Clone, Debug)]
pub struct CatalogProfile {
    profile: &'static str,
    status: CatalogProfileStatus,
}

impl CatalogProfile {
    pub const fn profile(&self) -> &'static str {
        self.profile
    }

    pub const fn status(&self) -> &CatalogProfileStatus {
        &self.status
    }
}

#[derive(Clone, Debug)]
pub struct VerifiedProductCatalog {
    kind: ProductCatalogKind,
    catalog_identity: [u8; 32],
    profiles: [CatalogProfile; 5],
}

impl VerifiedProductCatalog {
    pub const fn kind(&self) -> ProductCatalogKind {
        self.kind
    }

    pub const fn catalog_identity(&self) -> [u8; 32] {
        self.catalog_identity
    }

    pub const fn profiles(&self) -> &[CatalogProfile; 5] {
        &self.profiles
    }

    pub fn profile(&self, profile: &str) -> Option<&CatalogProfileStatus> {
        self.profiles
            .iter()
            .find(|entry| entry.profile == profile)
            .map(CatalogProfile::status)
    }

    pub fn all_profiles_qualified(&self) -> bool {
        self.profiles
            .iter()
            .all(|entry| entry.status.is_qualified())
    }
}

pub fn embedded_catalog(
    kind: ProductCatalogKind,
) -> Result<VerifiedProductCatalog, ProductCatalogError> {
    let text = match kind {
        ProductCatalogKind::ExactLegalBoard => LEGAL_BOARD_CATALOG_JSON,
        ProductCatalogKind::BoardConditionedReachability => CONDITIONED_CATALOG_JSON,
    };
    parse_catalog(text, kind)
}

pub const fn embedded_catalog_json(kind: ProductCatalogKind) -> &'static str {
    match kind {
        ProductCatalogKind::ExactLegalBoard => LEGAL_BOARD_CATALOG_JSON,
        ProductCatalogKind::BoardConditionedReachability => CONDITIONED_CATALOG_JSON,
    }
}

pub fn production_keyring() -> Result<StaticPublicKeyring<'static>, ProductCatalogError> {
    validate_keyring_document(KEYRING_JSON)?;
    Ok(StaticPublicKeyring::new(&PRODUCTION_KEYS))
}

fn parse_catalog(
    text: &str,
    kind: ProductCatalogKind,
) -> Result<VerifiedProductCatalog, ProductCatalogError> {
    validate_text(text, MAX_CATALOG_BYTES, "accelerator_catalog_text")?;
    let value: Value = serde_json::from_str(text)
        .map_err(|_| ProductCatalogError::new("accelerator_catalog_json"))?;
    exact_keys(
        object(&value, "accelerator_catalog_shape")?,
        &["product", "profiles", "schema"],
        "accelerator_catalog_shape",
    )?;
    if string(&value, "schema")? != kind.schema() || string(&value, "product")? != kind.as_str() {
        return Err(ProductCatalogError::new("accelerator_catalog_contract"));
    }
    let entries = value["profiles"]
        .as_array()
        .filter(|entries| entries.len() == PROFILES.len())
        .ok_or(ProductCatalogError::new("accelerator_catalog_profiles"))?;
    let keyring = production_keyring()?;
    let mut parsed = Vec::with_capacity(PROFILES.len());
    for (index, expected_profile) in PROFILES.iter().copied().enumerate() {
        parsed.push(parse_profile(
            &entries[index],
            expected_profile,
            kind,
            keyring,
        )?);
    }
    let profiles: [CatalogProfile; 5] = parsed
        .try_into()
        .map_err(|_| ProductCatalogError::new("accelerator_catalog_profiles"))?;
    Ok(VerifiedProductCatalog {
        kind,
        catalog_identity: Sha256::digest(text.as_bytes()).into(),
        profiles,
    })
}

fn parse_profile(
    value: &Value,
    expected_profile: &'static str,
    kind: ProductCatalogKind,
    keyring: StaticPublicKeyring<'_>,
) -> Result<CatalogProfile, ProductCatalogError> {
    exact_keys(
        object(value, "accelerator_catalog_profile_shape")?,
        &["activation_envelope_json", "metadata", "profile", "status"],
        "accelerator_catalog_profile_shape",
    )?;
    if string(value, "profile")? != expected_profile {
        return Err(ProductCatalogError::new(
            "accelerator_catalog_profile_order",
        ));
    }
    let status = match string(value, "status")? {
        "not_qualified" => {
            if !value["activation_envelope_json"].is_null() || !value["metadata"].is_null() {
                return Err(ProductCatalogError::new(
                    "accelerator_catalog_unqualified_payload",
                ));
            }
            CatalogProfileStatus::NotQualified
        }
        "qualified" => {
            let envelope_json = value["activation_envelope_json"]
                .as_str()
                .filter(|text| !text.is_empty())
                .ok_or(ProductCatalogError::new(
                    "accelerator_catalog_qualified_envelope",
                ))?;
            let authority = verify_accelerator_envelope(envelope_json, keyring)
                .map_err(|_| ProductCatalogError::new("accelerator_catalog_signature"))?;
            if authority.product() != kind.product()
                || authority.profile() != expected_profile
                || authority.repository() != REPOSITORY
                || !authority.asset_url().starts_with(RELEASE_URL_PREFIX)
                || authority.completeness_scope() != kind.completeness_scope()
                || authority.payload_bytes() > kind.maximum_payload_bytes()
            {
                return Err(ProductCatalogError::new(
                    "accelerator_catalog_authority_mismatch",
                ));
            }
            let metadata = parse_qualified_metadata(&value["metadata"], kind, &authority)?;
            CatalogProfileStatus::Qualified(QualifiedCatalogAsset {
                authority,
                metadata,
                envelope_json: envelope_json.to_owned(),
            })
        }
        _ => return Err(ProductCatalogError::new("accelerator_catalog_status")),
    };
    Ok(CatalogProfile {
        profile: expected_profile,
        status,
    })
}

fn parse_qualified_metadata(
    value: &Value,
    kind: ProductCatalogKind,
    authority: &VerifiedAcceleratorAuthority,
) -> Result<QualifiedProductMetadata, ProductCatalogError> {
    let common = match kind {
        ProductCatalogKind::ExactLegalBoard => &[
            "active_session_shared_bytes",
            "chain_identity",
            "generation_identity",
            "layer_counts",
            "layer_payload_identities",
            "payload_bytes",
            "payload_identity",
            "qualification_identity",
            "rule_identity",
            "url",
        ][..],
        ProductCatalogKind::BoardConditionedReachability => &[
            "active_session_shared_bytes",
            "generation_identity",
            "payload_bytes",
            "payload_identity",
            "qualification_identity",
            "query_set_identity",
            "record_count",
            "rule_identity",
            "url",
        ][..],
    };
    exact_keys(
        object(value, "accelerator_catalog_metadata_shape")?,
        common,
        "accelerator_catalog_metadata_shape",
    )?;
    if identity(value, "generation_identity")? != authority.generation_identity()
        || identity(value, "rule_identity")? != authority.rule_identity()
        || identity(value, "payload_identity")? != authority.payload_identity()
        || identity(value, "qualification_identity")? != authority.qualification_identity()
        || canonical_u64(value, "payload_bytes")? != authority.payload_bytes()
        || string(value, "url")? != authority.asset_url()
    {
        return Err(ProductCatalogError::new(
            "accelerator_catalog_metadata_authority_mismatch",
        ));
    }
    match kind {
        ProductCatalogKind::ExactLegalBoard => {
            let active_session_shared_bytes = canonical_u64(value, "active_session_shared_bytes")?;
            let chain_identity = identity(value, "chain_identity")?;
            let layer_counts = fixed_u64_array::<11>(&value["layer_counts"])?;
            let layer_payload_identities =
                fixed_identity_array::<11>(&value["layer_payload_identities"])?;
            if active_session_shared_bytes == 0
                || active_session_shared_bytes > 128 * 1024 * 1024
                || active_session_shared_bytes < authority.payload_bytes()
                || chain_identity == [0; 32]
                || layer_counts.contains(&0)
            {
                return Err(ProductCatalogError::new(
                    "accelerator_catalog_legal_metadata",
                ));
            }
            Ok(QualifiedProductMetadata::ExactLegalBoard {
                active_session_shared_bytes,
                chain_identity,
                layer_counts,
                layer_payload_identities,
            })
        }
        ProductCatalogKind::BoardConditionedReachability => {
            let active_session_shared_bytes = canonical_u64(value, "active_session_shared_bytes")?;
            let query_set_identity = identity(value, "query_set_identity")?;
            let record_count = canonical_u64(value, "record_count")?;
            if active_session_shared_bytes == 0
                || active_session_shared_bytes > 128 * 1024 * 1024
                || active_session_shared_bytes < authority.payload_bytes()
                || query_set_identity == [0; 32]
                || record_count == 0
            {
                return Err(ProductCatalogError::new(
                    "accelerator_catalog_conditioned_metadata",
                ));
            }
            Ok(QualifiedProductMetadata::BoardConditionedReachability {
                active_session_shared_bytes,
                query_set_identity,
                record_count,
            })
        }
    }
}

fn validate_keyring_document(text: &str) -> Result<(), ProductCatalogError> {
    validate_text(text, 4_096, "accelerator_keyring_text")?;
    let value: Value = serde_json::from_str(text)
        .map_err(|_| ProductCatalogError::new("accelerator_keyring_json"))?;
    exact_keys(
        object(&value, "accelerator_keyring_shape")?,
        &["keys", "schema"],
        "accelerator_keyring_shape",
    )?;
    if string(&value, "schema")? != "clearra.accelerator.activation-keyring.v1" {
        return Err(ProductCatalogError::new("accelerator_keyring_schema"));
    }
    let entries = value["keys"]
        .as_array()
        .filter(|entries| entries.len() == PRODUCTION_KEYS.len())
        .ok_or(ProductCatalogError::new("accelerator_keyring_keys"))?;
    for (entry, pinned) in entries.iter().zip(PRODUCTION_KEYS) {
        exact_keys(
            object(entry, "accelerator_keyring_key_shape")?,
            &["algorithm", "key_id", "public_key_hex", "status"],
            "accelerator_keyring_key_shape",
        )?;
        if string(entry, "algorithm")? != "ed25519"
            || string(entry, "key_id")? != pinned.key_id
            || decode_hex::<32>(string(entry, "public_key_hex")?)? != pinned.public_key
            || string(entry, "status")? != "active"
        {
            return Err(ProductCatalogError::new("accelerator_keyring_contract"));
        }
    }
    Ok(())
}

fn validate_text(
    text: &str,
    maximum: usize,
    code: &'static str,
) -> Result<(), ProductCatalogError> {
    if text.is_empty()
        || text.len() > maximum
        || text.as_bytes().contains(&b'\r')
        || !text.ends_with('\n')
        || text.ends_with("\n\n")
    {
        return Err(ProductCatalogError::new(code));
    }
    Ok(())
}

fn object<'a>(
    value: &'a Value,
    code: &'static str,
) -> Result<&'a Map<String, Value>, ProductCatalogError> {
    value.as_object().ok_or(ProductCatalogError::new(code))
}

fn exact_keys(
    object: &Map<String, Value>,
    expected: &[&str],
    code: &'static str,
) -> Result<(), ProductCatalogError> {
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        return Err(ProductCatalogError::new(code));
    }
    Ok(())
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, ProductCatalogError> {
    value[key]
        .as_str()
        .ok_or(ProductCatalogError::new("accelerator_catalog_value"))
}

fn identity(value: &Value, key: &str) -> Result<[u8; 32], ProductCatalogError> {
    decode_hex::<32>(string(value, key)?)
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], ProductCatalogError> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ProductCatalogError::new("accelerator_catalog_hex"));
    }
    let mut output = [0_u8; N];
    for (index, destination) in output.iter_mut().enumerate() {
        let offset = index * 2;
        *destination = u8::from_str_radix(&value[offset..offset + 2], 16)
            .map_err(|_| ProductCatalogError::new("accelerator_catalog_hex"))?;
    }
    Ok(output)
}

fn canonical_u64(value: &Value, key: &str) -> Result<u64, ProductCatalogError> {
    let text = string(value, key)?;
    let parsed = text
        .parse::<u64>()
        .map_err(|_| ProductCatalogError::new("accelerator_catalog_integer"))?;
    if parsed.to_string() != text {
        return Err(ProductCatalogError::new("accelerator_catalog_integer"));
    }
    Ok(parsed)
}

fn fixed_u64_array<const N: usize>(value: &Value) -> Result<[u64; N], ProductCatalogError> {
    let values = value
        .as_array()
        .filter(|values| values.len() == N)
        .ok_or(ProductCatalogError::new("accelerator_catalog_array"))?;
    let mut output = [0_u64; N];
    for (index, value) in values.iter().enumerate() {
        let text = value
            .as_str()
            .ok_or(ProductCatalogError::new("accelerator_catalog_integer"))?;
        let parsed = text
            .parse::<u64>()
            .map_err(|_| ProductCatalogError::new("accelerator_catalog_integer"))?;
        if parsed.to_string() != text {
            return Err(ProductCatalogError::new("accelerator_catalog_integer"));
        }
        output[index] = parsed;
    }
    Ok(output)
}

fn fixed_identity_array<const N: usize>(
    value: &Value,
) -> Result<[[u8; 32]; N], ProductCatalogError> {
    let values = value
        .as_array()
        .filter(|values| values.len() == N)
        .ok_or(ProductCatalogError::new("accelerator_catalog_array"))?;
    let mut output = [[0_u8; 32]; N];
    for (index, value) in values.iter().enumerate() {
        output[index] = decode_hex::<32>(
            value
                .as_str()
                .ok_or(ProductCatalogError::new("accelerator_catalog_hex"))?,
        )?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn embedded_catalogs_preserve_independent_profile_authority() {
        production_keyring().expect("checked-in public keyring");
        for kind in [
            ProductCatalogKind::ExactLegalBoard,
            ProductCatalogKind::BoardConditionedReachability,
        ] {
            let catalog = embedded_catalog(kind).expect("checked-in catalog");
            assert_eq!(catalog.kind(), kind);
            assert_eq!(
                catalog.all_profiles_qualified(),
                kind == ProductCatalogKind::BoardConditionedReachability
            );
            assert_eq!(
                catalog
                    .profiles()
                    .iter()
                    .map(CatalogProfile::profile)
                    .collect::<Vec<_>>(),
                PROFILES
            );
            for profile in catalog.profiles() {
                match (kind, profile.status()) {
                    (ProductCatalogKind::ExactLegalBoard, CatalogProfileStatus::NotQualified) => {}
                    (
                        ProductCatalogKind::BoardConditionedReachability,
                        CatalogProfileStatus::Qualified(asset),
                    ) => {
                        assert_eq!(asset.authority().profile(), profile.profile());
                        assert_eq!(asset.authority().completeness_scope(), CONDITIONED_SCOPE);
                    }
                    _ => panic!("product authority changed without qualification"),
                }
            }
        }
    }

    #[test]
    fn profile_order_is_part_of_the_catalog_contract() {
        let mut value: Value = serde_json::from_str(LEGAL_BOARD_CATALOG_JSON).unwrap();
        value["profiles"].as_array_mut().unwrap().swap(0, 1);
        let text = format!("{}\n", serde_json::to_string(&value).unwrap());
        assert_eq!(
            parse_catalog(&text, ProductCatalogKind::ExactLegalBoard)
                .unwrap_err()
                .code(),
            "accelerator_catalog_profile_order"
        );
    }

    #[test]
    fn qualified_slot_cannot_use_unsigned_authority() {
        let mut value: Value = serde_json::from_str(LEGAL_BOARD_CATALOG_JSON).unwrap();
        value["profiles"][0] = json!({
            "activation_envelope_json": "{}",
            "metadata": {},
            "profile": "srs",
            "status": "qualified"
        });
        let text = format!("{}\n", serde_json::to_string(&value).unwrap());
        assert_eq!(
            parse_catalog(&text, ProductCatalogKind::ExactLegalBoard)
                .unwrap_err()
                .code(),
            "accelerator_catalog_signature"
        );
    }
}
