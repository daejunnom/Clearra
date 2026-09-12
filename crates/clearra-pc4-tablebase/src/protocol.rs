use crate::manifest::{ArtifactDescriptor, Pc4ArtifactRole, Pc4RuleProfile, SnapshotIdentity};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RangeRequest {
    request_id: u64,
    snapshot: SnapshotIdentity,
    profile: Pc4RuleProfile,
    artifact: ArtifactDescriptor,
    offset: u64,
    length: u32,
}

impl RangeRequest {
    pub(crate) fn new(
        request_id: u64,
        snapshot: SnapshotIdentity,
        profile: Pc4RuleProfile,
        artifact: ArtifactDescriptor,
        offset: u64,
        length: u32,
    ) -> Self {
        debug_assert!(length > 0);
        Self {
            request_id,
            snapshot,
            profile,
            artifact,
            offset,
            length,
        }
    }

    pub const fn request_id(&self) -> u64 {
        self.request_id
    }

    pub const fn snapshot(&self) -> &SnapshotIdentity {
        &self.snapshot
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.profile
    }

    pub const fn artifact(&self) -> Pc4ArtifactRole {
        self.artifact.role()
    }

    pub const fn artifact_descriptor(&self) -> &ArtifactDescriptor {
        &self.artifact
    }

    pub const fn offset(&self) -> u64 {
        self.offset
    }

    pub const fn length(&self) -> u32 {
        self.length
    }

    pub const fn end_exclusive(&self) -> u64 {
        self.offset + self.length as u64
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeResponseKind {
    PartialContent,
    WholeContent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RangeResponse {
    pub request_id: u64,
    pub snapshot: SnapshotIdentity,
    pub profile: Pc4RuleProfile,
    pub artifact: Pc4ArtifactRole,
    pub artifact_content_identity: String,
    pub kind: RangeResponseKind,
    pub offset: u64,
    pub complete_length: u64,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeTransportFailure {
    Offline,
    RateLimited { retry_after_seconds: Option<u64> },
    Timeout,
    Unavailable,
}
