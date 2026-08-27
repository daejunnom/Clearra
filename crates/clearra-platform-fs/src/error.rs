use std::{fmt, io};

use crate::FileIdentity;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativePublicationErrorCode {
    InvalidTarget,
    UnsupportedPlatform,
    UnsupportedFilesystem,
    ParentMissing,
    ParentNotDirectory,
    ReparsePointRejected,
    TargetExists,
    StagingFailed,
    WriteFailed,
    SizeMismatch,
    Cancelled,
    PreCommitSyncFailed,
    PublicationFailed,
    CleanupFailed,
}

impl NativePublicationErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidTarget => "native-artifact-target-invalid",
            Self::UnsupportedPlatform => "native-artifact-platform-unsupported",
            Self::UnsupportedFilesystem => "native-artifact-filesystem-unsupported",
            Self::ParentMissing => "native-artifact-parent-missing",
            Self::ParentNotDirectory => "native-artifact-parent-not-directory",
            Self::ReparsePointRejected => "native-artifact-reparse-point-rejected",
            Self::TargetExists => "native-artifact-target-exists",
            Self::StagingFailed => "native-artifact-staging-failed",
            Self::WriteFailed => "native-artifact-write-failed",
            Self::SizeMismatch => "native-artifact-size-mismatch",
            Self::Cancelled => "native-artifact-cancelled",
            Self::PreCommitSyncFailed => "native-artifact-precommit-sync-failed",
            Self::PublicationFailed => "native-artifact-publication-failed",
            Self::CleanupFailed => "native-artifact-cleanup-failed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublicationResidue {
    None,
    OperatorActionRequired {
        staging_leaf: String,
        file_identity: Option<FileIdentity>,
    },
}

impl PublicationResidue {
    pub const fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativePublicationError {
    code: NativePublicationErrorCode,
    residue: PublicationResidue,
    raw_os_error: Option<i32>,
}

impl NativePublicationError {
    pub const fn new(code: NativePublicationErrorCode) -> Self {
        Self {
            code,
            residue: PublicationResidue::None,
            raw_os_error: None,
        }
    }

    pub(crate) fn from_io(code: NativePublicationErrorCode, error: &io::Error) -> Self {
        Self {
            code,
            residue: PublicationResidue::None,
            raw_os_error: error.raw_os_error(),
        }
    }

    pub(crate) fn with_residue(mut self, residue: PublicationResidue) -> Self {
        self.residue = residue;
        self
    }

    pub const fn code(&self) -> NativePublicationErrorCode {
        self.code
    }

    pub const fn residue(&self) -> &PublicationResidue {
        &self.residue
    }

    pub const fn raw_os_error(&self) -> Option<i32> {
        self.raw_os_error
    }
}

impl fmt::Display for NativePublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl std::error::Error for NativePublicationError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DurabilityUncertainReason {
    PostPublishFileSyncFailed,
    PostPublishParentSyncFailed,
    PersistenceDisarmFailed,
}

impl DurabilityUncertainReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PostPublishFileSyncFailed => "postpublish-file-sync-failed",
            Self::PostPublishParentSyncFailed => "postpublish-parent-sync-failed",
            Self::PersistenceDisarmFailed => "publication-persistence-disarm-failed",
        }
    }
}
