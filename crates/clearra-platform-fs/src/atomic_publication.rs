use std::{io, io::Write, path::Path};

use crate::{
    error::{DurabilityUncertainReason, NativePublicationError, NativePublicationErrorCode},
    FileIdentity,
};

#[cfg(target_os = "linux")]
use crate::linux as platform;
#[cfg(not(any(target_os = "linux", windows)))]
use crate::unsupported as platform;
#[cfg(windows)]
use crate::windows as platform;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationCheckpoint {
    BeforeMeasure,
    MeasuringProgress { completed_units: u64 },
    BeforeStage,
    BeforeEncoding,
    EncodingProgress { completed_units: u64 },
    BeforeCommitBarrier,
}

pub trait PublicationControl {
    fn cancelled_at(&self, checkpoint: PublicationCheckpoint) -> bool;
}

/// Recovery authority retained while a staged file is still pre-commit.
///
/// Both supported backends bind cleanup to the staged file object rather than
/// to a later pathname lookup. At most one explicitly identified staging leaf
/// can require operator action if the operating system rejects exact-handle
/// cleanup before this contract is established.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreCommitCrashRecovery {
    UnnamedFileReleasedOnLastHandleClose,
    DeleteOnCloseHandle {
        staging_leaf: String,
        file_identity: FileIdentity,
    },
}

/// Recovery rule after the commit-wins barrier has been crossed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostCommitCrashRecovery {
    /// The final target may already be owned. Recovery must inspect the typed
    /// receipt or target identity and must never automatically delete by path.
    TargetMayBeOwnedNeverPathDelete,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NeverCancelled;

impl PublicationControl for NeverCancelled {
    fn cancelled_at(&self, _checkpoint: PublicationCheckpoint) -> bool {
        false
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFileCommit {
    identity: FileIdentity,
    byte_count: u64,
}

impl NativeFileCommit {
    #[cfg(any(target_os = "linux", windows))]
    pub(crate) const fn new(identity: FileIdentity, byte_count: u64) -> Self {
        Self {
            identity,
            byte_count,
        }
    }

    pub const fn identity(&self) -> &FileIdentity {
        &self.identity
    }

    pub const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub const fn target_owned(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativePublicationOutcome {
    Committed(NativeFileCommit),
    DurabilityUncertain {
        commit: NativeFileCommit,
        reason: DurabilityUncertainReason,
    },
}

impl NativePublicationOutcome {
    pub const fn commit(&self) -> &NativeFileCommit {
        match self {
            Self::Committed(commit) | Self::DurabilityUncertain { commit, .. } => commit,
        }
    }

    pub const fn is_committed(&self) -> bool {
        matches!(self, Self::Committed(_))
    }
}

#[derive(Debug)]
pub struct AtomicFileStage {
    inner: Option<platform::StagedFile>,
    expected_bytes: u64,
    written_bytes: u64,
}

impl AtomicFileStage {
    pub fn begin(
        target: &Path,
        expected_bytes: u64,
        control: &dyn PublicationControl,
    ) -> Result<Self, NativePublicationError> {
        if control.cancelled_at(PublicationCheckpoint::BeforeStage) {
            return Err(NativePublicationError::new(
                NativePublicationErrorCode::Cancelled,
            ));
        }
        let inner = platform::StagedFile::begin(target)?;
        Ok(Self {
            inner: Some(inner),
            expected_bytes,
            written_bytes: 0,
        })
    }

    pub const fn expected_bytes(&self) -> u64 {
        self.expected_bytes
    }

    pub const fn written_bytes(&self) -> u64 {
        self.written_bytes
    }

    pub fn precommit_crash_recovery(&self) -> Option<PreCommitCrashRecovery> {
        self.inner
            .as_ref()
            .map(platform::StagedFile::precommit_crash_recovery)
    }

    pub const fn postcommit_crash_recovery() -> PostCommitCrashRecovery {
        PostCommitCrashRecovery::TargetMayBeOwnedNeverPathDelete
    }

    pub fn abort(mut self) -> Result<(), NativePublicationError> {
        let inner = self.inner.take().ok_or_else(|| {
            NativePublicationError::new(NativePublicationErrorCode::CleanupFailed)
        })?;
        inner.abort()
    }

    pub fn commit(
        mut self,
        control: &dyn PublicationControl,
    ) -> Result<NativePublicationOutcome, NativePublicationError> {
        if self.written_bytes != self.expected_bytes {
            return Err(NativePublicationError::new(
                NativePublicationErrorCode::SizeMismatch,
            ));
        }
        if control.cancelled_at(PublicationCheckpoint::BeforeCommitBarrier) {
            return Err(NativePublicationError::new(
                NativePublicationErrorCode::Cancelled,
            ));
        }

        // Commit-wins barrier: the platform commit performs no cancellation
        // checkpoints. Once it linearizes the target name it returns either a
        // durable commit or an ownership-bearing durability-uncertain receipt;
        // it never path-rolls back the published target.
        let inner = self.inner.take().ok_or_else(|| {
            NativePublicationError::new(NativePublicationErrorCode::PublicationFailed)
        })?;
        inner.commit(self.written_bytes)
    }

    fn inner_mut(&mut self) -> io::Result<&mut platform::StagedFile> {
        self.inner
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "artifact stage is closed"))
    }
}

impl Write for AtomicFileStage {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let length = u64::try_from(bytes.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "write length overflow"))?;
        let next = self
            .written_bytes
            .checked_add(length)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "byte count overflow"))?;
        if next > self.expected_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "artifact exceeds its measured byte count",
            ));
        }
        let written = self.inner_mut()?.write(bytes)?;
        self.written_bytes = self
            .written_bytes
            .checked_add(u64::try_from(written).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "written byte count overflow")
            })?)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "byte count overflow"))?;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner_mut()?.flush()
    }
}

impl Drop for AtomicFileStage {
    fn drop(&mut self) {
        // Platform staged handles own their cleanup. Linux staging is unnamed;
        // Windows keeps delete-on-close armed until the commit barrier.
        let _ = self.inner.take();
    }
}

#[cfg(any(target_os = "linux", windows))]
pub(crate) fn committed(identity: FileIdentity, byte_count: u64) -> NativePublicationOutcome {
    NativePublicationOutcome::Committed(NativeFileCommit::new(identity, byte_count))
}

#[cfg(any(target_os = "linux", windows))]
pub(crate) fn durability_uncertain(
    identity: FileIdentity,
    byte_count: u64,
    reason: DurabilityUncertainReason,
) -> NativePublicationOutcome {
    NativePublicationOutcome::DurabilityUncertain {
        commit: NativeFileCommit::new(identity, byte_count),
        reason,
    }
}
