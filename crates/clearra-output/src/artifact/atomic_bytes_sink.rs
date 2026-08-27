use std::{
    io::Write,
    path::{Path, PathBuf},
};

use clearra_platform_fs::{AtomicFileStage, NativePublicationOutcome, PublicationControl};

use super::{ArtifactSinkError, ArtifactSinkErrorCode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ByteArtifactCommit {
    byte_count: u64,
    file_identity: clearra_platform_fs::FileIdentity,
}

impl ByteArtifactCommit {
    pub const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub const fn target_owned(&self) -> bool {
        true
    }

    pub const fn file_identity(&self) -> &clearra_platform_fs::FileIdentity {
        &self.file_identity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ByteArtifactPublicationOutcome {
    Committed(ByteArtifactCommit),
    DurabilityUncertain {
        commit: ByteArtifactCommit,
        reason: clearra_platform_fs::DurabilityUncertainReason,
    },
}

impl ByteArtifactPublicationOutcome {
    pub const fn commit(&self) -> &ByteArtifactCommit {
        match self {
            Self::Committed(commit) | Self::DurabilityUncertain { commit, .. } => commit,
        }
    }
}

/// Atomic-new/no-overwrite sink for already materialized, typed public bytes.
/// Encoding and checksum authority remain with the producing command; this
/// sink owns only native publication and the exact commit receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AtomicBytesArtifactSink {
    target: PathBuf,
}

impl AtomicBytesArtifactSink {
    pub fn new(target: impl Into<PathBuf>) -> Self {
        Self {
            target: target.into(),
        }
    }

    pub fn target(&self) -> &Path {
        &self.target
    }

    pub fn publish(
        &mut self,
        bytes: &[u8],
        maximum_bytes: u64,
        control: &dyn PublicationControl,
    ) -> Result<ByteArtifactPublicationOutcome, ArtifactSinkError> {
        let byte_count = u64::try_from(bytes.len())
            .map_err(|_| ArtifactSinkError::new(ArtifactSinkErrorCode::CapacityExceeded))?;
        if byte_count > maximum_bytes {
            return Err(ArtifactSinkError::new(
                ArtifactSinkErrorCode::CapacityExceeded,
            ));
        }
        let mut stage = AtomicFileStage::begin(&self.target, byte_count, control)
            .map_err(ArtifactSinkError::from_native)?;
        if stage.write_all(bytes).and_then(|()| stage.flush()).is_err() {
            return Err(abort_after_write_error(stage));
        }
        let native = stage
            .commit(control)
            .map_err(ArtifactSinkError::from_native)?;
        let commit = ByteArtifactCommit {
            byte_count: native.commit().byte_count(),
            file_identity: native.commit().identity().clone(),
        };
        Ok(match native {
            NativePublicationOutcome::Committed(_) => {
                ByteArtifactPublicationOutcome::Committed(commit)
            }
            NativePublicationOutcome::DurabilityUncertain { reason, .. } => {
                ByteArtifactPublicationOutcome::DurabilityUncertain { commit, reason }
            }
        })
    }
}

fn abort_after_write_error(stage: AtomicFileStage) -> ArtifactSinkError {
    match stage.abort() {
        Ok(()) => ArtifactSinkError::new(ArtifactSinkErrorCode::WriteFailed),
        Err(cleanup) => ArtifactSinkError::from_native(cleanup),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use clearra_platform_fs::NeverCancelled;

    use super::*;

    #[test]
    fn publishes_exact_bytes_once_and_refuses_overwrite() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "clearra-output-typed-bytes-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("directory");
        let target = directory.join("document.txt");
        let mut sink = AtomicBytesArtifactSink::new(&target);
        let outcome = match sink.publish(b"v115@test", 1024, &NeverCancelled) {
            Ok(outcome) => outcome,
            Err(error)
                if cfg!(target_os = "linux")
                    && error.code() == ArtifactSinkErrorCode::UnsupportedFilesystem =>
            {
                fs::remove_dir(&directory).expect("unsupported cleanup");
                return;
            }
            Err(error) => panic!("publish: {error:?}"),
        };
        assert_eq!(outcome.commit().byte_count(), 9);
        assert!(outcome.commit().target_owned());
        assert_eq!(fs::read(&target).expect("bytes"), b"v115@test");
        assert_eq!(
            sink.publish(b"replacement", 1024, &NeverCancelled)
                .expect_err("no overwrite")
                .code(),
            ArtifactSinkErrorCode::TargetExists
        );
        fs::remove_file(&target).expect("target cleanup");
        fs::remove_dir(&directory).expect("directory cleanup");
    }
}
