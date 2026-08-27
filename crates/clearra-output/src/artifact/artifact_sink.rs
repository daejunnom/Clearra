use std::fmt;

use clearra_platform_fs::{
    DurabilityUncertainReason, FileIdentity, NativePublicationError, NativePublicationErrorCode,
    NeverCancelled, PublicationCheckpoint, PublicationControl, PublicationResidue,
};

use super::{
    solution_set_artifact::SolutionSetArtifact,
    solution_set_encoder::{
        ArtifactEncodingReceipt, ArtifactStreamVerifier, SolutionArtifactEncoder,
        SolutionArtifactEncoding, SolutionArtifactEncodingError, MAX_IN_MEMORY_ARTIFACT_BYTES,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactCommit {
    schema: &'static str,
    encoding: SolutionArtifactEncoding,
    byte_count: u64,
    checksum: String,
    uncompressed_bytes: u64,
    solution_count: usize,
    annotated_solution_count: usize,
    target_owned: bool,
    file_identity: Option<FileIdentity>,
}

impl ArtifactCommit {
    fn from_receipt(
        receipt: &ArtifactEncodingReceipt,
        target_owned: bool,
        file_identity: Option<FileIdentity>,
    ) -> Self {
        Self {
            schema: receipt.schema(),
            encoding: receipt.encoding(),
            byte_count: receipt.byte_count(),
            checksum: receipt.checksum().to_owned(),
            uncompressed_bytes: receipt.uncompressed_bytes(),
            solution_count: receipt.solution_count(),
            annotated_solution_count: receipt.annotated_solution_count(),
            target_owned,
            file_identity,
        }
    }

    pub fn from_memory_receipt(receipt: &ArtifactEncodingReceipt) -> Self {
        Self::from_receipt(receipt, false, None)
    }

    pub fn from_native_receipt(
        receipt: &ArtifactEncodingReceipt,
        file_identity: FileIdentity,
    ) -> Self {
        Self::from_receipt(receipt, true, Some(file_identity))
    }

    pub const fn schema(&self) -> &'static str {
        self.schema
    }

    pub const fn encoding(&self) -> SolutionArtifactEncoding {
        self.encoding
    }

    pub const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub fn checksum(&self) -> &str {
        &self.checksum
    }

    pub const fn uncompressed_bytes(&self) -> u64 {
        self.uncompressed_bytes
    }

    pub const fn solution_count(&self) -> usize {
        self.solution_count
    }

    pub const fn annotated_solution_count(&self) -> usize {
        self.annotated_solution_count
    }

    pub const fn target_owned(&self) -> bool {
        self.target_owned
    }

    pub const fn file_identity(&self) -> Option<&FileIdentity> {
        self.file_identity.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArtifactPublicationOutcome {
    Committed(ArtifactCommit),
    DurabilityUncertain {
        commit: ArtifactCommit,
        reason: DurabilityUncertainReason,
    },
}

impl ArtifactPublicationOutcome {
    pub const fn commit(&self) -> &ArtifactCommit {
        match self {
            Self::Committed(commit) | Self::DurabilityUncertain { commit, .. } => commit,
        }
    }

    pub const fn is_committed(&self) -> bool {
        matches!(self, Self::Committed(_))
    }
}

pub trait SolutionArtifactSink {
    fn publish(
        &mut self,
        encoder: &dyn SolutionArtifactEncoder,
        artifact: &SolutionSetArtifact,
        maximum_bytes: u64,
        control: &dyn PublicationControl,
    ) -> Result<ArtifactPublicationOutcome, ArtifactSinkError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryArtifactSink {
    maximum_bytes: u64,
    bytes: Vec<u8>,
    commit: Option<ArtifactCommit>,
}

impl MemoryArtifactSink {
    pub fn try_new(maximum_bytes: u64) -> Result<Self, ArtifactSinkError> {
        if maximum_bytes == 0 || maximum_bytes > MAX_IN_MEMORY_ARTIFACT_BYTES {
            return Err(ArtifactSinkError::new(
                ArtifactSinkErrorCode::CapacityExceeded,
            ));
        }
        usize::try_from(maximum_bytes)
            .map_err(|_| ArtifactSinkError::new(ArtifactSinkErrorCode::CapacityExceeded))?;
        Ok(Self {
            maximum_bytes,
            bytes: Vec::new(),
            commit: None,
        })
    }

    pub const fn maximum_bytes(&self) -> u64 {
        self.maximum_bytes
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub const fn commit(&self) -> Option<&ArtifactCommit> {
        self.commit.as_ref()
    }
}

impl SolutionArtifactSink for MemoryArtifactSink {
    fn publish(
        &mut self,
        encoder: &dyn SolutionArtifactEncoder,
        artifact: &SolutionSetArtifact,
        maximum_bytes: u64,
        control: &dyn PublicationControl,
    ) -> Result<ArtifactPublicationOutcome, ArtifactSinkError> {
        self.commit = None;
        self.bytes.clear();
        let effective_limit = maximum_bytes.min(self.maximum_bytes);
        let plan = encoder
            .measure_checked(artifact, effective_limit, control)
            .map_err(ArtifactSinkError::from_encoding)?;
        if plan.encoding() != encoder.encoding() {
            return Err(ArtifactSinkError::from_encoding(
                SolutionArtifactEncodingError::PlanMismatch,
            ));
        }
        if plan.byte_count() > effective_limit {
            return Err(ArtifactSinkError::from_encoding(
                SolutionArtifactEncodingError::CapacityExceeded,
            ));
        }
        if control.cancelled_at(PublicationCheckpoint::BeforeStage) {
            return Err(ArtifactSinkError::new(ArtifactSinkErrorCode::Cancelled));
        }
        let capacity = usize::try_from(plan.byte_count())
            .map_err(|_| ArtifactSinkError::new(ArtifactSinkErrorCode::CapacityExceeded))?;
        self.bytes
            .try_reserve_exact(capacity)
            .map_err(|_| ArtifactSinkError::new(ArtifactSinkErrorCode::AllocationFailed))?;
        let encoded = {
            let mut verifier =
                ArtifactStreamVerifier::new(&mut self.bytes, encoder.encoding(), plan.byte_count());
            match encoder.encode_into(artifact, &plan, &mut verifier, control) {
                Ok(receipt) => verifier.verify(&plan, &receipt).map(|()| receipt),
                Err(error) => Err(error),
            }
        };
        let receipt = match encoded {
            Ok(receipt) => receipt,
            Err(error) => {
                self.bytes.clear();
                return Err(ArtifactSinkError::from_encoding(error));
            }
        };
        if control.cancelled_at(PublicationCheckpoint::BeforeCommitBarrier) {
            self.bytes.clear();
            return Err(ArtifactSinkError::new(ArtifactSinkErrorCode::Cancelled));
        }
        let commit = ArtifactCommit::from_memory_receipt(&receipt);
        self.commit = Some(commit.clone());
        Ok(ArtifactPublicationOutcome::Committed(commit))
    }
}

impl MemoryArtifactSink {
    pub fn publish_uncancelled(
        &mut self,
        encoder: &dyn SolutionArtifactEncoder,
        artifact: &SolutionSetArtifact,
    ) -> Result<ArtifactPublicationOutcome, ArtifactSinkError> {
        self.publish(encoder, artifact, self.maximum_bytes, &NeverCancelled)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactSinkErrorCode {
    InvalidTarget,
    UnsupportedPlatform,
    UnsupportedFilesystem,
    ParentMissing,
    ParentNotDirectory,
    ReparsePointRejected,
    TargetExists,
    StagingFailed,
    EncodingFailed,
    CapacityExceeded,
    AllocationFailed,
    WriteFailed,
    SizeMismatch,
    Cancelled,
    PreCommitSyncFailed,
    PublicationFailed,
    CleanupFailed,
}

impl ArtifactSinkErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidTarget => "artifact-target-invalid",
            Self::UnsupportedPlatform => "artifact-platform-unsupported",
            Self::UnsupportedFilesystem => "artifact-filesystem-unsupported",
            Self::ParentMissing => "artifact-parent-missing",
            Self::ParentNotDirectory => "artifact-parent-not-directory",
            Self::ReparsePointRejected => "artifact-reparse-point-rejected",
            Self::TargetExists => "artifact-target-exists",
            Self::StagingFailed => "artifact-staging-failed",
            Self::EncodingFailed => "artifact-encoding-failed",
            Self::CapacityExceeded => "artifact-capacity-exceeded",
            Self::AllocationFailed => "artifact-allocation-failed",
            Self::WriteFailed => "artifact-write-failed",
            Self::SizeMismatch => "artifact-size-mismatch",
            Self::Cancelled => "artifact-cancelled",
            Self::PreCommitSyncFailed => "artifact-precommit-sync-failed",
            Self::PublicationFailed => "artifact-publication-failed",
            Self::CleanupFailed => "artifact-cleanup-failed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactSinkError {
    code: ArtifactSinkErrorCode,
    encoding_error: Option<SolutionArtifactEncodingError>,
    residue: PublicationResidue,
    raw_os_error: Option<i32>,
}

impl ArtifactSinkError {
    pub const fn new(code: ArtifactSinkErrorCode) -> Self {
        Self {
            code,
            encoding_error: None,
            residue: PublicationResidue::None,
            raw_os_error: None,
        }
    }

    pub(crate) fn from_native(error: NativePublicationError) -> Self {
        Self {
            code: map_native_code(error.code()),
            encoding_error: None,
            residue: error.residue().clone(),
            raw_os_error: error.raw_os_error(),
        }
    }

    pub(crate) fn from_encoding(error: SolutionArtifactEncodingError) -> Self {
        let code = match &error {
            SolutionArtifactEncodingError::CapacityExceeded => {
                ArtifactSinkErrorCode::CapacityExceeded
            }
            SolutionArtifactEncodingError::WriteFailed => ArtifactSinkErrorCode::WriteFailed,
            SolutionArtifactEncodingError::Cancelled => ArtifactSinkErrorCode::Cancelled,
            _ => ArtifactSinkErrorCode::EncodingFailed,
        };
        Self {
            code,
            encoding_error: Some(error),
            residue: PublicationResidue::None,
            raw_os_error: None,
        }
    }

    pub const fn code(&self) -> ArtifactSinkErrorCode {
        self.code
    }

    pub const fn residue(&self) -> &PublicationResidue {
        &self.residue
    }

    pub const fn encoding_error(&self) -> Option<&SolutionArtifactEncodingError> {
        self.encoding_error.as_ref()
    }

    pub const fn raw_os_error(&self) -> Option<i32> {
        self.raw_os_error
    }
}

fn map_native_code(code: NativePublicationErrorCode) -> ArtifactSinkErrorCode {
    match code {
        NativePublicationErrorCode::InvalidTarget => ArtifactSinkErrorCode::InvalidTarget,
        NativePublicationErrorCode::UnsupportedPlatform => {
            ArtifactSinkErrorCode::UnsupportedPlatform
        }
        NativePublicationErrorCode::UnsupportedFilesystem => {
            ArtifactSinkErrorCode::UnsupportedFilesystem
        }
        NativePublicationErrorCode::ParentMissing => ArtifactSinkErrorCode::ParentMissing,
        NativePublicationErrorCode::ParentNotDirectory => ArtifactSinkErrorCode::ParentNotDirectory,
        NativePublicationErrorCode::ReparsePointRejected => {
            ArtifactSinkErrorCode::ReparsePointRejected
        }
        NativePublicationErrorCode::TargetExists => ArtifactSinkErrorCode::TargetExists,
        NativePublicationErrorCode::StagingFailed => ArtifactSinkErrorCode::StagingFailed,
        NativePublicationErrorCode::WriteFailed => ArtifactSinkErrorCode::WriteFailed,
        NativePublicationErrorCode::SizeMismatch => ArtifactSinkErrorCode::SizeMismatch,
        NativePublicationErrorCode::Cancelled => ArtifactSinkErrorCode::Cancelled,
        NativePublicationErrorCode::PreCommitSyncFailed => {
            ArtifactSinkErrorCode::PreCommitSyncFailed
        }
        NativePublicationErrorCode::PublicationFailed => ArtifactSinkErrorCode::PublicationFailed,
        NativePublicationErrorCode::CleanupFailed => ArtifactSinkErrorCode::CleanupFailed,
    }
}

impl fmt::Display for ArtifactSinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl std::error::Error for ArtifactSinkError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::solution_set_encoder::{
        LimitIgnoringEncoder, SameLengthForgedCompactEncoder,
    };
    use crate::artifact::{
        CompactSolutionSetEncoder, SolutionArtifactAnnotation, SolutionArtifactEntry,
        SolutionSetArtifact,
    };

    fn artifact() -> SolutionSetArtifact {
        SolutionSetArtifact::try_new(
            "test-solution-set",
            "key-v1",
            "hash-v1",
            "hash:1",
            1,
            vec![
                SolutionArtifactEntry::try_new("solution-a", SolutionArtifactAnnotation::new())
                    .expect("entry"),
            ],
        )
        .expect("artifact")
    }

    #[test]
    fn memory_sink_is_explicitly_bounded_and_commits_only_exact_streamed_bytes() {
        assert_eq!(
            MemoryArtifactSink::try_new(MAX_IN_MEMORY_ARTIFACT_BYTES + 1)
                .expect_err("unbounded memory sink rejected")
                .code(),
            ArtifactSinkErrorCode::CapacityExceeded
        );
        let source = artifact();
        let mut sink = MemoryArtifactSink::try_new(4_096).expect("bounded sink");
        let outcome = sink
            .publish_uncancelled(&CompactSolutionSetEncoder, &source)
            .expect("commit");
        let ArtifactPublicationOutcome::Committed(commit) = outcome else {
            panic!("memory commit must be certain");
        };
        assert_eq!(commit.schema(), "solution-set-artifact.v1");
        assert_eq!(commit.byte_count(), sink.bytes().len() as u64);
        assert!(!commit.target_owned());
        assert_eq!(sink.commit(), Some(&commit));
    }

    #[test]
    fn memory_limit_is_checked_before_any_output_bytes_are_written() {
        let source = artifact();
        let mut sink = MemoryArtifactSink::try_new(16).expect("bounded sink");
        let error = sink
            .publish_uncancelled(&CompactSolutionSetEncoder, &source)
            .expect_err("limit");
        assert_eq!(error.code(), ArtifactSinkErrorCode::CapacityExceeded);
        assert_eq!(
            error.encoding_error(),
            Some(&SolutionArtifactEncodingError::CapacityExceeded)
        );
        assert!(sink.bytes().is_empty());
        assert!(sink.commit().is_none());
    }

    struct CancelAt(PublicationCheckpoint);

    impl PublicationControl for CancelAt {
        fn cancelled_at(&self, checkpoint: PublicationCheckpoint) -> bool {
            checkpoint == self.0
        }
    }

    #[test]
    fn cancelled_memory_stream_clears_partial_bytes_and_commit_state() {
        let source = artifact();
        let mut sink = MemoryArtifactSink::try_new(4_096).expect("bounded sink");
        let error = sink
            .publish(
                &CompactSolutionSetEncoder,
                &source,
                4_096,
                &CancelAt(PublicationCheckpoint::EncodingProgress { completed_units: 0 }),
            )
            .expect_err("cancelled");
        assert_eq!(error.code(), ArtifactSinkErrorCode::Cancelled);
        assert_eq!(
            error.encoding_error(),
            Some(&SolutionArtifactEncodingError::Cancelled)
        );
        assert!(sink.bytes().is_empty());
        assert!(sink.commit().is_none());
    }

    #[test]
    fn same_length_forged_encoder_cannot_commit_memory_bytes_or_metadata() {
        let source = artifact();
        let mut sink = MemoryArtifactSink::try_new(4_096).expect("bounded sink");
        sink.publish_uncancelled(&CompactSolutionSetEncoder, &source)
            .expect("seed honest commit");
        assert!(!sink.bytes().is_empty());
        assert!(sink.commit().is_some());
        let error = sink
            .publish_uncancelled(&SameLengthForgedCompactEncoder, &source)
            .expect_err("forged bytes must fail independent sink verification");
        assert_eq!(error.code(), ArtifactSinkErrorCode::EncodingFailed);
        assert_eq!(
            error.encoding_error(),
            Some(&SolutionArtifactEncodingError::StreamVerificationFailed)
        );
        assert!(sink.bytes().is_empty());
        assert!(sink.commit().is_none());
        assert!(error.residue().is_none());
    }

    #[test]
    fn limit_ignoring_encoder_cannot_reserve_write_or_commit_memory_output() {
        let source = artifact();
        let mut sink = MemoryArtifactSink::try_new(1).expect("bounded sink");
        let error = sink
            .publish_uncancelled(&LimitIgnoringEncoder, &source)
            .expect_err("sink must independently enforce its effective limit");

        assert_eq!(error.code(), ArtifactSinkErrorCode::CapacityExceeded);
        assert_eq!(
            error.encoding_error(),
            Some(&SolutionArtifactEncodingError::CapacityExceeded)
        );
        assert!(sink.bytes().is_empty());
        assert!(sink.commit().is_none());
        assert!(error.residue().is_none());
    }
}
