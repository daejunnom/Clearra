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

impl SnapshotIdentity {
    pub fn new(
        repository: impl Into<String>,
        revision: impl Into<String>,
        generation: impl Into<String>,
    ) -> Result<Self, ManifestError> {
        let repository = required_identity(repository.into(), "snapshot_repository_missing")?;
        let revision = required_identity(revision.into(), "snapshot_revision_missing")?;
        let generation = required_identity(generation.into(), "snapshot_generation_missing")?;
        if is_moving_revision(&revision) {
            return Err(ManifestError::MovingSnapshotRevision);
        }
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
    profiles: Vec<ProfileAvailability>,
}

impl DatasetSnapshotManifest {
    pub fn new(
        identity: SnapshotIdentity,
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
        Ok(Self { identity, profiles })
    }

    pub const fn identity(&self) -> &SnapshotIdentity {
        &self.identity
    }

    pub fn profile_availability(&self, profile: Pc4RuleProfile) -> &ProfileAvailability {
        self.profiles
            .iter()
            .find(|candidate| candidate.profile() == profile)
            .expect("validated manifest contains every PC4 profile")
    }

    pub fn activate(self) -> Result<ActivatedSnapshot, ActivationError> {
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
        Ok(ActivatedSnapshot {
            identity: self.identity,
            profiles,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivatedSnapshot {
    identity: SnapshotIdentity,
    profiles: Vec<Pc4ProfileManifest>,
}

impl ActivatedSnapshot {
    pub const fn identity(&self) -> &SnapshotIdentity {
        &self.identity
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
}

impl ActivationError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::UnsupportedProfile { .. } => "pc4_online_unsupported_profile",
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
    MovingSnapshotRevision,
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
            Self::MovingSnapshotRevision => "pc4_online_snapshot_revision_is_moving",
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

fn is_moving_revision(revision: &str) -> bool {
    let revision = revision.trim().to_ascii_lowercase();
    matches!(revision.as_str(), "main" | "master" | "latest" | "head")
        || revision.starts_with("refs/heads/")
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
        DatasetSnapshotManifest::new(
            SnapshotIdentity::new(
                "synthetic/repository",
                "immutable-revision-a",
                "generation-a",
            )
            .expect("synthetic identity"),
            profiles,
        )
        .expect("synthetic manifest")
        .activate()
        .expect("fully qualified synthetic snapshot")
    }

    fn role_name(role: Pc4ArtifactRole) -> &'static str {
        match role {
            Pc4ArtifactRole::FieldHashIndex => "field-hash-index",
            Pc4ArtifactRole::GraphOffsets => "graph-offsets",
            Pc4ArtifactRole::Graph => "graph",
        }
    }

    #[test]
    fn moving_revisions_and_arbitrary_paths_are_rejected() {
        assert_eq!(
            SnapshotIdentity::new("repository", "main", "generation"),
            Err(ManifestError::MovingSnapshotRevision)
        );
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
            SnapshotIdentity::new("repository", "immutable", "generation").expect("identity"),
            profiles,
        )
        .expect("staging manifest");
        assert_eq!(
            manifest.activate(),
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
            SnapshotIdentity::new(
                "synthetic/repository",
                "immutable-revision-b",
                "generation-b",
            )
            .expect("second identity"),
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
        .activate()
        .expect("second activation");
        assert_ne!(first.identity(), second.identity());
        assert_eq!(first.identity().revision(), "immutable-revision-a");
    }
}
