pub const CONTRACT_SCHEMA_VERSION: &str = "clearra.search.contract.v2";
pub const SUPPLY_SEMANTICS_ID: &str = "clearra.supply.projected-terminal-lookahead.v1";
pub const ARTIFACT_SCHEMA_VERSION: &str = "clearra.solution-data.v1";
pub const UNVERIFIED_LOCAL_BUILD: &str = "unverified-local-build";

pub const COMPILED_SOURCE_COMMIT: &str = match option_env!("CLEARRA_SOURCE_COMMIT") {
    Some(value) => {
        if !is_lowercase_commit_sha(value) {
            panic!("CLEARRA_SOURCE_COMMIT must be an exact lowercase 40-character commit SHA");
        }
        value
    }
    None => UNVERIFIED_LOCAL_BUILD,
};

pub const COMPILED_ENGINE_BUILD_ID: &str = match option_env!("CLEARRA_ENGINE_BUILD_ID") {
    Some(value) => {
        if !is_lowercase_commit_sha(value) {
            panic!("CLEARRA_ENGINE_BUILD_ID must be an exact lowercase 40-character commit SHA");
        }
        value
    }
    None => UNVERIFIED_LOCAL_BUILD,
};

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProductBuildIdentity {
    engine_build_id: String,
    source_commit: String,
    contract_schema_version: String,
    supply_semantics_id: String,
    artifact_schema_version: String,
}

impl ProductBuildIdentity {
    /// Returns the identity baked into this binary at compile time.
    ///
    /// Source identity never consults the process environment. Release builds
    /// must provide the two SHA values while compiling; local builds remain
    /// explicitly unverified instead of claiming a release commit.
    pub fn current() -> Self {
        Self {
            engine_build_id: COMPILED_ENGINE_BUILD_ID.to_owned(),
            source_commit: COMPILED_SOURCE_COMMIT.to_owned(),
            contract_schema_version: CONTRACT_SCHEMA_VERSION.to_owned(),
            supply_semantics_id: SUPPLY_SEMANTICS_ID.to_owned(),
            artifact_schema_version: ARTIFACT_SCHEMA_VERSION.to_owned(),
        }
    }

    /// Allocation-free owned-parts seam for a boundary that has already
    /// authorized each retained identity string.
    pub fn from_owned_memory_authorized_parts(
        engine_build_id: String,
        source_commit: String,
        contract_schema_version: String,
        supply_semantics_id: String,
        artifact_schema_version: String,
    ) -> Self {
        Self {
            engine_build_id,
            source_commit,
            contract_schema_version,
            supply_semantics_id,
            artifact_schema_version,
        }
    }

    pub fn engine_build_id(&self) -> &str {
        &self.engine_build_id
    }

    pub fn source_commit(&self) -> &str {
        &self.source_commit
    }

    pub fn contract_schema_version(&self) -> &str {
        &self.contract_schema_version
    }

    pub fn supply_semantics_id(&self) -> &str {
        &self.supply_semantics_id
    }

    pub fn artifact_schema_version(&self) -> &str {
        &self.artifact_schema_version
    }

    /// Returns the heap payload retained by all five identity strings, using
    /// actual allocator capacities. Inline `String` owners are excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        [
            &self.engine_build_id,
            &self.source_commit,
            &self.contract_schema_version,
            &self.supply_semantics_id,
            &self.artifact_schema_version,
        ]
        .into_iter()
        .try_fold(0_u128, |bytes, value| {
            bytes.checked_add(value.capacity() as u128)
        })
    }
}

impl Default for ProductBuildIdentity {
    fn default() -> Self {
        Self::current()
    }
}

const fn is_lowercase_commit_sha(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 40 {
        return false;
    }

    let mut index = 0;
    while index < bytes.len() {
        if !matches!(bytes[index], b'0'..=b'9' | b'a'..=b'f') {
            return false;
        }
        index += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_identity_has_the_five_required_product_fields() {
        let identity = ProductBuildIdentity::current();

        assert!(is_compile_identity(identity.engine_build_id()));
        assert!(is_compile_identity(identity.source_commit()));
        assert_eq!(identity.contract_schema_version(), CONTRACT_SCHEMA_VERSION);
        assert_eq!(identity.supply_semantics_id(), SUPPLY_SEMANTICS_ID);
        assert_eq!(identity.artifact_schema_version(), ARTIFACT_SCHEMA_VERSION);

        let value = serde_json::to_value(&identity).expect("serialize product build identity");
        assert_eq!(
            value.as_object().expect("identity object").len(),
            5,
            "the public identity must contain exactly the governed five fields"
        );
        assert_eq!(value["engine_build_id"], identity.engine_build_id());
        assert_eq!(value["source_commit"], identity.source_commit());
        assert_eq!(value["contract_schema_version"], CONTRACT_SCHEMA_VERSION);
        assert_eq!(value["supply_semantics_id"], SUPPLY_SEMANTICS_ID);
        assert_eq!(value["artifact_schema_version"], ARTIFACT_SCHEMA_VERSION);

        if let Some(expected) = option_env!("CLEARRA_SOURCE_COMMIT") {
            assert_eq!(identity.source_commit(), expected);
        } else {
            assert_eq!(identity.source_commit(), UNVERIFIED_LOCAL_BUILD);
        }
        if let Some(expected) = option_env!("CLEARRA_ENGINE_BUILD_ID") {
            assert_eq!(identity.engine_build_id(), expected);
        } else {
            assert_eq!(identity.engine_build_id(), UNVERIFIED_LOCAL_BUILD);
        }

        let decoded: ProductBuildIdentity =
            serde_json::from_value(value.clone()).expect("deserialize complete identity");
        assert_eq!(decoded, identity);

        let mut missing = value;
        missing
            .as_object_mut()
            .expect("identity object")
            .remove("supply_semantics_id");
        assert!(serde_json::from_value::<ProductBuildIdentity>(missing).is_err());
    }

    #[test]
    fn compile_identity_accepts_only_exact_lowercase_commit_shas() {
        assert!(is_lowercase_commit_sha(
            "0123456789abcdef0123456789abcdef01234567"
        ));
        assert!(!is_lowercase_commit_sha(
            "0123456789ABCDEF0123456789ABCDEF01234567"
        ));
        assert!(!is_lowercase_commit_sha(
            "0123456789abcdef0123456789abcdef0123456"
        ));
        assert!(!is_lowercase_commit_sha(
            "g123456789abcdef0123456789abcdef01234567"
        ));
    }

    #[test]
    fn retained_capacity_counts_all_five_owned_identity_strings() {
        fn allocated(capacity: usize, value: &str) -> String {
            let mut output = String::with_capacity(capacity);
            output.push_str(value);
            output
        }

        let identity = ProductBuildIdentity::from_owned_memory_authorized_parts(
            allocated(48, "engine"),
            allocated(64, "source"),
            allocated(80, "contract"),
            allocated(96, "supply"),
            allocated(112, "artifact"),
        );
        let expected = [
            identity.engine_build_id.capacity(),
            identity.source_commit.capacity(),
            identity.contract_schema_version.capacity(),
            identity.supply_semantics_id.capacity(),
            identity.artifact_schema_version.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |bytes, capacity| {
            bytes.checked_add(capacity as u128)
        });

        assert_eq!(identity.checked_retained_capacity_bytes(), expected);
    }

    fn is_compile_identity(value: &str) -> bool {
        value == UNVERIFIED_LOCAL_BUILD || is_lowercase_commit_sha(value)
    }
}
