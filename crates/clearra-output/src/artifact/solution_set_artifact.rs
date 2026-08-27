use std::fmt;

use clearra_core_domain::solution::{
    NormalizedTilingSolutionKey, NormalizedTilingSolutionSetHasher,
    NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM, NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
};

use super::solution_comment_layout::SolutionArtifactAnnotation;

/// Frozen envelope schema used by the existing Compact-v1 and JSON-v1
/// encodings. Those byte contracts remain stable for backwards compatibility.
pub const SOLUTION_SET_ARTIFACT_SCHEMA_V1: &str = "solution-set-artifact.v1";
/// Native document publication contract. CTK3 and Fumen are typed document
/// encodings rather than v1 key-envelope serializations.
pub const SOLUTION_SET_ARTIFACT_SCHEMA_V2: &str = "solution-set-artifact.v2";
/// Compatibility name for callers that operate on the v1 key-envelope model.
pub const SOLUTION_SET_ARTIFACT_SCHEMA: &str = SOLUTION_SET_ARTIFACT_SCHEMA_V1;
pub(crate) const MAX_ARTIFACT_ENTRIES: usize = 1_048_576;
pub(crate) const MAX_ARTIFACT_KEY_BYTES: usize = 1 << 20;
const MAX_ID_BYTES: usize = 512;
const NORMALIZED_TILING_SOURCE_CONTRACT: &str = "normalized-tiling-set";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionArtifactEntry {
    key: String,
    annotation: SolutionArtifactAnnotation,
}

impl SolutionArtifactEntry {
    pub fn try_new(
        key: impl Into<String>,
        annotation: SolutionArtifactAnnotation,
    ) -> Result<Self, SolutionSetArtifactError> {
        let key = key.into();
        if key.is_empty()
            || key.len() > MAX_ARTIFACT_KEY_BYTES
            || key
                .bytes()
                .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n')
        {
            return Err(SolutionSetArtifactError::InvalidSolutionKey);
        }
        Ok(Self { key, annotation })
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn annotation(&self) -> &SolutionArtifactAnnotation {
        &self.annotation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionSetArtifact {
    source_solution_set_contract: String,
    normalized_key_algorithm: String,
    normalized_set_hash_algorithm: String,
    normalized_set_hash: String,
    entries: Vec<SolutionArtifactEntry>,
}

impl SolutionSetArtifact {
    pub fn try_new(
        source_solution_set_contract: impl Into<String>,
        normalized_key_algorithm: impl Into<String>,
        normalized_set_hash_algorithm: impl Into<String>,
        normalized_set_hash: impl Into<String>,
        expected_solution_count: usize,
        mut entries: Vec<SolutionArtifactEntry>,
    ) -> Result<Self, SolutionSetArtifactError> {
        if entries.len() > MAX_ARTIFACT_ENTRIES {
            return Err(SolutionSetArtifactError::EntryLimitExceeded);
        }
        if entries.len() != expected_solution_count {
            return Err(SolutionSetArtifactError::SolutionCountMismatch);
        }

        let source_solution_set_contract =
            validate_id(source_solution_set_contract.into(), "source contract")?;
        let normalized_key_algorithm =
            validate_id(normalized_key_algorithm.into(), "key algorithm")?;
        let normalized_set_hash_algorithm =
            validate_id(normalized_set_hash_algorithm.into(), "hash algorithm")?;
        let normalized_set_hash = validate_id(normalized_set_hash.into(), "set hash")?;

        entries.sort_unstable_by(|left, right| left.key.cmp(&right.key));
        if entries.windows(2).any(|pair| pair[0].key == pair[1].key) {
            return Err(SolutionSetArtifactError::DuplicateSolutionKey);
        }
        validate_canonical_identity(
            &source_solution_set_contract,
            &normalized_key_algorithm,
            &normalized_set_hash_algorithm,
            &normalized_set_hash,
            &entries,
        )?;

        Ok(Self {
            source_solution_set_contract,
            normalized_key_algorithm,
            normalized_set_hash_algorithm,
            normalized_set_hash,
            entries,
        })
    }

    pub const fn schema(&self) -> &'static str {
        SOLUTION_SET_ARTIFACT_SCHEMA
    }

    pub fn source_solution_set_contract(&self) -> &str {
        &self.source_solution_set_contract
    }

    pub fn normalized_key_algorithm(&self) -> &str {
        &self.normalized_key_algorithm
    }

    pub fn normalized_set_hash_algorithm(&self) -> &str {
        &self.normalized_set_hash_algorithm
    }

    pub fn normalized_set_hash(&self) -> &str {
        &self.normalized_set_hash
    }

    pub fn entries(&self) -> &[SolutionArtifactEntry] {
        &self.entries
    }

    pub fn solution_count(&self) -> usize {
        self.entries.len()
    }

    pub fn annotated_solution_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| !entry.annotation.is_empty())
            .count()
    }
}

