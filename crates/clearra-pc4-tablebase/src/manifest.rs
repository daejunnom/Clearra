// SRP rationale: this module has one behavior-level change reason: defining,
// validating, and activating one immutable PC4 dataset snapshot manifest and
// its exact range-index identities.

use core::fmt;

pub(crate) const INDEX_HEADER_BYTES: u64 = 16;
pub(crate) const FIELD_HASH_RECORD_BYTES: u64 = 8;
// GOFFIDX1 entries are byte offsets into the graph artifact. Target-word
// encoding belongs to the qualified graph-record parser and never scales these
// offsets.
pub(crate) const GRAPH_OFFSET_BYTES: u64 = 4;
pub(crate) const FIELD_HASH_INDEX_MAGIC: [u8; 8] = *b"FHIDIDX1";
pub(crate) const GRAPH_OFFSETS_MAGIC: [u8; 8] = *b"GOFFIDX1";
pub(crate) const RANGE_INDEX_VERSION: u32 = 1;
const MAX_U24: u64 = 0x00ff_ffff;
const MAX_GRAPH_RECORD_BYTES: u32 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Pc4RuleProfile {
    Srs,
    SrsPlus,
    SrsX,
    Jstris180,
    NoKick,
}

impl Pc4RuleProfile {
    pub const ALL: [Self; 5] = [
        Self::Srs,
        Self::SrsPlus,
        Self::SrsX,
        Self::Jstris180,
        Self::NoKick,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Srs => "srs",
            Self::SrsPlus => "srs-plus",
            Self::SrsX => "srs-x",
            Self::Jstris180 => "jstris-180",
            Self::NoKick => "no-kick",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GraphTargetEncoding {
    U24LittleEndian,
    U32LittleEndian,
}

impl GraphTargetEncoding {
    pub const fn byte_width(self) -> usize {
        match self {
            Self::U24LittleEndian => 3,
            Self::U32LittleEndian => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Pc4ArtifactRole {
    FieldHashIndex,
    GraphOffsets,
    Graph,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SnapshotIdentity {
    repository: String,
    revision: String,
    generation: String,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManifestContentIdentity(String);

impl ManifestContentIdentity {
    pub fn new(value: impl Into<String>) -> Result<Self, ManifestError> {
        Ok(Self(required_identity(
            value.into(),
            "manifest_content_identity_missing",
        )?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl SnapshotIdentity {
    pub fn new(
        repository: impl Into<String>,
        revision: impl Into<String>,
        generation: impl Into<String>,
    ) -> Result<Self, ManifestError> {
        let repository = required_identity(repository.into(), "snapshot_repository_missing")?;
        let mut revision = required_identity(revision.into(), "snapshot_revision_missing")?;
        let generation = required_identity(generation.into(), "snapshot_generation_missing")?;
        if !is_resolved_git_object_id(&revision) {
            return Err(ManifestError::UnresolvedSnapshotRevision);
        }
        revision.make_ascii_lowercase();
        Ok(Self {
            repository,
            revision,
            generation,
        })
    }

    pub fn repository(&self) -> &str {
        &self.repository
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactDescriptor {
    role: Pc4ArtifactRole,
    path: String,
    byte_len: u64,
    content_identity: String,
}

impl ArtifactDescriptor {
    pub fn new(
        role: Pc4ArtifactRole,
        path: impl Into<String>,
        byte_len: u64,
        content_identity: impl Into<String>,
    ) -> Result<Self, ManifestError> {
        let path = path.into();
        if !valid_relative_artifact_path(&path) {
            return Err(ManifestError::InvalidArtifactPath);
        }
        if byte_len == 0 {
            return Err(ManifestError::EmptyArtifact { role });
        }
        let content_identity =
            required_identity(content_identity.into(), "artifact_content_identity_missing")?;
        Ok(Self {
            role,
            path,
            byte_len,
            content_identity,
        })
    }

    pub const fn role(&self) -> Pc4ArtifactRole {
        self.role
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }

    pub fn content_identity(&self) -> &str {
        &self.content_identity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileQualification {
    index_spec_identity: String,
    graph_spec_identity: String,
    provenance_identity: String,
    known_answer_identity: String,
}

impl ProfileQualification {
    pub fn new(
        index_spec_identity: impl Into<String>,
        graph_spec_identity: impl Into<String>,
        provenance_identity: impl Into<String>,
        known_answer_identity: impl Into<String>,
    ) -> Result<Self, ManifestError> {
        Ok(Self {
            index_spec_identity: required_identity(
                index_spec_identity.into(),
                "profile_index_spec_identity_missing",
            )?,
            graph_spec_identity: required_identity(
                graph_spec_identity.into(),
                "profile_graph_spec_identity_missing",
            )?,
            provenance_identity: required_identity(
                provenance_identity.into(),
                "profile_provenance_identity_missing",
            )?,
            known_answer_identity: required_identity(
                known_answer_identity.into(),
                "profile_known_answer_identity_missing",
            )?,
        })
    }

    pub fn index_spec_identity(&self) -> &str {
        &self.index_spec_identity
    }

    pub fn graph_spec_identity(&self) -> &str {
        &self.graph_spec_identity
    }

    pub fn provenance_identity(&self) -> &str {
        &self.provenance_identity
    }

    pub fn known_answer_identity(&self) -> &str {
        &self.known_answer_identity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4ProfileManifest {
    profile: Pc4RuleProfile,
    field_count: u32,
    graph_target_encoding: GraphTargetEncoding,
    maximum_graph_record_bytes: u32,
    field_hash_index: ArtifactDescriptor,
    graph_offsets: ArtifactDescriptor,
    graph: ArtifactDescriptor,
    qualification: ProfileQualification,
}

impl Pc4ProfileManifest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        profile: Pc4RuleProfile,
        field_count: u32,
        graph_target_encoding: GraphTargetEncoding,
        maximum_graph_record_bytes: u32,
        field_hash_index: ArtifactDescriptor,
        graph_offsets: ArtifactDescriptor,
        graph: ArtifactDescriptor,
        qualification: ProfileQualification,
    ) -> Result<Self, ManifestError> {
        if field_count == 0 || u64::from(field_count) > MAX_U24 + 1 {
            return Err(ManifestError::InvalidFieldCount { profile });
        }
        if maximum_graph_record_bytes == 0 || maximum_graph_record_bytes > MAX_GRAPH_RECORD_BYTES {
            return Err(ManifestError::InvalidGraphRecordLimit { profile });
        }
        require_role(&field_hash_index, Pc4ArtifactRole::FieldHashIndex, profile)?;
        require_role(&graph_offsets, Pc4ArtifactRole::GraphOffsets, profile)?;
        require_role(&graph, Pc4ArtifactRole::Graph, profile)?;

        let expected_field_index_bytes = INDEX_HEADER_BYTES
            .checked_add(u64::from(field_count) * FIELD_HASH_RECORD_BYTES)
            .ok_or(ManifestError::ArtifactLengthOverflow { profile })?;
        if field_hash_index.byte_len() != expected_field_index_bytes {
            return Err(ManifestError::IndexLengthMismatch {
                profile,
                role: Pc4ArtifactRole::FieldHashIndex,
            });
        }
        let expected_offsets_bytes = INDEX_HEADER_BYTES
            .checked_add((u64::from(field_count) + 1) * GRAPH_OFFSET_BYTES)
            .ok_or(ManifestError::ArtifactLengthOverflow { profile })?;
        if graph_offsets.byte_len() != expected_offsets_bytes {
            return Err(ManifestError::IndexLengthMismatch {
                profile,
                role: Pc4ArtifactRole::GraphOffsets,
            });
        }
        if graph.byte_len() > u64::from(u32::MAX) {
            return Err(ManifestError::GraphTooLargeForOffsetContract { profile });
        }
        Ok(Self {
            profile,
            field_count,
            graph_target_encoding,
            maximum_graph_record_bytes,
            field_hash_index,
            graph_offsets,
            graph,
            qualification,
        })
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.profile
    }

    pub const fn field_count(&self) -> u32 {
        self.field_count
    }

    pub const fn graph_target_encoding(&self) -> GraphTargetEncoding {
        self.graph_target_encoding
    }

    pub const fn maximum_graph_record_bytes(&self) -> u32 {
        self.maximum_graph_record_bytes
    }

    pub const fn field_hash_index(&self) -> &ArtifactDescriptor {
        &self.field_hash_index
    }

    pub const fn graph_offsets(&self) -> &ArtifactDescriptor {
        &self.graph_offsets
    }

    pub const fn graph(&self) -> &ArtifactDescriptor {
        &self.graph
    }

    pub const fn qualification(&self) -> &ProfileQualification {
        &self.qualification
    }

    pub(crate) const fn artifact(&self, role: Pc4ArtifactRole) -> &ArtifactDescriptor {
        match role {
            Pc4ArtifactRole::FieldHashIndex => &self.field_hash_index,
            Pc4ArtifactRole::GraphOffsets => &self.graph_offsets,
            Pc4ArtifactRole::Graph => &self.graph,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsupportedProfileReason {
    MissingProfileArtifacts,
    MissingProfileSpecificIndex,
    MissingFormatSpecification,
    MissingProvenance,
    MissingKnownAnswers,
    UnknownGraphEncoding,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileAvailability {
    Qualified(Box<Pc4ProfileManifest>),
    Unsupported {
        profile: Pc4RuleProfile,
        reason: UnsupportedProfileReason,
    },
}

impl ProfileAvailability {
    pub fn qualified(manifest: Pc4ProfileManifest) -> Self {
        Self::Qualified(Box::new(manifest))
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        match self {
            Self::Qualified(manifest) => manifest.profile(),
            Self::Unsupported { profile, .. } => *profile,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatasetSnapshotManifest {
    identity: SnapshotIdentity,
    content_identity: ManifestContentIdentity,
    profiles: Vec<ProfileAvailability>,
}

impl DatasetSnapshotManifest {
    pub fn new(
        identity: SnapshotIdentity,
        content_identity: ManifestContentIdentity,
        profiles: Vec<ProfileAvailability>,
    ) -> Result<Self, ManifestError> {
        if profiles.len() != Pc4RuleProfile::ALL.len() {
            return Err(ManifestError::ProfileSetIncomplete);
        }
        for expected in Pc4RuleProfile::ALL {
            if profiles
                .iter()
                .filter(|candidate| candidate.profile() == expected)
                .count()
                != 1
            {
                return Err(ManifestError::ProfileSetIncomplete);
            }
        }
        Ok(Self {
            identity,
            content_identity,
            profiles,
        })
    }

    pub const fn identity(&self) -> &SnapshotIdentity {
        &self.identity
    }

    pub const fn content_identity(&self) -> &ManifestContentIdentity {
        &self.content_identity
    }

    pub fn profile_availability(&self, profile: Pc4RuleProfile) -> &ProfileAvailability {
        self.profiles
            .iter()
            .find(|candidate| candidate.profile() == profile)
            .expect("validated manifest contains every PC4 profile")
    }

    pub fn activate<V>(self, verifier: &mut V) -> Result<ActivatedSnapshot, ActivationError>
    where
        V: DatasetSnapshotVerifier + ?Sized,
    {
        let mut profiles = Vec::with_capacity(Pc4RuleProfile::ALL.len());
        for profile in Pc4RuleProfile::ALL {
            match self
                .profiles
                .iter()
                .find(|candidate| candidate.profile() == profile)
                .expect("validated manifest contains every PC4 profile")
            {
                ProfileAvailability::Qualified(manifest) => {
                    profiles.push(manifest.as_ref().clone());
                }
                ProfileAvailability::Unsupported { reason, .. } => {
                    return Err(ActivationError::UnsupportedProfile {
                        profile,
                        reason: *reason,
                    });
                }
            }
        }

        let attestation = verifier
            .verify(SnapshotVerificationRequest { manifest: &self })
            .map_err(|failure| match failure {
                SnapshotVerificationFailure::Rejected => ActivationError::VerificationRejected,
                SnapshotVerificationFailure::ProviderError => {
                    ActivationError::VerificationProviderError
                }
            })?;
        if attestation.snapshot_identity() != &self.identity {
            return Err(ActivationError::VerificationBindingDrift {
                binding: SnapshotVerificationBinding::SnapshotIdentity,
            });
        }
        if attestation.manifest_content_identity() != &self.content_identity {
            return Err(ActivationError::VerificationBindingDrift {
                binding: SnapshotVerificationBinding::ManifestContentIdentity,
            });
        }

        Ok(ActivatedSnapshot {
            qualified_identity: QualifiedSnapshotIdentity::from_verified_attestation(attestation),
            profiles,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SnapshotVerificationRequest<'a> {
    manifest: &'a DatasetSnapshotManifest,
}

impl<'a> SnapshotVerificationRequest<'a> {
    pub const fn snapshot_identity(self) -> &'a SnapshotIdentity {
        self.manifest.identity()
    }

    pub const fn manifest_content_identity(self) -> &'a ManifestContentIdentity {
        self.manifest.content_identity()
    }

    pub fn profile_availability(self, profile: Pc4RuleProfile) -> &'a ProfileAvailability {
        self.manifest.profile_availability(profile)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SnapshotVerificationAttestation {
    snapshot_identity: SnapshotIdentity,
    manifest_content_identity: ManifestContentIdentity,
    evidence_identity: String,
}

impl SnapshotVerificationAttestation {
    pub fn new(
        snapshot_identity: SnapshotIdentity,
        manifest_content_identity: ManifestContentIdentity,
        evidence_identity: impl Into<String>,
    ) -> Result<Self, ManifestError> {
        Ok(Self {
            snapshot_identity,
            manifest_content_identity,
            evidence_identity: required_identity(
                evidence_identity.into(),
                "snapshot_verification_evidence_identity_missing",
            )?,
        })
    }

    pub const fn snapshot_identity(&self) -> &SnapshotIdentity {
        &self.snapshot_identity
    }

    pub const fn manifest_content_identity(&self) -> &ManifestContentIdentity {
        &self.manifest_content_identity
    }

    pub fn evidence_identity(&self) -> &str {
        &self.evidence_identity
    }
}

/// Nominal identity minted only after the host verifier accepts an exact
/// snapshot and manifest-content binding.
///
/// There is deliberately no public constructor or conversion from the two raw
/// labels. Runtime lookup, traversal, and materialization boundaries can only
/// obtain this value from an [`ActivatedSnapshot`] (or clone an already
/// qualified value), so manifest provenance cannot be discarded after
/// activation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct QualifiedSnapshotIdentity {
    attestation: SnapshotVerificationAttestation,
}

impl QualifiedSnapshotIdentity {
    fn from_verified_attestation(attestation: SnapshotVerificationAttestation) -> Self {
        Self { attestation }
    }

    pub const fn snapshot_identity(&self) -> &SnapshotIdentity {
        self.attestation.snapshot_identity()
    }

    pub const fn manifest_content_identity(&self) -> &ManifestContentIdentity {
        self.attestation.manifest_content_identity()
    }

    pub const fn verification_attestation(&self) -> &SnapshotVerificationAttestation {
        &self.attestation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotVerificationFailure {
    Rejected,
    ProviderError,
}

pub trait DatasetSnapshotVerifier {
    fn verify(
        &mut self,
        request: SnapshotVerificationRequest<'_>,
    ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivatedSnapshot {
    qualified_identity: QualifiedSnapshotIdentity,
    profiles: Vec<Pc4ProfileManifest>,
}

impl ActivatedSnapshot {
    pub const fn identity(&self) -> &SnapshotIdentity {
        self.qualified_identity.snapshot_identity()
    }

    pub const fn manifest_content_identity(&self) -> &ManifestContentIdentity {
        self.qualified_identity.manifest_content_identity()
    }

    pub const fn verification_attestation(&self) -> &SnapshotVerificationAttestation {
        self.qualified_identity.verification_attestation()
    }

    pub const fn qualified_identity(&self) -> &QualifiedSnapshotIdentity {
        &self.qualified_identity
    }

    pub fn profile(&self, profile: Pc4RuleProfile) -> &Pc4ProfileManifest {
        self.profiles
            .iter()
            .find(|candidate| candidate.profile() == profile)
            .expect("activated snapshot contains every PC4 profile")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivationError {
    UnsupportedProfile {
        profile: Pc4RuleProfile,
        reason: UnsupportedProfileReason,
    },
    VerificationRejected,
    VerificationProviderError,
    VerificationBindingDrift {
        binding: SnapshotVerificationBinding,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotVerificationBinding {
    SnapshotIdentity,
    ManifestContentIdentity,
}

impl ActivationError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::UnsupportedProfile { .. } => "pc4_online_unsupported_profile",
            Self::VerificationRejected => "pc4_online_snapshot_verification_rejected",
            Self::VerificationProviderError => "pc4_online_snapshot_verification_provider_error",
            Self::VerificationBindingDrift { .. } => {
                "pc4_online_snapshot_verification_binding_drift"
            }
        }
    }
}

impl fmt::Display for ActivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for ActivationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManifestError {
    EmptyIdentity(&'static str),
    UnresolvedSnapshotRevision,
    InvalidArtifactPath,
    EmptyArtifact {
        role: Pc4ArtifactRole,
    },
    ArtifactRoleMismatch {
        profile: Pc4RuleProfile,
        expected: Pc4ArtifactRole,
        actual: Pc4ArtifactRole,
    },
    InvalidFieldCount {
        profile: Pc4RuleProfile,
    },
    InvalidGraphRecordLimit {
        profile: Pc4RuleProfile,
    },
    ArtifactLengthOverflow {
        profile: Pc4RuleProfile,
    },
    IndexLengthMismatch {
        profile: Pc4RuleProfile,
        role: Pc4ArtifactRole,
    },
    GraphTooLargeForOffsetContract {
        profile: Pc4RuleProfile,
    },
    ProfileSetIncomplete,
}

impl ManifestError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::EmptyIdentity(reason) => reason,
            Self::UnresolvedSnapshotRevision => {
                "pc4_online_snapshot_revision_is_not_resolved_object_id"
            }
            Self::InvalidArtifactPath => "pc4_online_artifact_path_invalid",
            Self::EmptyArtifact { .. } => "pc4_online_artifact_empty",
            Self::ArtifactRoleMismatch { .. } => "pc4_online_artifact_role_mismatch",
            Self::InvalidFieldCount { .. } => "pc4_online_field_count_invalid",
            Self::InvalidGraphRecordLimit { .. } => "pc4_online_graph_record_limit_invalid",
            Self::ArtifactLengthOverflow { .. } => "pc4_online_artifact_length_overflow",
            Self::IndexLengthMismatch { .. } => "pc4_online_index_length_mismatch",
            Self::GraphTooLargeForOffsetContract { .. } => {
                "pc4_online_graph_offset_contract_overflow"
            }
            Self::ProfileSetIncomplete => "pc4_online_profile_set_incomplete",
        }
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for ManifestError {}

fn required_identity(value: String, reason: &'static str) -> Result<String, ManifestError> {
    if value.trim().is_empty() {
        Err(ManifestError::EmptyIdentity(reason))
    } else {
        Ok(value)
    }
}

fn is_resolved_git_object_id(revision: &str) -> bool {
    matches!(revision.len(), 40 | 64) && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_relative_artifact_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains('\\')
        && !path.contains('?')
        && !path.contains('#')
        && !path.contains("://")
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
}

fn require_role(
    artifact: &ArtifactDescriptor,
    expected: Pc4ArtifactRole,
    profile: Pc4RuleProfile,
) -> Result<(), ManifestError> {
    if artifact.role() != expected {
        return Err(ManifestError::ArtifactRoleMismatch {
            profile,
            expected,
            actual: artifact.role(),
        });
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const SYNTHETIC_REVISION_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    pub(crate) const SYNTHETIC_REVISION_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const SYNTHETIC_REVISION_C: &str = "cccccccccccccccccccccccccccccccccccccccc";
    const SYNTHETIC_REVISION_D: &str = "dddddddddddddddddddddddddddddddddddddddd";
    const SYNTHETIC_REVISION_E: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    const SYNTHETIC_REVISION_SHA256: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    pub(crate) struct SyntheticVerifier;

    impl DatasetSnapshotVerifier for SyntheticVerifier {
        fn verify(
            &mut self,
            request: SnapshotVerificationRequest<'_>,
        ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure> {
            Ok(SnapshotVerificationAttestation::new(
                request.snapshot_identity().clone(),
                request.manifest_content_identity().clone(),
                "synthetic-verification-evidence",
            )
            .expect("synthetic verification evidence identity"))
        }
    }

    #[derive(Clone, Copy)]
    enum VerificationMode {
        Accept,
        Reject,
        ProviderError,
        DriftSnapshot,
        DriftSnapshotRevision,
        DriftManifestContent,
    }

    struct ControlledVerifier {
        mode: VerificationMode,
        calls: usize,
    }

    impl ControlledVerifier {
        const fn new(mode: VerificationMode) -> Self {
            Self { mode, calls: 0 }
        }
    }

    impl DatasetSnapshotVerifier for ControlledVerifier {
        fn verify(
            &mut self,
            request: SnapshotVerificationRequest<'_>,
        ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure> {
            self.calls += 1;
            match self.mode {
                VerificationMode::Reject => Err(SnapshotVerificationFailure::Rejected),
                VerificationMode::ProviderError => Err(SnapshotVerificationFailure::ProviderError),
                VerificationMode::Accept
                | VerificationMode::DriftSnapshot
                | VerificationMode::DriftSnapshotRevision
                | VerificationMode::DriftManifestContent => {
                    for profile in Pc4RuleProfile::ALL {
                        assert!(matches!(
                            request.profile_availability(profile),
                            ProfileAvailability::Qualified(_)
                        ));
                    }
                    let snapshot_identity = match self.mode {
                        VerificationMode::DriftSnapshot => SnapshotIdentity::new(
                            "synthetic/other-repository",
                            SYNTHETIC_REVISION_B,
                            "other-generation",
                        )
                        .expect("drift identity"),
                        VerificationMode::DriftSnapshotRevision => SnapshotIdentity::new(
                            request.snapshot_identity().repository(),
                            SYNTHETIC_REVISION_B,
                            request.snapshot_identity().generation(),
                        )
                        .expect("revision drift identity"),
                        VerificationMode::Accept
                        | VerificationMode::Reject
                        | VerificationMode::ProviderError
                        | VerificationMode::DriftManifestContent => {
                            request.snapshot_identity().clone()
                        }
                    };
                    let manifest_content_identity =
                        if matches!(self.mode, VerificationMode::DriftManifestContent) {
                            ManifestContentIdentity::new("synthetic-other-manifest-content")
                                .expect("drift manifest content identity")
                        } else {
                            request.manifest_content_identity().clone()
                        };
                    Ok(SnapshotVerificationAttestation::new(
                        snapshot_identity,
                        manifest_content_identity,
                        "synthetic-controlled-verification",
                    )
                    .expect("controlled verification attestation"))
                }
            }
        }
    }

    pub(crate) fn descriptor(
        profile: Pc4RuleProfile,
        role: Pc4ArtifactRole,
        byte_len: u64,
    ) -> ArtifactDescriptor {
        ArtifactDescriptor::new(
            role,
            format!("{}/{}.bin", profile.as_str(), role_name(role)),
            byte_len,
            format!("oid:{}:{}", profile.as_str(), role_name(role)),
        )
        .expect("synthetic artifact descriptor")
    }

    pub(crate) fn qualified_profile(
        profile: Pc4RuleProfile,
        field_count: u32,
        graph_bytes: u64,
        encoding: GraphTargetEncoding,
    ) -> Pc4ProfileManifest {
        Pc4ProfileManifest::new(
            profile,
            field_count,
            encoding,
            1024,
            descriptor(
                profile,
                Pc4ArtifactRole::FieldHashIndex,
                INDEX_HEADER_BYTES + u64::from(field_count) * FIELD_HASH_RECORD_BYTES,
            ),
            descriptor(
                profile,
                Pc4ArtifactRole::GraphOffsets,
                INDEX_HEADER_BYTES + (u64::from(field_count) + 1) * GRAPH_OFFSET_BYTES,
            ),
            descriptor(profile, Pc4ArtifactRole::Graph, graph_bytes),
            ProfileQualification::new(
                format!("index-spec:{}", profile.as_str()),
                format!("graph-spec:{}", profile.as_str()),
                format!("provenance:{}", profile.as_str()),
                format!("kat:{}", profile.as_str()),
            )
            .expect("synthetic qualification"),
        )
        .expect("synthetic profile manifest")
    }

    pub(crate) fn activated_snapshot(field_count: u32, graph_bytes: u64) -> ActivatedSnapshot {
        activated_snapshot_with_manifest_content(
            field_count,
            graph_bytes,
            format!("synthetic-manifest:{field_count}:{graph_bytes}"),
        )
    }

    pub(crate) fn activated_snapshot_with_manifest_content(
        field_count: u32,
        graph_bytes: u64,
        manifest_content_identity: impl Into<String>,
    ) -> ActivatedSnapshot {
        qualified_manifest(
            SnapshotIdentity::new("synthetic/repository", SYNTHETIC_REVISION_A, "generation-a")
                .expect("synthetic identity"),
            ManifestContentIdentity::new(manifest_content_identity)
                .expect("synthetic manifest content identity"),
            field_count,
            graph_bytes,
        )
        .activate(&mut SyntheticVerifier)
        .expect("fully qualified synthetic snapshot")
    }

    pub(crate) fn qualified_snapshot_identity(
        generation: impl Into<String>,
        manifest_content_identity: impl Into<String>,
    ) -> QualifiedSnapshotIdentity {
        qualified_manifest(
            SnapshotIdentity::new("synthetic/repository", SYNTHETIC_REVISION_A, generation)
                .expect("synthetic snapshot identity"),
            ManifestContentIdentity::new(manifest_content_identity)
                .expect("synthetic manifest content identity"),
            2,
            8,
        )
        .activate(&mut SyntheticVerifier)
        .expect("synthetic snapshot activation")
        .qualified_identity()
        .clone()
    }

    fn qualified_manifest(
        identity: SnapshotIdentity,
        content_identity: ManifestContentIdentity,
        field_count: u32,
        graph_bytes: u64,
    ) -> DatasetSnapshotManifest {
        let profiles = Pc4RuleProfile::ALL
            .into_iter()
            .map(|profile| {
                ProfileAvailability::qualified(qualified_profile(
                    profile,
                    field_count,
                    graph_bytes,
                    if profile == Pc4RuleProfile::SrsX {
                        GraphTargetEncoding::U32LittleEndian
                    } else {
                        GraphTargetEncoding::U24LittleEndian
                    },
                ))
            })
            .collect();
        DatasetSnapshotManifest::new(identity, content_identity, profiles)
            .expect("synthetic manifest")
    }

    fn role_name(role: Pc4ArtifactRole) -> &'static str {
        match role {
            Pc4ArtifactRole::FieldHashIndex => "field-hash-index",
            Pc4ArtifactRole::GraphOffsets => "graph-offsets",
            Pc4ArtifactRole::Graph => "graph",
        }
    }

    #[test]
    fn snapshot_revision_requires_an_exact_resolved_git_object_id() {
        for mutable_or_unresolved in [
            "main",
            "master",
            "latest",
            "HEAD",
            "refs/heads/main",
            "refs/tags/v0.9.0",
            "v0.9.0",
            "abc123",
            "gggggggggggggggggggggggggggggggggggggggg",
        ] {
            assert_eq!(
                SnapshotIdentity::new("repository", mutable_or_unresolved, "generation"),
                Err(ManifestError::UnresolvedSnapshotRevision),
                "{mutable_or_unresolved}"
            );
        }
        assert_eq!(
            SnapshotIdentity::new("repository", SYNTHETIC_REVISION_A, "generation")
                .expect("SHA-1 object identity")
                .revision(),
            SYNTHETIC_REVISION_A
        );
        assert_eq!(
            SnapshotIdentity::new("repository", SYNTHETIC_REVISION_SHA256, "generation")
                .expect("SHA-256 object identity")
                .revision(),
            SYNTHETIC_REVISION_SHA256
        );
    }

    #[test]
    fn snapshot_revision_has_one_lowercase_canonical_form() {
        for lowercase in [SYNTHETIC_REVISION_A, SYNTHETIC_REVISION_SHA256] {
            let uppercase = lowercase.to_ascii_uppercase();
            let canonical =
                SnapshotIdentity::new("repository", lowercase, "generation").expect("lowercase");
            let normalized = SnapshotIdentity::new("repository", uppercase, "generation")
                .expect("uppercase resolved object identity");

            assert_eq!(normalized, canonical);
            assert_eq!(normalized.revision(), lowercase);
        }
    }

    #[test]
    fn arbitrary_artifact_paths_are_rejected() {
        assert_eq!(
            ArtifactDescriptor::new(Pc4ArtifactRole::Graph, "../graph.bin", 1, "oid"),
            Err(ManifestError::InvalidArtifactPath)
        );
        assert_eq!(
            ArtifactDescriptor::new(
                Pc4ArtifactRole::Graph,
                "https://example.test/graph.bin",
                1,
                "oid"
            ),
            Err(ManifestError::InvalidArtifactPath)
        );
    }

    #[test]
    fn production_activation_requires_every_profile_to_be_qualified() {
        let mut profiles: Vec<_> = Pc4RuleProfile::ALL
            .into_iter()
            .map(|profile| {
                ProfileAvailability::qualified(qualified_profile(
                    profile,
                    2,
                    8,
                    GraphTargetEncoding::U24LittleEndian,
                ))
            })
            .collect();
        profiles[2] = ProfileAvailability::Unsupported {
            profile: Pc4RuleProfile::SrsX,
            reason: UnsupportedProfileReason::MissingProfileSpecificIndex,
        };
        let manifest = DatasetSnapshotManifest::new(
            SnapshotIdentity::new("repository", SYNTHETIC_REVISION_A, "generation")
                .expect("identity"),
            ManifestContentIdentity::new("synthetic-incomplete-manifest")
                .expect("manifest content identity"),
            profiles,
        )
        .expect("staging manifest");
        assert_eq!(
            manifest.activate(&mut SyntheticVerifier),
            Err(ActivationError::UnsupportedProfile {
                profile: Pc4RuleProfile::SrsX,
                reason: UnsupportedProfileReason::MissingProfileSpecificIndex,
            })
        );
    }

    #[test]
    fn profile_manifest_rejects_index_size_drift() {
        let profile = Pc4RuleProfile::Srs;
        let qualification = || {
            ProfileQualification::new("index-spec", "graph-spec", "provenance", "kat")
                .expect("qualification")
        };
        assert_eq!(
            Pc4ProfileManifest::new(
                profile,
                2,
                GraphTargetEncoding::U24LittleEndian,
                1024,
                descriptor(
                    profile,
                    Pc4ArtifactRole::FieldHashIndex,
                    INDEX_HEADER_BYTES + FIELD_HASH_RECORD_BYTES,
                ),
                descriptor(
                    profile,
                    Pc4ArtifactRole::GraphOffsets,
                    INDEX_HEADER_BYTES + 3 * GRAPH_OFFSET_BYTES,
                ),
                descriptor(profile, Pc4ArtifactRole::Graph, 6),
                qualification(),
            ),
            Err(ManifestError::IndexLengthMismatch {
                profile,
                role: Pc4ArtifactRole::FieldHashIndex,
            })
        );
    }

    #[test]
    fn snapshot_identity_is_dynamic_but_immutable_after_activation() {
        let first = activated_snapshot(2, 8);
        let second = DatasetSnapshotManifest::new(
            SnapshotIdentity::new("synthetic/repository", SYNTHETIC_REVISION_B, "generation-b")
                .expect("second identity"),
            ManifestContentIdentity::new("synthetic-second-manifest")
                .expect("second manifest content identity"),
            Pc4RuleProfile::ALL
                .into_iter()
                .map(|profile| {
                    ProfileAvailability::qualified(qualified_profile(
                        profile,
                        2,
                        8,
                        GraphTargetEncoding::U24LittleEndian,
                    ))
                })
                .collect(),
        )
        .expect("second manifest")
        .activate(&mut SyntheticVerifier)
        .expect("second activation");
        assert_ne!(first.identity(), second.identity());
        assert_eq!(first.identity().revision(), SYNTHETIC_REVISION_A);
    }

    #[test]
    fn activation_preserves_exact_verified_snapshot_and_manifest_bindings() {
        let manifest = qualified_manifest(
            SnapshotIdentity::new(
                "synthetic/repository",
                SYNTHETIC_REVISION_C,
                "generation-verified",
            )
            .expect("snapshot identity"),
            ManifestContentIdentity::new("synthetic-manifest-content-verified")
                .expect("manifest content identity"),
            2,
            8,
        );
        let mut verifier = ControlledVerifier::new(VerificationMode::Accept);

        let activated = manifest
            .activate(&mut verifier)
            .expect("accepted exact binding activates");

        assert_eq!(verifier.calls, 1);
        assert_eq!(activated.identity().generation(), "generation-verified");
        assert_eq!(
            activated.manifest_content_identity().as_str(),
            "synthetic-manifest-content-verified"
        );
        assert_eq!(
            activated.verification_attestation().evidence_identity(),
            "synthetic-controlled-verification"
        );
    }

    #[test]
    fn verifier_rejection_and_provider_error_fail_closed() {
        for (mode, expected) in [
            (
                VerificationMode::Reject,
                ActivationError::VerificationRejected,
            ),
            (
                VerificationMode::ProviderError,
                ActivationError::VerificationProviderError,
            ),
        ] {
            let manifest = qualified_manifest(
                SnapshotIdentity::new(
                    "synthetic/repository",
                    SYNTHETIC_REVISION_D,
                    "generation-failure",
                )
                .expect("snapshot identity"),
                ManifestContentIdentity::new("synthetic-manifest-content-failure")
                    .expect("manifest content identity"),
                2,
                8,
            );
            let mut verifier = ControlledVerifier::new(mode);

            assert_eq!(manifest.activate(&mut verifier), Err(expected));
            assert_eq!(verifier.calls, 1);
        }
    }

    #[test]
    fn verifier_binding_drift_fails_closed() {
        for (mode, binding) in [
            (
                VerificationMode::DriftSnapshot,
                SnapshotVerificationBinding::SnapshotIdentity,
            ),
            (
                VerificationMode::DriftSnapshotRevision,
                SnapshotVerificationBinding::SnapshotIdentity,
            ),
            (
                VerificationMode::DriftManifestContent,
                SnapshotVerificationBinding::ManifestContentIdentity,
            ),
        ] {
            let manifest = qualified_manifest(
                SnapshotIdentity::new(
                    "synthetic/repository",
                    SYNTHETIC_REVISION_E,
                    "generation-binding",
                )
                .expect("snapshot identity"),
                ManifestContentIdentity::new("synthetic-manifest-content-binding")
                    .expect("manifest content identity"),
                2,
                8,
            );
            let mut verifier = ControlledVerifier::new(mode);

            assert_eq!(
                manifest.activate(&mut verifier),
                Err(ActivationError::VerificationBindingDrift { binding })
            );
            assert_eq!(verifier.calls, 1);
        }
    }
}
