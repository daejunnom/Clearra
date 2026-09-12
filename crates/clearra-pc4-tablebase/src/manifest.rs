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

/// A terminal row target that can be represented inside the PC4 graph state
/// domain. Construction does not qualify that target for product use.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pc4TargetLines(u8);

impl Pc4TargetLines {
    pub const MIN: u8 = 1;
    pub const MAX: u8 = 4;

    pub fn new(value: u8) -> Result<Self, ManifestError> {
        if !(Self::MIN..=Self::MAX).contains(&value) {
            return Err(ManifestError::TargetLinesOutsideGraphDomain { actual: value });
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Product use case whose terminal/completeness semantics were qualified.
/// PC completion evidence must never authorize Setup traversal or vice versa.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Pc4TerminalUseCase {
    PcSearch,
    SetupSearch,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GraphTargetEncoding {
    U24LittleEndian,
    U32LittleEndian,
}

/// Qualified relationship between a graph field ID and the ordinal of its
/// `FHIDIDX1` record.
///
/// A hash-to-ID index can represent an arbitrary permutation. Reverse lookup
/// by field ID is safe only when qualification proves that IDs are record
/// ordinals. Clearra must not infer this property from a few sampled rows.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FieldIdIndexRelation {
    RecordOrdinal,
    ExplicitMappingOnly,
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

/// Independent completeness evidence for one profile and terminal target.
///
/// A qualified graph format alone does not prove that an early PC or Setup
/// terminal predicate is complete. Each enabled target binds its semantics,
/// full outgoing-edge statement, known answers, and exact offline parity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileTargetCompletenessQualification {
    use_case: Pc4TerminalUseCase,
    target_lines: Pc4TargetLines,
    terminal_semantics_identity: String,
    outgoing_edge_completeness_identity: String,
    known_answer_identity: String,
    offline_exact_parity_identity: String,
}

impl ProfileTargetCompletenessQualification {
    pub fn new(
        use_case: Pc4TerminalUseCase,
        target_lines: Pc4TargetLines,
        terminal_semantics_identity: impl Into<String>,
        outgoing_edge_completeness_identity: impl Into<String>,
        known_answer_identity: impl Into<String>,
        offline_exact_parity_identity: impl Into<String>,
    ) -> Result<Self, ManifestError> {
        Ok(Self {
            use_case,
            target_lines,
            terminal_semantics_identity: required_identity(
                terminal_semantics_identity.into(),
                "profile_target_terminal_semantics_identity_missing",
            )?,
            outgoing_edge_completeness_identity: required_identity(
                outgoing_edge_completeness_identity.into(),
                "profile_target_outgoing_completeness_identity_missing",
            )?,
            known_answer_identity: required_identity(
                known_answer_identity.into(),
                "profile_target_known_answer_identity_missing",
            )?,
            offline_exact_parity_identity: required_identity(
                offline_exact_parity_identity.into(),
                "profile_target_offline_exact_parity_identity_missing",
            )?,
        })
    }

    pub const fn use_case(&self) -> Pc4TerminalUseCase {
        self.use_case
    }

    pub const fn target_lines(&self) -> Pc4TargetLines {
        self.target_lines
    }

    pub fn terminal_semantics_identity(&self) -> &str {
        &self.terminal_semantics_identity
    }

    pub fn outgoing_edge_completeness_identity(&self) -> &str {
        &self.outgoing_edge_completeness_identity
    }

    pub fn known_answer_identity(&self) -> &str {
        &self.known_answer_identity
    }

    pub fn offline_exact_parity_identity(&self) -> &str {
        &self.offline_exact_parity_identity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4ProfileManifest {
    profile: Pc4RuleProfile,
    field_count: u32,
    graph_target_encoding: GraphTargetEncoding,
    field_id_index_relation: FieldIdIndexRelation,
    maximum_graph_record_bytes: u32,
    field_hash_index: ArtifactDescriptor,
    graph_offsets: ArtifactDescriptor,
    graph: ArtifactDescriptor,
    qualification: ProfileQualification,
    target_qualifications: Vec<ProfileTargetCompletenessQualification>,
}

impl Pc4ProfileManifest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        profile: Pc4RuleProfile,
        field_count: u32,
        graph_target_encoding: GraphTargetEncoding,
        field_id_index_relation: FieldIdIndexRelation,
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
            field_id_index_relation,
            maximum_graph_record_bytes,
            field_hash_index,
            graph_offsets,
            graph,
            qualification,
            target_qualifications: Vec::new(),
        })
    }