fn validate_canonical_identity(
    source_contract: &str,
    key_algorithm: &str,
    set_hash_algorithm: &str,
    set_hash: &str,
    entries: &[SolutionArtifactEntry],
) -> Result<(), SolutionSetArtifactError> {
    let canonical_contract = source_contract == NORMALIZED_TILING_SOURCE_CONTRACT;
    let canonical_key_algorithm = key_algorithm == NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM;
    let canonical_hash_algorithm =
        set_hash_algorithm == NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM;
    if canonical_contract || canonical_key_algorithm || canonical_hash_algorithm {
        if !(canonical_contract && canonical_key_algorithm && canonical_hash_algorithm) {
            return Err(SolutionSetArtifactError::CanonicalContractMismatch);
        }
        let mut hasher = NormalizedTilingSolutionSetHasher::default();
        for entry in entries {
            let key = NormalizedTilingSolutionKey::parse_canonical(entry.key())
                .map_err(|_| SolutionSetArtifactError::InvalidCanonicalSolutionKey)?;
            hasher.update_canonical_key(&key);
        }
        if hasher.finish() != set_hash {
            return Err(SolutionSetArtifactError::CanonicalSetHashMismatch);
        }
    }
    Ok(())
}

fn validate_id(value: String, _field: &'static str) -> Result<String, SolutionSetArtifactError> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err(SolutionSetArtifactError::InvalidIdentityMetadata);
    }
    Ok(value)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolutionSetArtifactError {
    InvalidSolutionKey,
    DuplicateSolutionKey,
    EntryLimitExceeded,
    SolutionCountMismatch,
    InvalidIdentityMetadata,
    CanonicalContractMismatch,
    InvalidCanonicalSolutionKey,
    CanonicalSetHashMismatch,
    UnknownAnnotationKey,
    DuplicateAnnotationKey,
}

impl fmt::Display for SolutionSetArtifactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSolutionKey => "solution artifact key is invalid",
            Self::DuplicateSolutionKey => "solution artifact contains a duplicate key",
            Self::EntryLimitExceeded => "solution artifact entry limit is exceeded",
            Self::SolutionCountMismatch => "solution artifact count does not match its entries",
            Self::InvalidIdentityMetadata => "solution artifact identity metadata is invalid",
            Self::CanonicalContractMismatch => {
                "solution artifact canonical contract metadata does not match"
            }
            Self::InvalidCanonicalSolutionKey => {
                "solution artifact canonical solution key is invalid"
            }
            Self::CanonicalSetHashMismatch => {
                "solution artifact canonical set hash does not match its keys"
            }
            Self::UnknownAnnotationKey => "solution annotation refers to an unknown key",
            Self::DuplicateAnnotationKey => "solution annotation key is duplicated",
        })
    }
}

impl std::error::Error for SolutionSetArtifactError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str) -> SolutionArtifactEntry {
        SolutionArtifactEntry::try_new(key, SolutionArtifactAnnotation::new()).expect("entry")
    }

    #[test]
    fn construction_canonicalizes_set_order_but_rejects_duplicates() {
        let artifact = SolutionSetArtifact::try_new(
            "test-solution-set",
            "key-v1",
            "hash-v1",
            "hash:1234",
            2,
            vec![entry("solution-b"), entry("solution-a")],
        )
        .expect("artifact");
        assert_eq!(artifact.entries()[0].key(), "solution-a");
        assert_eq!(artifact.entries()[1].key(), "solution-b");

        assert_eq!(
            SolutionSetArtifact::try_new(
                "test-solution-set",
                "key-v1",
                "hash-v1",
                "hash:1234",
                2,
                vec![entry("solution-a"), entry("solution-a")],
            ),
            Err(SolutionSetArtifactError::DuplicateSolutionKey)
        );
    }

    #[test]
    fn empty_complete_set_is_representable_without_inventing_a_solution() {
        let artifact = SolutionSetArtifact::try_new(
            "test-solution-set",
            "key-v1",
            "hash-v1",
            "hash:empty",
            0,
            Vec::new(),
        )
        .expect("empty complete artifact");
        assert_eq!(artifact.solution_count(), 0);
    }

    #[test]
    fn canonical_contract_recomputes_key_validity_and_set_hash() {
        let key = "ctk1|initial=0000000000000000|placements=I:000000000000000f";
        let parsed = NormalizedTilingSolutionKey::parse_canonical(key).expect("canonical key");
        let mut hasher = NormalizedTilingSolutionSetHasher::default();
        hasher.update_canonical_key(&parsed);
        let hash = hasher.finish();
        let artifact = SolutionSetArtifact::try_new(
            NORMALIZED_TILING_SOURCE_CONTRACT,
            NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
            NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
            &hash,
            1,
            vec![entry(key)],
        )
        .expect("canonical artifact");
        assert_eq!(artifact.normalized_set_hash(), hash);

        assert_eq!(
            SolutionSetArtifact::try_new(
                NORMALIZED_TILING_SOURCE_CONTRACT,
                NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
                NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
                "cts1:0000000000000000",
                1,
                vec![entry(key)],
            ),
            Err(SolutionSetArtifactError::CanonicalSetHashMismatch)
        );
        assert_eq!(
            SolutionSetArtifact::try_new(
                NORMALIZED_TILING_SOURCE_CONTRACT,
                "parallel-key-v1",
                NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
                &hash,
                1,
                vec![entry(key)],
            ),
            Err(SolutionSetArtifactError::CanonicalContractMismatch)
        );
    }
}