    /// Adds independently qualified terminal targets to this profile.
    ///
    /// An empty set keeps the snapshot reader usable but leaves every PC and
    /// Setup target feature-off. Duplicate target claims fail closed.
    pub fn with_target_qualifications(
        mut self,
        mut target_qualifications: Vec<ProfileTargetCompletenessQualification>,
    ) -> Result<Self, ManifestError> {
        target_qualifications.sort_unstable_by_key(|qualification| {
            (qualification.use_case(), qualification.target_lines())
        });
        if let Some(duplicate) = target_qualifications.windows(2).find(|pair| {
            pair[0].use_case() == pair[1].use_case()
                && pair[0].target_lines() == pair[1].target_lines()
        }) {
            return Err(ManifestError::DuplicateTargetQualification {
                profile: self.profile,
                use_case: duplicate[0].use_case(),
                target_lines: duplicate[0].target_lines(),
            });
        }
        self.target_qualifications = target_qualifications;
        Ok(self)
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

    pub const fn field_id_index_relation(&self) -> FieldIdIndexRelation {
        self.field_id_index_relation
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

    pub fn target_qualifications(&self) -> &[ProfileTargetCompletenessQualification] {
        &self.target_qualifications
    }

    pub fn target_qualification(
        &self,
        use_case: Pc4TerminalUseCase,
        target_lines: Pc4TargetLines,
    ) -> Option<&ProfileTargetCompletenessQualification> {
        self.target_qualifications
            .binary_search_by_key(&(use_case, target_lines), |qualification| {
                (qualification.use_case(), qualification.target_lines())
            })
            .ok()
            .map(|index| &self.target_qualifications[index])
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

    /// Mints a target-bound identity only when that exact profile/target
    /// completeness claim was part of the verified immutable manifest.
    pub fn qualified_target(
        &self,
        profile: Pc4RuleProfile,
        use_case: Pc4TerminalUseCase,
        target_lines: Pc4TargetLines,
    ) -> Result<QualifiedPc4TargetIdentity, TargetQualificationError> {
        let qualification = self
            .profile(profile)
            .target_qualification(use_case, target_lines)
            .ok_or(TargetQualificationError::Unavailable {
                profile,
                use_case,
                target_lines,
            })?;
        Ok(QualifiedPc4TargetIdentity {
            snapshot: self.qualified_identity.clone(),
            profile,
            use_case,
            qualification: qualification.clone(),
        })
    }
}

/// Snapshot-, profile-, and target-bound completeness authority.
///
/// Future candidate producers must require this identity rather than deriving
/// target completeness from a graph hit or a generic activated snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualifiedPc4TargetIdentity {
    snapshot: QualifiedSnapshotIdentity,
    profile: Pc4RuleProfile,
    use_case: Pc4TerminalUseCase,
    qualification: ProfileTargetCompletenessQualification,
}

impl QualifiedPc4TargetIdentity {
    pub const fn snapshot(&self) -> &QualifiedSnapshotIdentity {
        &self.snapshot
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.profile
    }

    pub const fn use_case(&self) -> Pc4TerminalUseCase {
        self.use_case
    }

    pub const fn target_lines(&self) -> Pc4TargetLines {
        self.qualification.target_lines()
    }

    pub const fn qualification(&self) -> &ProfileTargetCompletenessQualification {
        &self.qualification
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetQualificationError {
    Unavailable {
        profile: Pc4RuleProfile,
        use_case: Pc4TerminalUseCase,
        target_lines: Pc4TargetLines,
    },
}

impl TargetQualificationError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Unavailable { .. } => "pc4_online_target_completeness_unavailable",
        }
    }
}

impl fmt::Display for TargetQualificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for TargetQualificationError {}

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
    TargetLinesOutsideGraphDomain {
        actual: u8,
    },
    DuplicateTargetQualification {
        profile: Pc4RuleProfile,
        use_case: Pc4TerminalUseCase,
        target_lines: Pc4TargetLines,
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
            Self::TargetLinesOutsideGraphDomain { .. } => {
                "pc4_online_target_lines_outside_graph_domain"
            }
            Self::DuplicateTargetQualification { .. } => {
                "pc4_online_duplicate_target_qualification"
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
        qualified_profile_with_field_id_relation(
            profile,
            field_count,
            graph_bytes,
            encoding,
            FieldIdIndexRelation::RecordOrdinal,
        )
    }

    pub(crate) fn qualified_profile_with_field_id_relation(
        profile: Pc4RuleProfile,
        field_count: u32,
        graph_bytes: u64,
        encoding: GraphTargetEncoding,
        field_id_index_relation: FieldIdIndexRelation,
    ) -> Pc4ProfileManifest {
        Pc4ProfileManifest::new(
            profile,
            field_count,
            encoding,
            field_id_index_relation,
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

    pub(crate) fn activated_snapshot_with_field_id_relation(
        field_count: u32,
        graph_bytes: u64,
        field_id_index_relation: FieldIdIndexRelation,
    ) -> ActivatedSnapshot {
        let profiles = Pc4RuleProfile::ALL
            .into_iter()
            .map(|profile| {
                ProfileAvailability::qualified(qualified_profile_with_field_id_relation(
                    profile,
                    field_count,
                    graph_bytes,
                    if profile == Pc4RuleProfile::SrsX {
                        GraphTargetEncoding::U32LittleEndian
                    } else {
                        GraphTargetEncoding::U24LittleEndian
                    },
                    field_id_index_relation,
                ))
            })
            .collect();
        DatasetSnapshotManifest::new(
            SnapshotIdentity::new(
                "synthetic/repository",
                SYNTHETIC_REVISION_A,
                "generation-with-field-id-relation",
            )
            .expect("synthetic identity"),
            ManifestContentIdentity::new("synthetic-manifest-with-field-id-relation")
                .expect("synthetic manifest content identity"),
            profiles,
        )
        .expect("synthetic manifest")
        .activate(&mut SyntheticVerifier)
        .expect("fully qualified synthetic snapshot")
    }

    fn target_qualification(target_lines: u8) -> ProfileTargetCompletenessQualification {
        target_qualification_for(Pc4TerminalUseCase::PcSearch, target_lines)
    }

    fn target_qualification_for(
        use_case: Pc4TerminalUseCase,
        target_lines: u8,
    ) -> ProfileTargetCompletenessQualification {
        ProfileTargetCompletenessQualification::new(
            use_case,
            Pc4TargetLines::new(target_lines).expect("target inside graph domain"),
            format!("terminal:{use_case:?}:{target_lines}"),
            format!("outgoing-complete:{use_case:?}:{target_lines}"),
            format!("target-kat:{use_case:?}:{target_lines}"),
            format!("offline-parity:{use_case:?}:{target_lines}"),
        )
        .expect("synthetic profile-target qualification")
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

    pub(crate) fn activated_snapshot_for_generation(
        generation: impl Into<String>,
        manifest_content_identity: impl Into<String>,
    ) -> ActivatedSnapshot {
        activated_snapshot_for_revision_generation(
            SYNTHETIC_REVISION_A,
            generation,
            manifest_content_identity,
        )
    }

    pub(crate) fn activated_snapshot_for_revision_generation(
        revision: impl Into<String>,
        generation: impl Into<String>,
        manifest_content_identity: impl Into<String>,
    ) -> ActivatedSnapshot {
        qualified_manifest(
            SnapshotIdentity::new("synthetic/repository", revision, generation)
                .expect("synthetic identity"),
            ManifestContentIdentity::new(manifest_content_identity)
                .expect("synthetic manifest content identity"),
            2,
            8,
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

    pub(crate) fn qualified_target_identity(
        generation: impl Into<String>,
        manifest_content_identity: impl Into<String>,
        profile: Pc4RuleProfile,
        use_case: Pc4TerminalUseCase,
        target_lines: u8,
    ) -> QualifiedPc4TargetIdentity {
        let profiles = Pc4RuleProfile::ALL
            .into_iter()
            .map(|candidate_profile| {
                let manifest = qualified_profile(
                    candidate_profile,
                    2,
                    8,
                    GraphTargetEncoding::U24LittleEndian,
                );
                let manifest = if candidate_profile == profile {
                    manifest
                        .with_target_qualifications(vec![target_qualification_for(
                            use_case,
                            target_lines,
                        )])
                        .expect("synthetic target qualification")
                } else {
                    manifest
                };
                ProfileAvailability::qualified(manifest)
            })
            .collect();
        DatasetSnapshotManifest::new(
            SnapshotIdentity::new("synthetic/repository", SYNTHETIC_REVISION_A, generation)
                .expect("synthetic target snapshot identity"),
            ManifestContentIdentity::new(manifest_content_identity)
                .expect("synthetic target manifest content identity"),
            profiles,
        )
        .expect("synthetic target manifest")
        .activate(&mut SyntheticVerifier)
        .expect("synthetic target snapshot activation")
        .qualified_target(
            profile,
            use_case,
            Pc4TargetLines::new(target_lines).expect("target inside graph domain"),
        )
        .expect("synthetic qualified target")
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
    fn target_lines_are_bounded_to_the_one_through_four_row_graph_domain() {
        assert_eq!(Pc4TargetLines::new(1).expect("one row").get(), 1);
        assert_eq!(Pc4TargetLines::new(4).expect("four rows").get(), 4);
        for actual in [0, 5, u8::MAX] {
            assert_eq!(
                Pc4TargetLines::new(actual),
                Err(ManifestError::TargetLinesOutsideGraphDomain { actual })
            );
        }
    }

    #[test]
    fn profile_target_completeness_is_independent_and_duplicate_targets_fail_closed() {
        let target_one = target_qualification(1);
        let target_three = target_qualification(3);
        let setup_target_one = target_qualification_for(Pc4TerminalUseCase::SetupSearch, 1);
        let profile = qualified_profile(
            Pc4RuleProfile::SrsPlus,
            2,
            8,
            GraphTargetEncoding::U24LittleEndian,
        )
        .with_target_qualifications(vec![
            setup_target_one.clone(),
            target_three.clone(),
            target_one.clone(),
        ])
        .expect("distinct target qualifications");

        assert_eq!(
            profile
                .target_qualifications()
                .iter()
                .map(|qualification| {
                    (qualification.use_case(), qualification.target_lines().get())
                })
                .collect::<Vec<_>>(),
            vec![
                (Pc4TerminalUseCase::PcSearch, 1),
                (Pc4TerminalUseCase::PcSearch, 3),
                (Pc4TerminalUseCase::SetupSearch, 1),
            ]
        );
        assert_eq!(
            profile
                .target_qualification(
                    Pc4TerminalUseCase::PcSearch,
                    Pc4TargetLines::new(1).expect("one row"),
                )
                .expect("one-row target"),
            &target_one
        );
        assert!(profile
            .target_qualification(
                Pc4TerminalUseCase::PcSearch,
                Pc4TargetLines::new(2).expect("two rows"),
            )
            .is_none());
        assert_eq!(
            profile
                .target_qualification(
                    Pc4TerminalUseCase::SetupSearch,
                    Pc4TargetLines::new(1).expect("one row"),
                )
                .expect("separately qualified Setup target"),
            &setup_target_one
        );

        assert_eq!(
            qualified_profile(
                Pc4RuleProfile::SrsPlus,
                2,
                8,
                GraphTargetEncoding::U24LittleEndian,
            )
            .with_target_qualifications(vec![target_three.clone(), target_three]),
            Err(ManifestError::DuplicateTargetQualification {
                profile: Pc4RuleProfile::SrsPlus,
                use_case: Pc4TerminalUseCase::PcSearch,
                target_lines: Pc4TargetLines::new(3).expect("three rows"),
            })
        );
    }

    #[test]
    fn activated_snapshot_mints_only_manifest_qualified_profile_targets() {
        let profiles = Pc4RuleProfile::ALL
            .into_iter()
            .map(|profile| {
                let manifest =
                    qualified_profile(profile, 2, 8, GraphTargetEncoding::U24LittleEndian);
                let manifest = if profile == Pc4RuleProfile::SrsX {
                    manifest
                        .with_target_qualifications(vec![target_qualification(2)])
                        .expect("qualified two-row target")
                } else {
                    manifest
                };
                ProfileAvailability::qualified(manifest)
            })
            .collect();
        let snapshot = DatasetSnapshotManifest::new(
            SnapshotIdentity::new(
                "synthetic/repository",
                SYNTHETIC_REVISION_A,
                "generation-targets",
            )
            .expect("identity"),
            ManifestContentIdentity::new("synthetic-target-qualified-manifest")
                .expect("manifest content"),
            profiles,
        )
        .expect("manifest")
        .activate(&mut SyntheticVerifier)
        .expect("snapshot verification");

        let target_two = Pc4TargetLines::new(2).expect("two rows");
        let qualified = snapshot
            .qualified_target(
                Pc4RuleProfile::SrsX,
                Pc4TerminalUseCase::PcSearch,
                target_two,
            )
            .expect("qualified target identity");
        assert_eq!(qualified.snapshot(), snapshot.qualified_identity());
        assert_eq!(qualified.profile(), Pc4RuleProfile::SrsX);
        assert_eq!(qualified.use_case(), Pc4TerminalUseCase::PcSearch);
        assert_eq!(qualified.target_lines(), target_two);
        assert_eq!(
            qualified
                .qualification()
                .outgoing_edge_completeness_identity(),
            "outgoing-complete:PcSearch:2"
        );
        assert_eq!(
            snapshot.qualified_target(
                Pc4RuleProfile::SrsX,
                Pc4TerminalUseCase::PcSearch,
                Pc4TargetLines::new(1).expect("one"),
            ),
            Err(TargetQualificationError::Unavailable {
                profile: Pc4RuleProfile::SrsX,
                use_case: Pc4TerminalUseCase::PcSearch,
                target_lines: Pc4TargetLines::new(1).expect("one"),
            })
        );
        assert_eq!(
            snapshot.qualified_target(
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::PcSearch,
                target_two,
            ),
            Err(TargetQualificationError::Unavailable {
                profile: Pc4RuleProfile::Srs,
                use_case: Pc4TerminalUseCase::PcSearch,
                target_lines: target_two,
            })
        );
        assert_eq!(
            snapshot.qualified_target(
                Pc4RuleProfile::SrsX,
                Pc4TerminalUseCase::SetupSearch,
                target_two,
            ),
            Err(TargetQualificationError::Unavailable {
                profile: Pc4RuleProfile::SrsX,
                use_case: Pc4TerminalUseCase::SetupSearch,
                target_lines: target_two,
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
                FieldIdIndexRelation::RecordOrdinal,
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
